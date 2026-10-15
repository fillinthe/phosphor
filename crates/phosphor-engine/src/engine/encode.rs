//! Recording passes into one command encoder, and reading frames back (captures, snapshots).

use super::*;

impl Engine {
    pub(super) fn encode(&mut self, passes: &[Pass]) {
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
                    wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(pass.bufs.first().unwrap_or(blank)) },
                    wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(pass.bufs.get(1).unwrap_or(blank)) },
                    wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(pass.bufs.get(2).unwrap_or(blank)) },
                    wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(pass.bufs.get(3).unwrap_or(blank)) },
                    wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::TextureView(pass.bufs.get(4).unwrap_or(blank)) },
                    wgpu::BindGroupEntry { binding: 11, resource: wgpu::BindingResource::TextureView(pass.bufs.get(5).unwrap_or(blank)) },
                    wgpu::BindGroupEntry { binding: 12, resource: wgpu::BindingResource::TextureView(pass.bufs.get(6).unwrap_or(blank)) },
                    wgpu::BindGroupEntry { binding: 13, resource: wgpu::BindingResource::TextureView(pass.bufs.get(7).unwrap_or(blank)) },
                ],
            });
            // Surfaces, meshes and masks draw on top of what the output already has.
            let surface = matches!(pass.entry, "fs_surface" | "fs_mesh" | "fs_mask");
            let vertices = match pass.entry {
                _ if pass.mesh.is_some() => pass.mesh.as_ref().map_or(0, |m| m.vertices),
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
            if let Some(m) = &pass.mesh {
                rp.set_vertex_buffer(0, m.buffer.slice(..));
            }
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
