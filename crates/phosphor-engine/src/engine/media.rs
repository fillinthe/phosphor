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

    /// A video frame's luma (R8) and chroma (RG8) planes as textures someone else made (zero-copy hardware
    /// video: the decoder's own memory). The caller keeps the memory alive while frames are in flight.
    pub fn set_layer_nv12_textures(&mut self, layer: u64, outgoing: bool, luma: wgpu::Texture, chroma: wgpu::Texture, full_range: bool) {
        let target = |texture: wgpu::Texture| {
            let size = texture.size();
            let view = texture.create_view(&Default::default());
            Target { texture, view, width: size.width, height: size.height }
        };
        let res = self.layers.entry(layer).or_default();
        let slot = if outgoing { &mut res.media_out } else { &mut res.media };
        *slot = Some(Media::Nv12 { luma: target(luma), chroma: target(chroma), full_range });
    }

    /// The GPU can sample block-compressed (BC/DXT) textures: Hap movies skip CPU decoding.
    pub fn compressed_supported(&self) -> bool {
        self.gpu.device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
    }

    /// Uploads a block-compressed frame (Hap: DXT1/DXT5 blocks, rows of 4×4 blocks) for a layer, reusing
    /// the texture while size and format stay the same. `width`/`height` must be multiples of 4.
    pub fn upload_compressed(&mut self, layer: u64, outgoing: bool, width: u32, height: u32, format: CompressedFormat, blocks: &[u8]) {
        let (bw, bh) = (width.div_ceil(4), height.div_ceil(4));
        if blocks.len() < (bw * bh * format.block_bytes()) as usize || !self.compressed_supported() {
            return;
        }
        let res = self.layers.entry(layer).or_default();
        let slot = if outgoing { &mut res.media_out } else { &mut res.media };
        let reuse = matches!(slot, Some(Media::Compressed { target, format: f }) if target.width == bw * 4 && target.height == bh * 4 && *f == format);
        if !reuse {
            let texture = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hap"),
                size: wgpu::Extent3d { width: bw * 4, height: bh * 4, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: format.texture_format(),
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            *slot = Some(Media::Compressed { target: Target { texture, view, width: bw * 4, height: bh * 4 }, format });
        }
        let Some(Media::Compressed { target, .. }) = slot else { return };
        self.gpu.queue.write_texture(
            target.texture.as_image_copy(),
            blocks,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bw * format.block_bytes()), rows_per_image: Some(bh) },
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
        // Only our own 8-bit TV-range textures are reused (zero-copy frames bring their own).
        let reuse = matches!(slot, Some(Media::Nv12 { luma, full_range: false, .. })
            if luma.width == width && luma.height == height && luma.texture.format() == wgpu::TextureFormat::R8Unorm && luma.texture.usage().contains(wgpu::TextureUsages::COPY_DST));
        if !reuse {
            *slot = Some(Media::Nv12 {
                luma: Target::with_format(device, "luma", width, height, wgpu::TextureFormat::R8Unorm),
                chroma: Target::with_format(device, "chroma", cw, ch, wgpu::TextureFormat::Rg8Unorm),
                full_range: false,
            });
        }
        let Some(Media::Nv12 { luma: lt, chroma: ct, .. }) = slot else { return };
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
