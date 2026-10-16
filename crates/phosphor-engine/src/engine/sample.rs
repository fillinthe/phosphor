//! Reading a few pixels of the program back to the CPU (LED / DMX pixel mapping): the points are stored
//! in a small texture (16-bit x and y packed into RGBA8), one pass samples the program at each, and the
//! result comes back through the non-blocking readback ring. Costs nothing while no points are set.

use crate::capture::Capture;
use crate::gpu::Target;

use super::*;

/// Points per row of the point / result textures.
const ROW: usize = 512;

#[derive(Default)]
pub(crate) struct PixelSampler {
    points: Option<Target>,
    out: Option<Target>,
    count: usize,
    hash: u64,
    /// Sample on the next render.
    wanted: bool,
    /// The readback was queued for this frame's result.
    queued: bool,
    capture: Capture,
}

impl Engine {
    /// Where to sample the program (0..1, y down). Uploaded only when the points change; empty = off.
    pub fn set_sample_points(&mut self, points: &[[f32; 2]]) {
        let hash = hash_points(points, &[points.len() as u32]);
        let s = &mut self.sampler;
        if hash == s.hash && s.count == points.len() {
            return;
        }
        s.hash = hash;
        s.count = points.len();
        if points.is_empty() {
            *s = PixelSampler::default();
            return;
        }
        let w = points.len().min(ROW) as u32;
        let h = points.len().div_ceil(ROW) as u32;
        Target::ensure(&mut s.points, &self.gpu.device, "sample points", w, h);
        Target::ensure(&mut s.out, &self.gpu.device, "samples", w, h);
        let mut data = vec![0u8; (w * h * 4) as usize];
        for (i, p) in points.iter().enumerate() {
            let x = (p[0].clamp(0.0, 1.0) * 65535.0).round() as u16;
            let y = (p[1].clamp(0.0, 1.0) * 65535.0).round() as u16;
            data[i * 4..i * 4 + 4].copy_from_slice(&[(x >> 8) as u8, x as u8, (y >> 8) as u8, y as u8]);
        }
        let t = s.points.as_ref().unwrap();
        self.gpu.queue.write_texture(
            t.texture.as_image_copy(),
            &data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: None },
            t.texture.size(),
        );
    }

    /// Samples the points on the next frame (call at the rate the lights need).
    pub fn request_samples(&mut self) {
        self.sampler.wanted = self.sampler.count > 0;
    }

    /// The newest finished samples, RGBA per point in order (non-blocking; None until one is ready).
    pub fn take_samples(&mut self) -> Option<Vec<[u8; 4]>> {
        let n = self.sampler.count;
        let frame = self.sampler.capture.poll(&self.gpu.device).pop()?;
        Some(frame.rgba.chunks_exact(4).take(n).map(|c| [c[0], c[1], c[2], c[3]]).collect())
    }

    /// The sampling pass, after the program is finished (in `render`).
    pub(super) fn push_samples(&mut self, program: &wgpu::TextureView, passes: &mut Vec<Pass>) {
        let s = &mut self.sampler;
        if !s.wanted {
            return;
        }
        let (Some(points), Some(out)) = (&s.points, &s.out) else { return };
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [out.width as f32, out.height as f32];
        passes.push(Pass { entry: "fs_sample", target: out.view.clone(), tex0: program.clone(), tex1: Some(points.view.clone()), tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
        s.wanted = false;
        s.queued = true;
    }

    /// Queues the readback once the frame's passes are submitted.
    pub(super) fn read_samples(&mut self) {
        let s = &mut self.sampler;
        if !std::mem::take(&mut s.queued) {
            return;
        }
        if let Some(out) = &s.out {
            s.capture.enqueue(&self.gpu.device, &self.gpu.queue, out);
        }
    }
}
