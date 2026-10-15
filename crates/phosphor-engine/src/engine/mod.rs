//! The render graph: sources → per-layer effect chains → blended layer stack → output texture.
//! Everything for a frame is recorded into one command encoder and submitted once.

use std::collections::{HashMap, HashSet};

use crate::capture::{Capture, CapturedFrame};
use crate::control::{ModInputs, modulate, resolve_params};
use crate::gpu::{Gpu, Target, Uniforms};
use crate::mapping;
use crate::model::{BlendMode, Composition, EffectState, FitMode, LayerGroup, LayerState, SourceSpec};
use crate::shaders;

/// Media uploaded by the app for a layer.
enum Media {
    Rgba(Target),
    /// Bi-planar video frame: luma (R8/R16) + interleaved chroma (RG8/RG16); TV range unless `full_range`.
    Nv12 { luma: Target, chroma: Target, full_range: bool },
    /// Block-compressed frame straight from a Hap movie (BC1/BC3); `ycocg` = Hap Q colour.
    Compressed { target: Target, format: CompressedFormat },
}

/// GPU block-compressed picture formats (Hap): the GPU samples them directly, no CPU decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressedFormat {
    /// DXT1: Hap.
    Bc1,
    /// DXT5: Hap Alpha.
    Bc3,
    /// DXT5 holding scaled YCoCg: Hap Q (converted to RGB in the shader).
    Bc3YCoCg,
    /// BPTC: Hap R.
    Bc7,
}

impl CompressedFormat {
    pub fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            CompressedFormat::Bc1 => wgpu::TextureFormat::Bc1RgbaUnorm,
            CompressedFormat::Bc3 | CompressedFormat::Bc3YCoCg => wgpu::TextureFormat::Bc3RgbaUnorm,
            CompressedFormat::Bc7 => wgpu::TextureFormat::Bc7RgbaUnorm,
        }
    }

    /// Bytes per 4×4 block.
    pub fn block_bytes(self) -> u32 {
        if self == CompressedFormat::Bc1 { 8 } else { 16 }
    }
}

#[derive(Default)]
struct LayerResources {
    generator: Option<Target>,
    /// Outgoing source during a transition.
    generator_out: Option<Target>,
    media_out: Option<Media>,
    /// RGB copies of incoming/outgoing (when YUV) and the blended result.
    trans: [Option<Target>; 3],
    fx: [Option<Target>; 3],
    /// Reduced-size target for Rutt-Etra; its scale sets the apparent line width.
    rutt: Option<Target>,
    /// Previous outputs of history effects (trails), per effect id, ping-ponged by frame parity.
    history: HashMap<u64, [Option<Target>; 2]>,
    /// Multi-pass shader buffers, per effect id (or `GENERATOR_KEY` for the layer's source).
    multipass: HashMap<u64, PassBuffers>,
    media: Option<Media>,
    thumbnail: Option<Target>,
    /// Full-size picture of the layer alone, while a mapping surface shows it.
    feed: Option<Target>,
}

/// A multi-pass shader's buffers: two textures each (read one, write the other), and which is
/// the newest. Kept between frames so persistent buffers give feedback.
#[derive(Default)]
struct PassBuffers {
    slots: Vec<[Option<Target>; 2]>,
    front: Vec<usize>,
}

/// `multipass` keys for a layer's generator source (and its outgoing copy in transitions).
const GENERATOR_KEY: u64 = u64::MAX;
const GENERATOR_OUT_KEY: u64 = u64::MAX - 1;

/// Width of the per-layer thumbnails; height follows the canvas aspect.
pub const THUMBNAIL_WIDTH: u32 = 240;

/// Resource slot for the master effect chain (never a real layer id).
pub const MASTER_ID: u64 = u64::MAX;

/// Resource slots at or above this id belong to the engine (master chain, explorer previews).
const RESERVED_IDS: u64 = u64::MAX - 100_000;

/// Width of explorer preview tiles; height follows the canvas aspect.
pub const PREVIEW_WIDTH: u32 = 192;

/// Something to render as a small live preview in the effects explorer.
#[derive(Clone, Debug)]
pub enum PreviewItem {
    Effect(crate::model::EffectState),
    Generator(String),
}

/// A frame source after it has been resolved for this frame.
#[derive(Clone)]
enum Frame {
    Rgb { view: wgpu::TextureView, w: u32, h: u32 },
    Yuv { luma: wgpu::TextureView, chroma: wgpu::TextureView, w: u32, h: u32, full_range: bool },
    /// Hap Q: scaled YCoCg in a BC3 texture.
    YCoCg { view: wgpu::TextureView, w: u32, h: u32 },
}

impl Frame {
    fn size(&self) -> (u32, u32) {
        match self {
            Frame::Rgb { w, h, .. } | Frame::Yuv { w, h, .. } | Frame::YCoCg { w, h, .. } => (*w, *h),
        }
    }

    /// What a pass samples for this frame: (tex0, tex2, `Uniforms::yuv` conversion: 0 RGB, 1 NV12 TV range,
    /// 2 Hap Q YCoCg, 3 NV12 full range).
    /// Every shader reading a source goes through `sample_source`, which uses these.
    fn bind(self) -> (wgpu::TextureView, Option<wgpu::TextureView>, u32) {
        match self {
            Frame::Rgb { view, .. } => (view, None, 0),
            Frame::Yuv { luma, chroma, full_range, .. } => (luma, Some(chroma), if full_range { 3 } else { 1 }),
            Frame::YCoCg { view, .. } => (view, None, 2),
        }
    }
}

struct Pass {
    entry: &'static str,
    target: wgpu::TextureView,
    tex0: wgpu::TextureView,
    tex1: Option<wgpu::TextureView>,
    tex2: Option<wgpu::TextureView>,
    uniforms: Uniforms,
    /// Triangles from a vertex buffer (mesh-warped surfaces, masks) instead of generated ones.
    mesh: Option<MeshDraw>,
    /// Multi-pass buffers bound as `buf0…` (bindings 6..); unbound ones are blank.
    bufs: Vec<wgpu::TextureView>,
}

mod encode;
mod fit;
mod media;
mod mesh;
mod passes;
mod previews;

use fit::{fit_transform, push_fit_copy};
use mesh::{MeshCache, MeshDraw, hash_points};

/// Per-frame timing reported back to the app.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameInfo {
    pub passes: usize,
    pub layers_drawn: usize,
}

pub struct Engine {
    gpu: Gpu,
    layers: HashMap<u64, LayerResources>,
    /// Mixing targets, ping-ponged: 0–1 the main stack, 2–3 the layer group being mixed (only while groups exist).
    accum: [Option<Target>; 4],
    output: Option<Target>,
    output_generation: u64,
    uniform_buffer: Option<wgpu::Buffer>,
    uniform_capacity: usize,
    bind_cache: encode::BindCache,
    capture: Capture,
    frame_index: u64,
    /// One target per configured output, at that output's size.
    output_targets: HashMap<u64, Option<Target>>,
    meshes: MeshCache,
    previews: Vec<(PreviewItem, bool)>,
    /// Monitor requests this frame: (layer id or MASTER_ID, width, height).
    monitor_req: Vec<(u64, u32, u32)>,
    monitor_targets: Vec<Option<Target>>,
    preview_source: u64,
    preview_src: Option<Target>,
    preview_targets: Vec<Option<Target>>,
    custom_generation: u64,
    /// Imported shaders waiting to be compiled on first use: entry → WGSL (without prelude).
    pending_modules: HashMap<&'static str, String>,
    /// Shaders that failed to build, by entry, with the error.
    pub failed_modules: HashMap<&'static str, String>,
    compile_budget: u32,
}

impl Engine {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let mut gpu = Gpu::new(device, queue);
        gpu.prewarm();
        Self {
            gpu,
            layers: HashMap::new(),
            accum: [None, None, None, None],
            output: None,
            output_generation: 0,
            uniform_buffer: None,
            uniform_capacity: 0,
            bind_cache: Default::default(),
            capture: Capture::default(),
            frame_index: 0,
            output_targets: HashMap::new(),
            meshes: MeshCache::default(),
            previews: Vec::new(),
            monitor_req: Vec::new(),
            monitor_targets: Vec::new(),
            preview_source: MASTER_ID,
            preview_src: None,
            preview_targets: Vec::new(),
            custom_generation: 0,
            pending_modules: HashMap::new(),
            failed_modules: HashMap::new(),
            compile_budget: 0,
        }
    }

    /// Registers an already-converted shader (e.g. ISF) that is compiled the first time it is
    /// drawn. `wgsl` must define `@fragment fn <entry>(in: VOut)` on top of the common prelude.
    pub fn register_lazy(&mut self, desc: &'static crate::shaders::ShaderDesc, wgsl: String) {
        self.pending_modules.insert(desc.entry, wgsl);
        crate::shaders::register_custom(desc);
    }

    /// True if `entry` is ready to draw. Compiles pending shaders within a small per-frame budget
    /// so browsing a big library never stalls the show.
    fn ensure_module(&mut self, entry: &'static str) -> bool {
        if self.failed_modules.contains_key(entry) {
            return false;
        }
        let Some(src) = self.pending_modules.get(entry) else { return true };
        if self.compile_budget == 0 {
            return false;
        }
        self.compile_budget -= 1;
        let src = src.clone();
        self.pending_modules.remove(entry);
        let full = format!("{}\n{src}", crate::shaders::common_source());
        let prelude_lines = crate::shaders::common_source().lines().count() as u32 + 1;
        if let Err(e) = crate::custom::validate(&full, prelude_lines) {
            self.failed_modules.insert(entry, e.to_string());
            return false;
        }
        let module = self.gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(entry), source: wgpu::ShaderSource::Wgsl(full.into()) });
        self.gpu.custom_modules.insert(entry, module);
        self.gpu.pipeline(entry);
        true
    }

    /// Compiles a user shader (see `custom`). On success it is registered under `id` and can be
    /// used like any built-in effect or generator.
    pub fn compile_custom(&mut self, id: &str, source: &str) -> Result<&'static crate::shaders::ShaderDesc, crate::custom::CompileError> {
        let header = crate::custom::parse_header(source)?;
        self.custom_generation += 1;
        let slug: String = id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        let entry: &'static str = Box::leak(format!("custom_{slug}_{}", self.custom_generation).into_boxed_str());
        let (full, prelude_lines) = crate::custom::module_source(source, entry)?;
        crate::custom::validate(&full, prelude_lines)?;
        let module = self.gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(entry),
            source: wgpu::ShaderSource::Wgsl(full.into()),
        });
        self.gpu.custom_modules.insert(entry, module);
        self.gpu.pipeline(entry);
        let desc: &'static crate::shaders::ShaderDesc = Box::leak(Box::new(crate::custom::make_desc(id, &header, entry)));
        crate::shaders::register_custom(desc);
        Ok(desc)
    }

    /// Asks for previews of effects (applied to `source`: a layer id or `MASTER_ID`) and
    /// generators on the next frame. Items flagged `false` keep last frame's picture and cost
    /// nothing, so a big library can show stills and only animate what you're looking at.
    pub fn request_previews(&mut self, source: u64, items: Vec<(PreviewItem, bool)>) {
        self.preview_source = source;
        self.previews = items;
    }

    /// Asks for monitor pictures on the next frame: each entry is a layer id (its picture after
    /// effects, before blending) or `MASTER_ID` (the mix after master effects, before mapping),
    /// rendered at the given size.
    pub fn request_monitors(&mut self, req: Vec<(u64, u32, u32)>) {
        self.monitor_req = req;
    }

    pub fn monitor(&self, index: usize) -> Option<&Target> {
        self.monitor_targets.get(index)?.as_ref()
    }

    pub fn preview(&self, index: usize) -> Option<&Target> {
        self.preview_targets.get(index)?.as_ref()
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.gpu.device
    }

    /// The final composited frame. Its identity changes only when the resolution does;
    /// compare `output_generation()` to know when to re-register it with a UI.
    pub fn output(&self) -> Option<&Target> {
        self.output.as_ref()
    }

    /// The picture for one configured output (after its mapping).
    pub fn output_texture(&self, id: u64) -> Option<&Target> {
        self.output_targets.get(&id)?.as_ref()
    }

    pub fn output_generation(&self) -> u64 {
        self.output_generation
    }

    /// Records and submits one frame. `time` is in seconds and drives animated shaders;
    /// `mods` carries this frame's audio/LFO signals for modulated parameters.
    pub fn render(&mut self, comp: &Composition, time: f32, mods: &ModInputs) -> FrameInfo {
        let (w, h) = (comp.width.max(16), comp.height.max(16));
        let device = self.gpu.device.clone();
        self.compile_budget = 2;

        // Drop GPU resources for layers that no longer exist.
        let live: HashSet<u64> = comp.layers.iter().map(|l| l.id).chain(comp.groups.iter().map(|g| g.id)).collect();
        self.layers.retain(|id, _| *id >= RESERVED_IDS || live.contains(id));

        for (i, slot) in self.accum.iter_mut().enumerate() {
            if i < 2 || !comp.groups.is_empty() {
                Target::ensure(slot, &device, "accum", w, h);
            } else {
                *slot = None;
            }
        }
        let old_output = self.output.as_ref().map(|t| (t.width, t.height));
        Target::ensure(&mut self.output, &device, "output", w, h);
        if old_output != Some((w, h)) {
            self.output_generation += 1;
        }

        let mut passes: Vec<Pass> = Vec::new();
        let mut current: Option<usize> = None; // which accum holds the stack so far
        let mut layers_drawn = 0;

        let thumb_h = (THUMBNAIL_WIDTH * h / w).max(1);
        let monitors = std::mem::take(&mut self.monitor_req);
        if self.monitor_targets.len() < monitors.len() {
            self.monitor_targets.resize_with(monitors.len(), || None);
        }
        let any_solo = comp.layers.iter().any(|l| l.enabled && l.solo);
        // Layers shown on their own by a mapping surface, and their pictures this frame.
        let wanted: HashSet<u64> = comp.outputs.iter().filter(|o| o.enabled).flat_map(|o| o.mapping.layer_feeds()).collect();
        let mut feeds: HashMap<u64, wgpu::TextureView> = HashMap::new();
        // The layer group being mixed (into accum 2–3) and which of those holds it so far.
        let mut open_group: Option<u64> = None;
        let mut group_current: Option<usize> = None;
        let mut ctx = MixCtx { canvas: (w, h), time, mods, thumb_h, wanted: &wanted, layers: &comp.layers };
        for layer in &comp.layers {
            // Leaving a group's members: finish its picture and mix it in like one layer.
            let gid = layer.parent.filter(|p| comp.group(*p).is_some());
            if open_group.is_some() && open_group != gid {
                let g = comp.group(open_group.take().unwrap()).unwrap();
                self.finish_group(g, group_current.take(), &mut current, &mut feeds, &mut ctx, &mut passes);
            }
            if open_group.is_none() && gid.is_some() {
                open_group = gid;
            }
            // Faded-out layers still render (into their thumbnail) so they can be cued before going live.
            if !layer.enabled {
                continue;
            }
            let incoming = self.resolve_spec(layer.id, &layer.source, &layer.generator_values, &layer.generator_mods, false, w, h, time, mods, &mut passes);
            // A stopped clip fades out to nothing over its transition.
            let fading_out = incoming.is_none() && layer.transition.as_ref().is_some_and(|t| t.progress(time) < 1.0);
            let Some(mut frame) = incoming.or_else(|| fading_out.then(|| Frame::Rgb { view: self.gpu.blank.clone(), w: 1, h: 1 })) else {
                continue;
            };
            match &layer.transition {
                Some(t) if t.progress(time) < 1.0 => {
                    if let Some(out) = self.resolve_spec(layer.id, &t.from, &t.from_values, &[], true, w, h, time, mods, &mut passes) {
                        frame = self.crossfade(layer.id, frame, out, t, time, layer.fit, (w, h), &mut passes);
                    }
                }
                _ => {
                    if let Some(res) = self.layers.get_mut(&layer.id) {
                        res.generator_out = None;
                        res.media_out = None;
                    }
                }
            }
            let frame = if layer.fx_bypass { frame } else { self.apply_effects(layer.id, &layer.effects, frame, time, mods, &comp.layers, &mut passes) };
            self.push_thumbnail(layer.id, layer.fit, &frame, (w, h), thumb_h, &mut passes);
            if wanted.contains(&layer.id) {
                let res = self.layers.entry(layer.id).or_default();
                Target::ensure(&mut res.feed, &device, "layer feed", w, h);
                let view = res.feed.as_ref().unwrap().view.clone();
                push_fit_copy(view.clone(), &frame, (w, h), layer.fit, (w, h), &mut passes);
                feeds.insert(layer.id, view);
            } else if let Some(res) = self.layers.get_mut(&layer.id) {
                res.feed = None;
            }
            for i in 0..monitors.len() {
                let (src, mw, mh) = monitors[i];
                if src == layer.id {
                    Target::ensure(&mut self.monitor_targets[i], &self.gpu.device, "monitor", mw, mh);
                    let view = self.monitor_targets[i].as_ref().unwrap().view.clone();
                    push_fit_copy(view, &frame, (w, h), layer.fit, (mw, mh), &mut passes);
                }
            }
            let muted = (any_solo && !layer.solo) || !layer.in_mix;
            let opacity = if muted { 0.0 } else { modulate(layer.opacity, 0.0, 1.0, layer.opacity_mod.as_ref(), mods) * layer.group.gain(comp.crossfader) };
            if opacity <= 0.001 {
                continue;
            }

            if open_group.is_some() {
                self.push_blend(frame, layer.fit, opacity, layer.blend, 2, &mut group_current, (w, h), time, &mut passes);
            } else {
                self.push_blend(frame, layer.fit, opacity, layer.blend, 0, &mut current, (w, h), time, &mut passes);
            }
            layers_drawn += 1;
        }
        if let Some(g) = open_group.and_then(|id| comp.group(id)) {
            self.finish_group(g, group_current.take(), &mut current, &mut feeds, &mut ctx, &mut passes);
        }
        let _ = &mut ctx;

        let output_view = self.output.as_ref().unwrap().view.clone();
        let mut master = current.map_or(self.gpu.blank.clone(), |c| self.accum[c].as_ref().unwrap().view.clone());
        if current.is_some() && !comp.master_fx_bypass && comp.master_effects.iter().any(|e| e.enabled) {
            let frame = Frame::Rgb { view: master.clone(), w, h };
            if let Frame::Rgb { view, .. } = self.apply_effects(MASTER_ID, &comp.master_effects, frame, time, mods, &comp.layers, &mut passes) {
                master = view;
            }
        }
        for (i, (src, mw, mh)) in monitors.iter().copied().enumerate() {
            if src == MASTER_ID {
                Target::ensure(&mut self.monitor_targets[i], &self.gpu.device, "monitor", mw, mh);
                let mut u = Uniforms::with_params(&[]);
                u.resolution = [mw as f32, mh as f32];
                u.opacity = 1.0;
                let target = self.monitor_targets[i].as_ref().unwrap().view.clone();
                passes.push(Pass { entry: "fs_finalize", target, tex0: master.clone(), tex1: None, tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
            }
        }
        self.render_previews(&master, (w, h), time, mods, &comp.layers, &mut passes);
        let fade = modulate(comp.master_fade, 0.0, 1.0, comp.master_fade_mod.as_ref(), mods);
        // Program feed: the mix with the master fader (preview, streaming, snapshots).
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [w as f32, h as f32];
        u.opacity = fade;
        passes.push(Pass { entry: "fs_finalize", target: output_view, tex0: master.clone(), tex1: None, tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });

        // Each output: its own size and mapping surfaces.
        for o in comp.outputs.iter().filter(|o| o.enabled) {
            let (ow, oh) = o.size((w, h));
            let (ow, oh) = (ow.max(16), oh.max(16));
            let slot = self.output_targets.entry(o.id).or_insert(None);
            Target::ensure(slot, &device, "output", ow, oh);
            let view = slot.as_ref().unwrap().view.clone();
            let mut u = Uniforms::with_params(&[]);
            u.resolution = [ow as f32, oh as f32];
            let [g0, g1] = o.color.uniforms();
            u.p[5] = g0;
            u.p[6] = g1;
            if !o.mapping.active() {
                u.opacity = fade;
                passes.push(Pass { entry: "fs_finalize", target: view.clone(), tex0: master.clone(), tex1: None, tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
                self.push_masks(o.id, &o.mapping, &view, (ow, oh), &mut passes);
                continue;
            }
            // Black, then each enabled surface blended on top.
            u.opacity = 0.0;
            passes.push(Pass { entry: "fs_finalize", target: view.clone(), tex0: self.gpu.blank.clone(), tex1: None, tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
            for s in o.mapping.surfaces.iter().filter(|s| s.enabled) {
                // A layer's own picture (black while that layer is off); the mix if the layer is gone.
                let src = match s.layer {
                    Some(id) if comp.layers.iter().any(|l| l.id == id) || comp.group(id).is_some() => feeds.get(&id).cloned().unwrap_or_else(|| self.gpu.blank.clone()),
                    _ => master.clone(),
                };
                // Mesh warp: one pass drawing the cached, finely subdivided mesh.
                if let Some(m) = &s.mesh {
                    let hash = hash_points(&m.points, &[m.cols, m.rows]);
                    let Some(draw) = self.meshes.get(&device, &self.gpu.queue, (o.id, s.id), hash, || m.triangles()) else { continue };
                    let mut u = Uniforms::with_params(&[]);
                    u.resolution = [ow as f32, oh as f32];
                    u.opacity = s.opacity * fade;
                    u.uv_xform = [s.input[2], s.input[3], s.input[0], s.input[1]];
                    u.mode = o.mapping.test_pattern as u32;
                    u.p[4] = [0.0, s.feather, s.blend_gamma - 1.0, 0.0];
                    u.p[5] = g0;
                    u.p[6] = g1;
                    [u.p[7], u.p[8]] = s.color.uniforms();
                    passes.push(Pass { entry: "fs_mesh", target: view.clone(), tex0: src.clone(), tex1: None, tex2: None, uniforms: u, mesh: Some(draw), bufs: Vec::new() });
                    continue;
                }
                let Some(inv) = mapping::quad_to_square(&s.corners) else { continue };
                let c = &s.corners;
                let mut u = Uniforms::with_params(&[]);
                u.resolution = [ow as f32, oh as f32];
                u.opacity = s.opacity * fade;
                u.uv_xform = [s.input[2], s.input[3], s.input[0], s.input[1]];
                u.mode = o.mapping.test_pattern as u32;
                u.p[0] = [c[0][0], c[0][1], c[1][0], c[1][1]];
                u.p[1] = [c[2][0], c[2][1], c[3][0], c[3][1]];
                u.p[2] = [inv[0], inv[1], inv[2], inv[3]];
                u.p[3] = [inv[4], inv[5], inv[6], inv[7]];
                u.p[4] = [inv[8], s.feather, s.blend_gamma - 1.0, 0.0];
                u.p[5] = g0;
                u.p[6] = g1;
                [u.p[7], u.p[8]] = s.color.uniforms();
                passes.push(Pass { entry: "fs_surface", target: view.clone(), tex0: src.clone(), tex1: None, tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
            }
            self.push_masks(o.id, &o.mapping, &view, (ow, oh), &mut passes);
        }
        self.output_targets.retain(|id, _| comp.outputs.iter().any(|o| o.id == *id && o.enabled));
        self.meshes.sweep();

        self.encode(&passes);
        self.frame_index += 1;
        FrameInfo { passes: passes.len(), layers_drawn }
    }
}

/// What mixing a stack needs besides the engine (so helpers don't take ten arguments).
struct MixCtx<'a> {
    canvas: (u32, u32),
    pub time: f32,
    pub mods: &'a ModInputs,
    pub thumb_h: u32,
    /// Layer/group ids a mapping surface shows on their own.
    pub wanted: &'a HashSet<u64>,
    pub layers: &'a [LayerState],
}

impl Engine {
    /// Blends `frame` onto a stack (accum `base`/`base + 1`, ping-ponged; `current` = which holds it so far).
    #[allow(clippy::too_many_arguments)]
    fn push_blend(&self, frame: Frame, fit: FitMode, opacity: f32, blend: BlendMode, base: usize, current: &mut Option<usize>, canvas: (u32, u32), time: f32, passes: &mut Vec<Pass>) {
        let next = current.map_or(0, |c| 1 - c);
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [canvas.0 as f32, canvas.1 as f32];
        u.time = time;
        u.opacity = opacity;
        u.mode = blend.shader_index();
        u.uv_xform = fit_transform(frame.size(), canvas, fit);
        let (tex0, tex2, yuv) = frame.bind();
        u.yuv = yuv;
        passes.push(Pass {
            entry: "fs_composite",
            target: self.accum[base + next].as_ref().unwrap().view.clone(),
            tex0,
            tex1: current.map(|c| self.accum[base + c].as_ref().unwrap().view.clone()),
            tex2,
            uniforms: u,
            mesh: None,
            bufs: Vec::new(),
        });
        *current = Some(next);
    }

    /// A layer group's members are mixed (in accum 2–3): run the group's effects, update its thumbnail and
    /// surface feed, and blend it onto the main stack like one layer.
    fn finish_group(&mut self, g: &LayerGroup, mixed: Option<usize>, current: &mut Option<usize>, feeds: &mut HashMap<u64, wgpu::TextureView>, ctx: &mut MixCtx, passes: &mut Vec<Pass>) {
        let Some(c) = mixed else { return };
        let (w, h) = ctx.canvas;
        let frame = Frame::Rgb { view: self.accum[2 + c].as_ref().unwrap().view.clone(), w, h };
        let frame = if g.fx_bypass { frame } else { self.apply_effects(g.id, &g.effects, frame, ctx.time, ctx.mods, ctx.layers, passes) };
        self.push_thumbnail(g.id, FitMode::Stretch, &frame, ctx.canvas, ctx.thumb_h, passes);
        let res = self.layers.entry(g.id).or_default();
        if ctx.wanted.contains(&g.id) {
            Target::ensure(&mut res.feed, &self.gpu.device, "group feed", w, h);
            let view = res.feed.as_ref().unwrap().view.clone();
            push_fit_copy(view.clone(), &frame, ctx.canvas, FitMode::Stretch, ctx.canvas, passes);
            feeds.insert(g.id, view);
        } else {
            res.feed = None;
        }
        let opacity = modulate(g.opacity, 0.0, 1.0, g.opacity_mod.as_ref(), ctx.mods);
        if g.enabled && g.in_mix && opacity > 0.001 {
            self.push_blend(frame, FitMode::Stretch, opacity, g.blend, 0, current, ctx.canvas, ctx.time, passes);
        }
    }
}
