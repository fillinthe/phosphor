//! The render graph: sources → per-layer effect chains → blended layer stack → output texture.
//! Everything for a frame is recorded into one command encoder and submitted once.

use std::collections::{HashMap, HashSet};

use crate::capture::{Capture, CapturedFrame};
use crate::control::{ModInputs, modulate, resolve_params};
use crate::gpu::{Gpu, Target, Uniforms};
use crate::mapping;
use crate::model::{Composition, EffectState, FitMode, LayerState, SourceSpec};
use crate::shaders;

/// Media uploaded by the app for a layer.
enum Media {
    Rgba(Target),
    /// Bi-planar video frame: luma (R8) + interleaved chroma (RG8).
    Nv12 { luma: Target, chroma: Target },
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
    media: Option<Media>,
    thumbnail: Option<Target>,
}

/// Width of the per-layer thumbnails; height follows the canvas aspect.
pub const THUMBNAIL_WIDTH: u32 = 240;

/// Resource slot for the master effect chain (never a real layer id).
pub const MASTER_ID: u64 = u64::MAX;

/// A frame source after it has been resolved for this frame.
#[derive(Clone)]
enum Frame {
    Rgb { view: wgpu::TextureView, w: u32, h: u32 },
    Yuv { luma: wgpu::TextureView, chroma: wgpu::TextureView, w: u32, h: u32 },
}

impl Frame {
    fn size(&self) -> (u32, u32) {
        match self {
            Frame::Rgb { w, h, .. } | Frame::Yuv { w, h, .. } => (*w, *h),
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
}

/// Per-frame timing reported back to the app.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameInfo {
    pub passes: usize,
    pub layers_drawn: usize,
}

pub struct Engine {
    gpu: Gpu,
    layers: HashMap<u64, LayerResources>,
    accum: [Option<Target>; 2],
    output: Option<Target>,
    output_generation: u64,
    uniform_buffer: Option<wgpu::Buffer>,
    uniform_capacity: usize,
    capture: Capture,
    frame_index: u64,
}

impl Engine {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let mut gpu = Gpu::new(device, queue);
        gpu.prewarm();
        Self {
            gpu,
            layers: HashMap::new(),
            accum: [None, None],
            output: None,
            output_generation: 0,
            uniform_buffer: None,
            uniform_capacity: 0,
            capture: Capture::default(),
            frame_index: 0,
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.gpu.device
    }

    /// The final composited frame. Its identity changes only when the resolution does;
    /// compare `output_generation()` to know when to re-register it with a UI.
    pub fn output(&self) -> Option<&Target> {
        self.output.as_ref()
    }

    pub fn output_generation(&self) -> u64 {
        self.output_generation
    }

    /// Uploads an RGBA8 frame (tightly packed) for a layer: a still image or one video frame.
    /// The texture is reused while the size stays the same.
    pub fn upload_rgba(&mut self, layer: u64, width: u32, height: u32, rgba: &[u8]) {
        self.upload_rgba_slot(layer, false, width, height, rgba);
    }

    /// Same as `upload_rgba`, for the outgoing source of a transition.
    pub fn upload_rgba_outgoing(&mut self, layer: u64, width: u32, height: u32, rgba: &[u8]) {
        self.upload_rgba_slot(layer, true, width, height, rgba);
    }

    fn upload_rgba_slot(&mut self, layer: u64, outgoing: bool, width: u32, height: u32, rgba: &[u8]) {
        let res = self.layers.entry(layer).or_default();
        let slot = if outgoing { &mut res.media_out } else { &mut res.media };
        let reuse = matches!(slot, Some(Media::Rgba(t)) if t.width == width && t.height == height);
        if !reuse {
            *slot = Some(Media::Rgba(Target::new(&self.gpu.device, "media", width, height)));
        }
        let Some(Media::Rgba(target)) = slot else { return };
        self.gpu.queue.write_texture(
            target.texture.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: None },
            target.texture.size(),
        );
    }

    /// Uploads one NV12 (420 bi-planar, video range) video frame for a layer.
    #[allow(clippy::too_many_arguments)]
    pub fn upload_nv12(&mut self, layer: u64, width: u32, height: u32, luma: &[u8], luma_stride: u32, chroma: &[u8], chroma_stride: u32) {
        self.upload_nv12_slot(layer, false, width, height, luma, luma_stride, chroma, chroma_stride);
    }

    /// Same as `upload_nv12`, for the outgoing source of a transition.
    #[allow(clippy::too_many_arguments)]
    pub fn upload_nv12_outgoing(&mut self, layer: u64, width: u32, height: u32, luma: &[u8], luma_stride: u32, chroma: &[u8], chroma_stride: u32) {
        self.upload_nv12_slot(layer, true, width, height, luma, luma_stride, chroma, chroma_stride);
    }

    #[allow(clippy::too_many_arguments)]
    fn upload_nv12_slot(&mut self, layer: u64, outgoing: bool, width: u32, height: u32, luma: &[u8], luma_stride: u32, chroma: &[u8], chroma_stride: u32) {
        let device = &self.gpu.device;
        let res = self.layers.entry(layer).or_default();
        let slot = if outgoing { &mut res.media_out } else { &mut res.media };
        let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
        let reuse = matches!(slot, Some(Media::Nv12 { luma, .. }) if luma.width == width && luma.height == height);
        if !reuse {
            *slot = Some(Media::Nv12 {
                luma: Target::with_format(device, "luma", width, height, wgpu::TextureFormat::R8Unorm),
                chroma: Target::with_format(device, "chroma", cw, ch, wgpu::TextureFormat::Rg8Unorm),
            });
        }
        let Some(Media::Nv12 { luma: lt, chroma: ct }) = slot else { return };
        let write = |t: &Target, data: &[u8], stride: u32| {
            self.gpu.queue.write_texture(
                t.texture.as_image_copy(),
                data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None },
                t.texture.size(),
            );
        };
        write(lt, luma, luma_stride);
        write(ct, chroma, chroma_stride);
    }

    /// Keeps the current media as the outgoing picture of a transition.
    pub fn begin_transition(&mut self, layer: u64) {
        let res = self.layers.entry(layer).or_default();
        res.media_out = res.media.take();
    }

    pub fn clear_media(&mut self, layer: u64) {
        if let Some(res) = self.layers.get_mut(&layer) {
            res.media = None;
            res.media_out = None;
        }
    }

    /// Records and submits one frame. `time` is in seconds and drives animated shaders;
    /// `mods` carries this frame's audio/LFO signals for modulated parameters.
    pub fn render(&mut self, comp: &Composition, time: f32, mods: &ModInputs) -> FrameInfo {
        let (w, h) = (comp.width.max(16), comp.height.max(16));
        let device = self.gpu.device.clone();

        // Drop GPU resources for layers that no longer exist.
        let live: HashSet<u64> = comp.layers.iter().map(|l| l.id).collect();
        self.layers.retain(|id, _| *id == MASTER_ID || live.contains(id));

        for slot in &mut self.accum {
            Target::ensure(slot, &device, "accum", w, h);
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
        let any_solo = comp.layers.iter().any(|l| l.enabled && l.solo);
        for layer in &comp.layers {
            // Faded-out layers still render (into their thumbnail) so they can be cued before going live.
            if !layer.enabled {
                continue;
            }
            let Some(mut frame) =
                self.resolve_spec(layer.id, &layer.source, &layer.generator_values, &layer.generator_mods, false, w, h, time, mods, &mut passes)
            else {
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
            self.push_thumbnail(layer, &frame, (w, h), thumb_h, &mut passes);
            let muted = any_solo && !layer.solo;
            let opacity = if muted { 0.0 } else { modulate(layer.opacity, 0.0, 1.0, layer.opacity_mod.as_ref(), mods) };
            if opacity <= 0.001 {
                continue;
            }

            let next = current.map_or(0, |c| 1 - c);
            let (sw, sh) = frame.size();
            let mut u = Uniforms::with_params(&[]);
            u.resolution = [w as f32, h as f32];
            u.time = time;
            u.opacity = opacity;
            u.mode = layer.blend.shader_index();
            u.uv_xform = fit_transform((sw, sh), (w, h), layer.fit);
            let (tex0, tex2) = match frame {
                Frame::Rgb { view, .. } => (view, None),
                Frame::Yuv { luma, chroma, .. } => {
                    u.yuv = 1;
                    (luma, Some(chroma))
                }
            };
            passes.push(Pass {
                entry: "fs_composite",
                target: self.accum[next].as_ref().unwrap().view.clone(),
                tex0,
                tex1: current.map(|c| self.accum[c].as_ref().unwrap().view.clone()),
                tex2,
                uniforms: u,
            });
            current = Some(next);
            layers_drawn += 1;
        }

        let output_view = self.output.as_ref().unwrap().view.clone();
        let mut master = current.map_or(self.gpu.blank.clone(), |c| self.accum[c].as_ref().unwrap().view.clone());
        if current.is_some() && !comp.master_fx_bypass && comp.master_effects.iter().any(|e| e.enabled) {
            let frame = Frame::Rgb { view: master.clone(), w, h };
            if let Frame::Rgb { view, .. } = self.apply_effects(MASTER_ID, &comp.master_effects, frame, time, mods, &comp.layers, &mut passes) {
                master = view;
            }
        }
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [w as f32, h as f32];
        if comp.mapping.active() {
            // Black output, then each enabled surface blended on top.
            u.opacity = 0.0;
            passes.push(Pass {
                entry: "fs_finalize",
                target: output_view.clone(),
                tex0: self.gpu.blank.clone(),
                tex1: None,
                tex2: None,
                uniforms: u,
            });
            for s in comp.mapping.surfaces.iter().filter(|s| s.enabled) {
                let Some(inv) = mapping::quad_to_square(&s.corners) else { continue };
                let c = &s.corners;
                let mut u = Uniforms::with_params(&[]);
                u.resolution = [w as f32, h as f32];
                u.opacity = s.opacity * comp.master_fade;
                u.uv_xform = [s.input[2], s.input[3], s.input[0], s.input[1]];
                u.mode = comp.mapping.test_pattern as u32;
                u.p[0] = [c[0][0], c[0][1], c[1][0], c[1][1]];
                u.p[1] = [c[2][0], c[2][1], c[3][0], c[3][1]];
                u.p[2] = [inv[0], inv[1], inv[2], inv[3]];
                u.p[3] = [inv[4], inv[5], inv[6], inv[7]];
                u.p[4] = [inv[8], s.feather, 0.0, 0.0];
                passes.push(Pass {
                    entry: "fs_surface",
                    target: output_view.clone(),
                    tex0: master.clone(),
                    tex1: None,
                    tex2: None,
                    uniforms: u,
                });
            }
        } else {
            u.opacity = comp.master_fade;
            passes.push(Pass { entry: "fs_finalize", target: output_view, tex0: master, tex1: None, tex2: None, uniforms: u });
        }

        self.encode(&passes);
        self.frame_index += 1;
        FrameInfo { passes: passes.len(), layers_drawn }
    }

    fn push_thumbnail(&mut self, layer: &LayerState, frame: &Frame, canvas: (u32, u32), thumb_h: u32, passes: &mut Vec<Pass>) {
        let res = self.layers.entry(layer.id).or_default();
        Target::ensure(&mut res.thumbnail, &self.gpu.device, "thumbnail", THUMBNAIL_WIDTH, thumb_h);
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [THUMBNAIL_WIDTH as f32, thumb_h as f32];
        u.uv_xform = fit_transform(frame.size(), canvas, layer.fit);
        let (tex0, tex2) = match frame.clone() {
            Frame::Rgb { view, .. } => (view, None),
            Frame::Yuv { luma, chroma, .. } => {
                u.yuv = 1;
                (luma, Some(chroma))
            }
        };
        passes.push(Pass {
            entry: "fs_composite",
            target: res.thumbnail.as_ref().unwrap().view.clone(),
            tex0,
            tex1: None,
            tex2,
            uniforms: u,
        });
    }

    /// The layer's own picture (source + effects, before blending), for UI thumbnails.
    pub fn layer_thumbnail(&self, layer: u64) -> Option<&Target> {
        self.layers.get(&layer)?.thumbnail.as_ref()
    }

    /// Resolves a source spec into a frame, rendering generators into the layer's slot
    /// (`outgoing` selects the transition slot).
    #[allow(clippy::too_many_arguments)]
    fn resolve_spec(
        &mut self,
        layer_id: u64,
        spec: &SourceSpec,
        values: &[f32],
        value_mods: &[crate::model::ParamMod],
        outgoing: bool,
        w: u32,
        h: u32,
        time: f32,
        mods: &ModInputs,
        passes: &mut Vec<Pass>,
    ) -> Option<Frame> {
        let device = &self.gpu.device;
        let res = self.layers.entry(layer_id).or_default();
        match spec {
            SourceSpec::None => None,
            SourceSpec::Generator(id) => {
                let desc = shaders::generator(id)?;
                let slot = if outgoing { &mut res.generator_out } else { &mut res.generator };
                Target::ensure(slot, device, "generator", w, h);
                let target = slot.as_ref().unwrap();
                let mut u = Uniforms::with_params(&resolve_params(desc, values, value_mods, mods));
                u.resolution = [w as f32, h as f32];
                u.time = time;
                passes.push(Pass {
                    entry: desc.entry,
                    target: target.view.clone(),
                    tex0: self.gpu.blank.clone(),
                    tex1: None,
                    tex2: None,
                    uniforms: u,
                });
                Some(Frame::Rgb { view: target.view.clone(), w, h })
            }
            SourceSpec::Image { .. } | SourceSpec::Movie { .. } => {
                let slot = if outgoing { &res.media_out } else { &res.media };
                match slot.as_ref()? {
                    Media::Rgba(t) => Some(Frame::Rgb { view: t.view.clone(), w: t.width, h: t.height }),
                    Media::Nv12 { luma, chroma } => Some(Frame::Yuv {
                        luma: luma.view.clone(),
                        chroma: chroma.view.clone(),
                        w: luma.width,
                        h: luma.height,
                    }),
                }
            }
        }
    }

    /// Blends the outgoing source into the incoming one at the canvas size.
    #[allow(clippy::too_many_arguments)]
    fn crossfade(
        &mut self,
        layer_id: u64,
        incoming: Frame,
        outgoing: Frame,
        t: &crate::model::Transition,
        time: f32,
        fit: FitMode,
        canvas: (u32, u32),
        passes: &mut Vec<Pass>,
    ) -> Frame {
        let (w, h) = canvas;
        let device = &self.gpu.device;
        let res = self.layers.entry(layer_id).or_default();
        let mut base = Uniforms::with_params(&[]);
        base.time = time;
        // YUV sources first become RGB at their own size.
        let to_rgb = |frame: Frame, slot: &mut Option<Target>, passes: &mut Vec<Pass>| -> (wgpu::TextureView, (u32, u32)) {
            match frame {
                Frame::Rgb { view, w, h } => (view, (w, h)),
                Frame::Yuv { luma, chroma, w, h } => {
                    Target::ensure(slot, device, "transition", w, h);
                    let view = slot.as_ref().unwrap().view.clone();
                    let mut u = base;
                    u.resolution = [w as f32, h as f32];
                    u.yuv = 1;
                    passes.push(Pass { entry: "fs_to_rgb", target: view.clone(), tex0: luma, tex1: None, tex2: Some(chroma), uniforms: u });
                    (view, (w, h))
                }
            }
        };
        let [slot_in, slot_out, slot_result] = &mut res.trans;
        let (in_view, in_size) = to_rgb(incoming, slot_in, passes);
        let (out_view, out_size) = to_rgb(outgoing, slot_out, passes);
        Target::ensure(slot_result, device, "transition", w, h);
        let result = slot_result.as_ref().unwrap().view.clone();
        let mut u = base;
        u.resolution = [w as f32, h as f32];
        u.opacity = t.progress(time);
        u.mode = t.kind.shader_index();
        u.uv_xform = fit_transform(in_size, canvas, fit);
        u.p[0] = fit_transform(out_size, canvas, fit);
        passes.push(Pass { entry: "fs_crossfade", target: result.clone(), tex0: in_view, tex1: Some(out_view), tex2: None, uniforms: u });
        Frame::Rgb { view: result, w, h }
    }

    /// Runs the layer's enabled effects at the source's native resolution (ping-pong).
    fn apply_effects(
        &mut self,
        res_id: u64,
        effects: &[EffectState],
        frame: Frame,
        time: f32,
        mods: &ModInputs,
        all_layers: &[LayerState],
        passes: &mut Vec<Pass>,
    ) -> Frame {
        let chain: Vec<_> = effects
            .iter()
            .filter(|e| e.enabled)
            .filter_map(|e| shaders::effect(&e.effect).map(|d| (d, e.id, &e.values, &e.mods, e.mix)))
            .collect();
        if chain.is_empty() {
            return frame;
        }
        let (w, h) = frame.size();
        let device = &self.gpu.device;
        let res = self.layers.entry(res_id).or_default();
        for slot in &mut res.fx {
            Target::ensure(slot, device, "fx", w, h);
        }
        let views: Vec<_> = res.fx.iter().map(|t| t.as_ref().unwrap().view.clone()).collect();
        let mut base = Uniforms::with_params(&[]);
        base.resolution = [w as f32, h as f32];
        base.time = time;

        // Ping-pong across three views, never writing the one being read or the one holding
        // the chain's dry input.
        let mut input_idx: Option<usize> = None;
        let mut input = match frame {
            Frame::Rgb { view, .. } => view,
            Frame::Yuv { luma, chroma, .. } => {
                passes.push(Pass {
                    entry: "fs_to_rgb",
                    target: views[0].clone(),
                    tex0: luma,
                    tex1: None,
                    tex2: Some(chroma),
                    uniforms: Uniforms { yuv: 1, ..base },
                });
                input_idx = Some(0);
                views[0].clone()
            }
        };
        let parity = (self.frame_index % 2) as usize;
        for (desc, fx_id, values, param_mods, mix) in chain {
            let dry = input.clone();
            let dry_idx = input_idx;
            let params = resolve_params(desc, values, param_mods, mods);
            let sub = shaders::multipass(desc.entry);
            let entries: Vec<&'static str> = if sub.is_empty() { vec![desc.entry] } else { sub.to_vec() };
            let last_index = entries.len() - 1;
            for (k, entry) in entries.into_iter().enumerate() {
                let mut u = Uniforms::with_params(&params);
                u.resolution = base.resolution;
                u.time = time;
                let mut tex1 = None;
                let mut tex2 = None;
                let mut next_idx: Option<usize> = None;
                let target = if entry == "fs_rutt" || entry == "fs_rutt_points" {
                    let scale = params.get(6).copied().unwrap_or(2.0).max(1.0);
                    let (rw, rh) = ((w as f32 / scale) as u32, (h as f32 / scale) as u32);
                    // Optional displacement from another layer's picture (its live thumbnail).
                    let other = params.get(12).copied().unwrap_or(0.0) as usize;
                    if other > 0 && let Some(l) = all_layers.get(other - 1) {
                        tex2 = self.layers.get(&l.id).and_then(|r| r.thumbnail.as_ref()).map(|t| t.view.clone());
                    }
                    let res = self.layers.entry(res_id).or_default();
                    Target::ensure(&mut res.rutt, &self.gpu.device, "rutt", rw, rh);
                    u.resolution = [rw.max(1) as f32, rh.max(1) as f32];
                    res.rutt.as_ref().unwrap().view.clone()
                } else if entry == "fx_trails" || entry == "fx_slitscan" {
                    // History effects write one slot while reading the other.
                    let res = self.layers.entry(res_id).or_default();
                    let slots = res.history.entry(fx_id).or_default();
                    for slot in slots.iter_mut() {
                        Target::ensure(slot, &self.gpu.device, "history", w, h);
                    }
                    tex1 = Some(slots[1 - parity].as_ref().unwrap().view.clone());
                    slots[parity].as_ref().unwrap().view.clone()
                } else {
                    let i = (0..3).find(|i| Some(*i) != input_idx && Some(*i) != dry_idx).unwrap_or(0);
                    next_idx = Some(i);
                    views[i].clone()
                };
                if k == last_index && entry == "fx_glow_add" {
                    tex1 = Some(dry.clone());
                }
                passes.push(Pass { entry, target: target.clone(), tex0: input, tex1, tex2, uniforms: u });
                input = target;
                input_idx = next_idx;
            }
            if mix < 0.999 {
                let i = (0..3).find(|i| Some(*i) != input_idx && Some(*i) != dry_idx).unwrap_or(0);
                let u = Uniforms { opacity: mix, ..base };
                passes.push(Pass { entry: "fs_mix", target: views[i].clone(), tex0: input, tex1: Some(dry), tex2: None, uniforms: u });
                input = views[i].clone();
                input_idx = Some(i);
            }
        }
        Frame::Rgb { view: input, w, h }
    }

    fn encode(&mut self, passes: &[Pass]) {
        let stride = self.gpu.uniform_stride as usize;
        if self.uniform_capacity < passes.len() {
            self.uniform_capacity = passes.len().next_power_of_two().max(16);
            self.uniform_buffer = Some(self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("uniforms"),
                size: (self.uniform_capacity * stride) as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let buffer = self.uniform_buffer.clone().unwrap();
        let mut bytes = vec![0u8; passes.len() * stride];
        for (i, pass) in passes.iter().enumerate() {
            let src = bytemuck::bytes_of(&pass.uniforms);
            bytes[i * stride..i * stride + src.len()].copy_from_slice(src);
        }
        self.gpu.queue.write_buffer(&buffer, 0, &bytes);

        let mut encoder =
            self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        for (i, pass) in passes.iter().enumerate() {
            let blank = &self.gpu.blank;
            let bind_group = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.gpu.sampler) },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.gpu.sampler_mirror),
                    },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&pass.tex0) },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(pass.tex1.as_ref().unwrap_or(blank)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(pass.tex2.as_ref().unwrap_or(blank)),
                    },
                ],
            });
            let surface = pass.entry == "fs_surface";
            let vertices = match pass.entry {
                "fs_surface" => 6,
                "fs_rutt" => {
                    let rows = (pass.uniforms.p[0][0] as u32).max(2);
                    let cols = (pass.uniforms.p[0][1] as u32).max(2);
                    let mode = pass.uniforms.p[2][2] as u32;
                    let horizontal = if mode == 3 { 0 } else { rows * (cols - 1) };
                    let vertical = if mode >= 1 { cols * (rows - 1) } else { 0 };
                    let diagonal = if mode == 2 { (rows - 1) * (cols - 1) } else { 0 };
                    (horizontal + vertical + diagonal) * 2
                }
                "fs_rutt_points" => (pass.uniforms.p[0][0] as u32).max(2) * (pass.uniforms.p[0][1] as u32).max(2),
                _ => 3,
            };
            let pipeline = self.gpu.pipeline(pass.entry).clone();
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(pass.entry),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &pass.target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if surface { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_pipeline(&pipeline);
            rp.set_bind_group(0, &bind_group, &[(i * stride) as u32]);
            rp.draw(0..vertices, 0..1);
        }
        self.gpu.queue.submit([encoder.finish()]);
    }

    /// Queues a non-blocking copy of the current output for streaming/recording.
    /// Returns false if the readback ring is full (the caller should repeat its last frame).
    pub fn capture_frame(&mut self) -> bool {
        match &self.output {
            Some(out) => self.capture.enqueue(&self.gpu.device, &self.gpu.queue, out),
            None => false,
        }
    }

    /// Frames whose readback finished since the last call. Never blocks.
    pub fn poll_captures(&mut self) -> Vec<CapturedFrame> {
        self.capture.poll(&self.gpu.device)
    }

    /// Copies the output texture back to the CPU as tightly packed RGBA8 (for snapshots/tests).
    /// Blocks until the GPU is done, so never call this during a live show.
    pub fn read_output(&self) -> Option<(u32, u32, Vec<u8>)> {
        let out = self.output.as_ref()?;
        let (w, h) = (out.width, out.height);
        let padded = (w * 4).div_ceil(256) * 256;
        let buffer = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            out.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: None },
            },
            out.texture.size(),
        );
        self.gpu.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.gpu.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).ok()?;
        let data = slice.get_mapped_range().ok()?;
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for row in data.chunks(padded as usize) {
            pixels.extend_from_slice(&row[..(w * 4) as usize]);
        }
        Some((w, h, pixels))
    }
}

/// Maps target uv → source uv so the source is fitted, filled, or stretched.
fn fit_transform(source: (u32, u32), target: (u32, u32), mode: FitMode) -> [f32; 4] {
    let (sw, sh) = (source.0.max(1) as f32, source.1.max(1) as f32);
    let (tw, th) = (target.0.max(1) as f32, target.1.max(1) as f32);
    let k = match mode {
        FitMode::Stretch => return [1.0, 1.0, 0.0, 0.0],
        FitMode::Fit => (tw / sw).min(th / sh),
        FitMode::Fill => (tw / sw).max(th / sh),
    };
    let sx = tw / (sw * k);
    let sy = th / (sh * k);
    [sx, sy, 0.5 - 0.5 * sx, 0.5 - 0.5 * sy]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_letterboxes_and_fill_crops() {
        // 4:3 source into 16:9 target.
        let fit = fit_transform((640, 480), (1920, 1080), FitMode::Fit);
        assert!((fit[1] - 1.0).abs() < 1e-6 && fit[0] > 1.0, "fit pads horizontally: {fit:?}");
        let fill = fit_transform((640, 480), (1920, 1080), FitMode::Fill);
        assert!((fill[0] - 1.0).abs() < 1e-6 && fill[1] < 1.0, "fill crops vertically: {fill:?}");
        assert_eq!(fit_transform((1, 1), (9, 9), FitMode::Stretch), [1.0, 1.0, 0.0, 0.0]);
    }
}
