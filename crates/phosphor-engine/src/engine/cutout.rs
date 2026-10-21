//! Remove background (`fx_cutout`): the effect keeps what a matte says is a person. The matte comes from the app
//! (Apple Vision person segmentation): when asked, the engine copies each cut-out effect's input (before the cut, at
//! a small size) and reads it back; the app sends the matte back with `set_matte`. Effects without a matte yet
//! pass the picture through. Costs nothing without the effect.

use crate::capture::{Capture, CapturedFrame};
use crate::gpu::Target;

use super::*;

/// Width of the picture read back for segmentation (height keeps the picture's shape).
const READ_WIDTH: u32 = 512;
/// Mattes (and read targets) not used for this many frames are dropped (the effect went away).
const KEEP_FRAMES: u64 = 120;

#[derive(Default)]
pub(crate) struct Cutouts {
    /// Read this frame's cut-out inputs back (the app's worker is free for one).
    want: bool,
    /// Effect resources (layer / group / MASTER ids) whose small copy goes back after this frame.
    reads: Vec<u64>,
    small: HashMap<u64, (Target, u64)>,
    mattes: HashMap<u64, (Option<Target>, u64)>,
    capture: Capture,
}

impl Engine {
    /// Copy every Remove background effect's input back on the next frame (`take_cutout_frames`).
    pub fn request_cutout_frames(&mut self) {
        self.cutouts.want = true;
    }

    /// Finished copies (non-blocking): tag = the layer / group id the effect sits on, RGBA8.
    pub fn take_cutout_frames(&mut self) -> Vec<CapturedFrame> {
        self.cutouts.capture.poll(&self.gpu.device)
    }

    /// The matte for the Remove background effect on `res` (one byte per pixel, 255 = keep).
    pub fn set_matte(&mut self, res: u64, width: u32, height: u32, mask: &[u8]) {
        if width == 0 || height == 0 || mask.len() < (width * height) as usize {
            return;
        }
        let entry = self.cutouts.mattes.entry(res).or_insert((None, self.frame_index));
        // One byte per pixel, straight up (no conversion on this thread).
        Target::ensure_format(&mut entry.0, &self.gpu.device, "matte", width, height, wgpu::TextureFormat::R8Unorm);
        let t = entry.0.as_ref().unwrap();
        self.gpu.queue.write_texture(
            t.texture.as_image_copy(),
            &mask[..(width * height) as usize],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width), rows_per_image: None },
            t.texture.size(),
        );
    }

    /// Inside `apply_effects` for `fx_cutout` on `res`: queues the small copy of `input` when asked, and returns the
    /// matte to cut with (None = none yet).
    pub(super) fn cutout_matte(&mut self, res: u64, input: &wgpu::TextureView, size: (u32, u32), passes: &mut Vec<Pass>) -> Option<wgpu::TextureView> {
        let frame = self.frame_index;
        // Effect previews and "through an effect" transitions use made-up ids: they never ask Vision for a matte.
        let real = res < super::RESERVED_IDS && res & (super::TRANSITION_IN | super::TRANSITION_OUT) == 0;
        if real && self.cutouts.want && !self.cutouts.reads.contains(&res) {
            let h = (READ_WIDTH as f32 * size.1 as f32 / size.0.max(1) as f32).round().clamp(16.0, 2048.0) as u32;
            let entry = self.cutouts.small.entry(res).or_insert_with(|| (Target::new(&self.gpu.device, "cutout read", READ_WIDTH, h), frame));
            if (entry.0.width, entry.0.height) != (READ_WIDTH, h) {
                entry.0 = Target::new(&self.gpu.device, "cutout read", READ_WIDTH, h);
            }
            entry.1 = frame;
            let src = Frame::Rgb { view: input.clone(), w: size.0, h: size.1 };
            push_fit_copy(entry.0.view.clone(), &src, (READ_WIDTH, h), crate::FitMode::Stretch, (READ_WIDTH, h), passes);
            self.cutouts.reads.push(res);
        }
        let m = self.cutouts.mattes.get_mut(&res)?;
        m.1 = frame;
        m.0.as_ref().map(|t| t.view.clone())
    }

    /// After the frame is submitted: queue the readbacks, forget what's no longer used.
    pub(super) fn finish_cutouts(&mut self) {
        let c = &mut self.cutouts;
        c.want = false;
        for res in std::mem::take(&mut c.reads) {
            if let Some((t, _)) = c.small.get(&res) {
                c.capture.enqueue_tagged(&self.gpu.device, &self.gpu.queue, t, res);
            }
        }
        let frame = self.frame_index;
        c.mattes.retain(|_, (_, used)| frame.saturating_sub(*used) < KEEP_FRAMES);
        c.small.retain(|_, (_, used)| frame.saturating_sub(*used) < KEEP_FRAMES);
    }
}
