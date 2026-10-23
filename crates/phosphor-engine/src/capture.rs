//! Non-blocking readback of the output texture, for streaming/recording. A small ring of
//! staging buffers is copied into on the GPU; frames come back a few frames later via
//! `poll_captures`, never stalling the render loop.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::gpu::Target;

const SLOTS: usize = 3;
const FREE: u8 = 0;
const IN_FLIGHT: u8 = 1;
const READY: u8 = 2;
const FAILED: u8 = 3;

pub struct CapturedFrame {
    /// Whatever the caller passed to `enqueue_tagged` (0 for `enqueue`): says which picture this is.
    pub tag: u64,
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA8.
    pub rgba: Vec<u8>,
}

struct Slot {
    tag: u64,
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    width: u32,
    height: u32,
    padded_row: u32,
}

#[derive(Default)]
pub(crate) struct Capture {
    slots: Vec<Slot>,
    /// Most copies in flight at once (0 = `SLOTS`); slots are only made when needed.
    max: usize,
}

impl Capture {
    /// A ring that can have up to `n` copies in flight (several readers sharing it).
    pub fn with_slots(n: usize) -> Self {
        Self { slots: Vec::new(), max: n }
    }

    /// Enqueues a copy of `out`; returns false (and copies nothing) if every slot is still busy.
    pub fn enqueue(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, out: &Target) -> bool {
        self.enqueue_tagged(device, queue, out, 0)
    }

    /// Same, remembering `tag` for the frame that comes back (one ring can carry several pictures).
    pub fn enqueue_tagged(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, out: &Target, tag: u64) -> bool {
        let padded_row = (out.width * 4).div_ceil(256) * 256;
        let size = (padded_row * out.height) as u64;
        // Drop slots whose size no longer matches (resolution changed).
        self.slots.retain(|s| s.state.load(Ordering::Acquire) != FREE || (s.width == out.width && s.height == out.height));
        let free = self.slots.iter().position(|s| s.state.load(Ordering::Acquire) == FREE);
        let idx = match free {
            Some(i) => i,
            None if self.slots.len() < if self.max > 0 { self.max } else { SLOTS } => {
                self.slots.push(Slot {
                    tag,
                    buffer: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("capture"),
                        size,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    }),
                    state: Arc::new(AtomicU8::new(FREE)),
                    width: out.width,
                    height: out.height,
                    padded_row,
                });
                self.slots.len() - 1
            }
            None => return false,
        };
        self.slots[idx].tag = tag;
        let slot = &self.slots[idx];
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("capture") });
        encoder.copy_texture_to_buffer(
            out.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &slot.buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_row), rows_per_image: None },
            },
            out.texture.size(),
        );
        queue.submit([encoder.finish()]);
        slot.state.store(IN_FLIGHT, Ordering::Release);
        let state = slot.state.clone();
        slot.buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            state.store(if r.is_ok() { READY } else { FAILED }, Ordering::Release);
        });
        true
    }

    /// Collects frames whose copies have finished. Call every frame; it never blocks.
    pub fn poll(&mut self, device: &wgpu::Device) -> Vec<CapturedFrame> {
        if self.slots.is_empty() {
            return Vec::new();
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let mut frames = Vec::new();
        for slot in &self.slots {
            match slot.state.load(Ordering::Acquire) {
                READY => {
                    if let Ok(data) = slot.buffer.slice(..).get_mapped_range() {
                        let row = (slot.width * 4) as usize;
                        let mut rgba = Vec::with_capacity(row * slot.height as usize);
                        for r in data.chunks(slot.padded_row as usize) {
                            rgba.extend_from_slice(&r[..row]);
                        }
                        frames.push(CapturedFrame { tag: slot.tag, width: slot.width, height: slot.height, rgba });
                    }
                    slot.buffer.unmap();
                    slot.state.store(FREE, Ordering::Release);
                }
                FAILED => {
                    slot.buffer.unmap();
                    slot.state.store(FREE, Ordering::Release);
                }
                _ => {}
            }
        }
        frames
    }
}
