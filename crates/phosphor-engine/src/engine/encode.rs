//! Recording passes into one command encoder, and reading frames back (captures, snapshots).

use super::*;

/// Textures in a bind group: tex0, tex1, tex2 and the multi-pass buffers.
pub(super) const BIND_TEXTURES: usize = 3 + crate::shaders::MAX_BUFFERS;

/// Bind groups by the views they bind (tex0, tex1, tex2, buf0..): most passes bind the same textures every
/// frame, so the group is made once. Groups unused for a frame are dropped (they keep their textures alive).
#[derive(Default)]
pub(super) struct BindCache {
    current: HashMap<[wgpu::TextureView; BIND_TEXTURES], wgpu::BindGroup>,
    previous: HashMap<[wgpu::TextureView; BIND_TEXTURES], wgpu::BindGroup>,
}

impl BindCache {
    /// Starts a frame: what wasn't used last frame goes.
    fn next_frame(&mut self) {
        self.previous = std::mem::take(&mut self.current);
    }

    fn clear(&mut self) {
        self.current.clear();
        self.previous.clear();
    }

    fn get(&mut self, key: [wgpu::TextureView; BIND_TEXTURES], make: impl FnOnce(&[wgpu::TextureView; BIND_TEXTURES]) -> wgpu::BindGroup) -> wgpu::BindGroup {
        if let Some(g) = self.current.get(&key) {
            return g.clone();
        }
        let g = self.previous.remove(&key).unwrap_or_else(|| make(&key));
        self.current.insert(key, g.clone());
        g
    }
}

impl Engine {
    pub(super) fn encode(&mut self, passes: &[Pass]) {
        let stride = self.gpu.uniform_stride as usize;
        self.bind_cache.next_frame();
        if self.uniform_capacity < passes.len() {
            self.bind_cache.clear();
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
            let view = |v: Option<&wgpu::TextureView>| v.unwrap_or(blank).clone();
            let key: [wgpu::TextureView; BIND_TEXTURES] = std::array::from_fn(|i| match i {
                0 => pass.tex0.clone(),
                1 => view(pass.tex1.as_ref()),
                2 => view(pass.tex2.as_ref()),
                i => view(pass.bufs.get(i - 3)),
            });
            let (device, layout, sampler, sampler_mirror) = (&self.gpu.device, &self.gpu.layout, &self.gpu.sampler, &self.gpu.sampler_mirror);
            let bind_group = self.bind_cache.get(key, |k| device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler_mirror) },
                    // tex0, tex1, tex2, buf0.. at bindings 3.. (see common.wgsl).
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&k[0]) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&k[1]) },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&k[2]) },
                    wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&k[3]) },
                    wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&k[4]) },
                    wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&k[5]) },
                    wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(&k[6]) },
                    wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::TextureView(&k[7]) },
                    wgpu::BindGroupEntry { binding: 11, resource: wgpu::BindingResource::TextureView(&k[8]) },
                    wgpu::BindGroupEntry { binding: 12, resource: wgpu::BindingResource::TextureView(&k[9]) },
                    wgpu::BindGroupEntry { binding: 13, resource: wgpu::BindingResource::TextureView(&k[10]) },
                    wgpu::BindGroupEntry { binding: 14, resource: wgpu::BindingResource::TextureView(&k[11]) },
                    wgpu::BindGroupEntry { binding: 15, resource: wgpu::BindingResource::TextureView(&k[12]) },
                    wgpu::BindGroupEntry { binding: 16, resource: wgpu::BindingResource::TextureView(&k[13]) },
                    wgpu::BindGroupEntry { binding: 17, resource: wgpu::BindingResource::TextureView(&k[14]) },
                ],
            }));
            // Surfaces, meshes and masks draw on top of what the output already has.
            let surface = pass.entry.starts_with("fs_surface") || pass.entry.starts_with("fs_mesh") || matches!(pass.entry, "fs_mask" | "fs_grid");
            let vertices = match pass.entry {
                _ if pass.mesh.is_some() => pass.mesh.as_ref().map_or(0, |m| m.vertices),
                e if e.starts_with("fs_surface") => 6,
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
                // An ISF vertex shader moves a quad: six vertices.
                e if crate::shaders::vertex_entry(e).is_some() => 6,
                _ => 3,
            };
            let pipeline = self.gpu.pipeline_for(pass.entry, pass.target.texture().format()).clone();
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
                timestamp_writes: self.gpu_timer.as_ref().and_then(|t| t.writes(i, passes.len())),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_pipeline(&pipeline);
            rp.set_bind_group(0, &bind_group, &[(i * stride) as u32]);
            if let Some(m) = &pass.mesh {
                rp.set_vertex_buffer(0, m.buffer.slice(..));
            }
            rp.draw(0..vertices, 0..1);
        }
        if let Some(t) = &mut self.gpu_timer
            && !passes.is_empty()
        {
            t.finish(&mut encoder);
        }
        self.gpu.queue.submit([encoder.finish()]);
        self.mark_external_frame();
        if let Some(t) = &mut self.gpu_timer {
            t.after_submit();
        }
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

    /// Queues a copy of one layer's (or group's) thumbnail; it comes back from `poll_thumbnails`
    /// tagged with the id. False if the layer has no picture yet or the ring is full.
    pub fn capture_thumbnail(&mut self, layer: u64) -> bool {
        match self.layers.get(&layer).and_then(|l| l.thumbnail.as_ref()) {
            Some(t) => self.thumb_capture.enqueue_tagged(&self.gpu.device, &self.gpu.queue, t, layer),
            None => false,
        }
    }

    /// Thumbnails whose readback finished (`tag` = layer id). Never blocks; nothing to do when none were asked.
    pub fn poll_thumbnails(&mut self) -> Vec<CapturedFrame> {
        self.thumb_capture.poll(&self.gpu.device)
    }

    /// Copies the output texture back to the CPU as tightly packed RGBA8 (for snapshots/tests).
    /// Blocks until the GPU is done, so never call this during a live show.
    pub fn read_output(&self) -> Option<(u32, u32, Vec<u8>)> {
        self.read_target(self.output.as_ref()?)
    }

    /// Same as `read_output`, for one configured output (after its mapping).
    pub fn read_output_of(&self, id: u64) -> Option<(u32, u32, Vec<u8>)> {
        self.read_target(self.output_texture(id)?)
    }

    pub(super) fn read_target(&self, out: &Target) -> Option<(u32, u32, Vec<u8>)> {
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
