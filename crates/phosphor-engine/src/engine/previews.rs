//! Effect/source previews for the explorer, and layer thumbnails.

use super::*;

/// Previews drawn fresh per frame at most.
const NEW_PREVIEWS_PER_FRAME: usize = 6;

impl Engine {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_previews(&mut self, master: &wgpu::TextureView, canvas: (u32, u32), time: f32, mods: &ModInputs, layers: &[LayerState], passes: &mut Vec<Pass>) {
        let items = std::mem::take(&mut self.previews);
        if self.preview_ready.len() < items.len() {
            self.preview_ready.resize(items.len(), false);
        }
        // A tile whose slot now holds another item stops showing the old picture right away.
        for (i, (_, refresh)) in items.iter().enumerate() {
            if *refresh == PreviewRefresh::Moved {
                self.preview_ready[i] = false;
            }
        }
        if !items.iter().enumerate().any(|(i, (_, refresh))| *refresh != PreviewRefresh::No || !self.preview_ready[i]) {
            return;
        }
        let (w, h) = canvas;
        let ph = (PREVIEW_WIDTH * h / w).max(1);
        let copy = |target: wgpu::TextureView, src: wgpu::TextureView, size: (u32, u32), passes: &mut Vec<Pass>| {
            let mut u = Uniforms::with_params(&[]);
            u.resolution = [size.0 as f32, size.1 as f32];
            u.opacity = 1.0;
            passes.push(Pass { entry: "fs_finalize", target, tex0: src, tex1: None, tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
        };
        // Source picture: the chosen layer's thumbnail, or a downscaled copy of the mix.
        let layer_thumb = (self.preview_source != MASTER_ID)
            .then(|| self.layers.get(&self.preview_source).and_then(|r| r.thumbnail.as_ref()).map(|t| t.view.clone()))
            .flatten();
        Target::ensure(&mut self.preview_src, &self.gpu.device, "preview source", PREVIEW_WIDTH, ph);
        let src_view = self.preview_src.as_ref().unwrap().view.clone();
        copy(src_view.clone(), layer_thumb.unwrap_or_else(|| master.clone()), (PREVIEW_WIDTH, ph), passes);

        if self.preview_targets.len() < items.len() {
            self.preview_targets.resize_with(items.len(), || None);
        }
        // New pictures per frame (scrolling a big list would draw every tile on one frame); live ones are spread out
        // by the app already.
        let mut budget = NEW_PREVIEWS_PER_FRAME;
        for (i, (item, refresh)) in items.iter().enumerate() {
            let ready = self.preview_ready[i];
            if *refresh == PreviewRefresh::No && ready {
                continue;
            }
            // An effect whose shader is still being built: ask for it (a few build at a time) and come back; drawing
            // it now would only pass the picture through untouched.
            if let PreviewItem::Effect(fx) = item
                && let Some(d) = shaders::effect(&fx.effect).filter(|d| !shaders::is_external(d.entry))
                && !self.ensure_module(d.entry)
                && !self.failed_modules.contains_key(d.entry)
            {
                continue;
            }
            if !ready {
                if budget == 0 {
                    continue;
                }
                budget -= 1;
            }
            let rid = RESERVED_IDS + 1 + i as u64;
            let frame = match item {
                PreviewItem::Effect(fx) => Some(self.apply_effects(
                    rid,
                    std::slice::from_ref(fx),
                    Frame::Rgb { view: src_view.clone(), w: PREVIEW_WIDTH, h: ph },
                    time,
                    mods,
                    layers,
                    passes,
                )),
                PreviewItem::Generator(id) => shaders::generator(id).and_then(|desc| {
                    self.resolve_spec(rid, &SourceSpec::Generator(id.clone()), &desc.defaults(), &[], false, PREVIEW_WIDTH, ph, time, mods, passes)
                }),
                // A few seconds in, so shaders that start dark (fades, feedback) show something in a still.
                PreviewItem::Clip(id, values) => self.resolve_spec(rid, &SourceSpec::Generator(id.clone()), values, &[], false, PREVIEW_WIDTH, ph, time + 7.0, mods, passes),
            };
            let Some(Frame::Rgb { view, .. }) = frame else { continue };
            Target::ensure(&mut self.preview_targets[i], &self.gpu.device, "preview", PREVIEW_WIDTH, ph);
            copy(self.preview_targets[i].as_ref().unwrap().view.clone(), view, (PREVIEW_WIDTH, ph), passes);
            self.preview_ready[i] = true;
        }
    }

    pub(super) fn push_thumbnail(&mut self, id: u64, fit: FitMode, frame: &Frame, canvas: (u32, u32), thumb_h: u32, passes: &mut Vec<Pass>) {
        let res = self.layers.entry(id).or_default();
        Target::ensure(&mut res.thumbnail, &self.gpu.device, "thumbnail", THUMBNAIL_WIDTH, thumb_h);
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [THUMBNAIL_WIDTH as f32, thumb_h as f32];
        u.uv_xform = fit_transform(frame.size(), canvas, fit);
        let (tex0, tex2, yuv) = frame.clone().bind();
        u.yuv = yuv;
        passes.push(Pass {
            entry: "fs_composite",
            target: res.thumbnail.as_ref().unwrap().view.clone(),
            tex0,
            tex1: None,
            tex2,
            uniforms: u, mesh: None, bufs: Vec::new() });
    }

    /// The layer's own picture (source + effects, before blending), for UI thumbnails.
    pub fn layer_thumbnail(&self, layer: u64) -> Option<&Target> {
        self.layers.get(&layer)?.thumbnail.as_ref()
    }
}
