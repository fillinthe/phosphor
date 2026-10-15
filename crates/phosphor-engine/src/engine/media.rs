//! Media the app hands the engine for a layer: RGBA / NV12 uploads, zero-copy textures, transitions.

use super::*;

impl Engine {
    /// Uploads an RGBA8 frame (tightly packed) for a layer: a still image or one video frame.
    /// The texture is reused while the size stays the same.
    pub fn upload_rgba(&mut self, layer: u64, width: u32, height: u32, rgba: &[u8]) {
        self.upload_rgba_slot(layer, false, width, height, rgba);
    }

    /// Same as `upload_rgba`, for the outgoing source of a transition.
    pub fn upload_rgba_outgoing(&mut self, layer: u64, width: u32, height: u32, rgba: &[u8]) {
        self.upload_rgba_slot(layer, true, width, height, rgba);
    }

    pub(super) fn upload_rgba_slot(&mut self, layer: u64, outgoing: bool, width: u32, height: u32, rgba: &[u8]) {
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

    /// Uses a GPU texture another app (or the system) produced as a layer's picture, with no copy:
    /// Syphon input. It must be sampleable on this device.
    pub fn set_layer_texture(&mut self, layer: u64, texture: wgpu::Texture) {
        let size = texture.size();
        let view = texture.create_view(&Default::default());
        let target = Target { texture, view, width: size.width, height: size.height };
        self.layers.entry(layer).or_default().media = Some(Media::Rgba(target));
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
    pub(super) fn upload_nv12_slot(&mut self, layer: u64, outgoing: bool, width: u32, height: u32, luma: &[u8], luma_stride: u32, chroma: &[u8], chroma_stride: u32) {
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
}
