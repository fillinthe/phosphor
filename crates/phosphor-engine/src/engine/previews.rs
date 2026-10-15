//! Effect/source previews for the explorer, and layer thumbnails.

use super::*;

impl Engine {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_previews(&mut self, master: &wgpu::TextureView, canvas: (u32, u32), time: f32, mods: &ModInputs, layers: &[LayerState], passes: &mut Vec<Pass>) {
        let items = std::mem::take(&mut self.previews);
        if !items.iter().any(|(_, refresh)| *refresh) {
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
        for (i, (item, refresh)) in items.iter().enumerate() {
            let has_picture = self.preview_targets[i].is_some();
            if !*refresh && has_picture {
                continue;
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
            };
            let Some(Frame::Rgb { view, .. }) = frame else { continue };
            Target::ensure(&mut self.preview_targets[i], &self.gpu.device, "preview", PREVIEW_WIDTH, ph);
            copy(self.preview_targets[i].as_ref().unwrap().view.clone(), view, (PREVIEW_WIDTH, ph), passes);
        }
    }

    pub(super) fn push_thumbnail(&mut self, layer: &LayerState, frame: &Frame, canvas: (u32, u32), thumb_h: u32, passes: &mut Vec<Pass>) {
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
            uniforms: u, mesh: None, bufs: Vec::new() });
    }

    /// The layer's own picture (source + effects, before blending), for UI thumbnails.
    pub fn layer_thumbnail(&self, layer: u64) -> Option<&Target> {
        self.layers.get(&layer)?.thumbnail.as_ref()
    }
}
