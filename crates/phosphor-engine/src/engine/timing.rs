//! Real GPU time per frame: a timestamp at the start of the first pass and the end of the last, resolved into
//! a small buffer and read back a few frames later (never blocking). Only where the adapter offers timestamps.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

const FREE: u8 = 0;
const MAPPING: u8 = 1;
const READY: u8 = 2;

pub struct GpuTimer {
    query: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<(wgpu::Buffer, Arc<AtomicU8>)>,
    /// Nanoseconds per timestamp tick.
    period: f32,
    /// Slot the current frame copies into (None = none free this frame).
    current: Option<usize>,
    /// A 1×1 target for the empty "frame end" pass.
    end_target: crate::gpu::Target,
    pub last_ms: Option<f32>,
    /// Diagnostics: readbacks completed, and the last raw pair.
    pub reads: u32,
    pub raw: (u64, u64),
}

impl GpuTimer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let query = device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("frame time"), ty: wgpu::QueryType::Timestamp, count: 2 });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor { label: Some("frame time resolve"), size: 16, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let slots = (0..3)
            .map(|_| {
                let b = device.create_buffer(&wgpu::BufferDescriptor { label: Some("frame time readback"), size: 16, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
                (b, Arc::new(AtomicU8::new(FREE)))
            })
            .collect();
        Some(Self { query, resolve, slots, period: queue.get_timestamp_period(), current: None, end_target: crate::gpu::Target::new(device, "frame end", 1, 1), last_ms: None, reads: 0, raw: (0, 0) })
    }

    /// Timestamp writes for pass `i` of `n`: the start of the first pass. (The end is the start of an empty
    /// pass added after the last one: Metal samples at stage boundaries, and an end-of-pass sample came back
    /// older than the start on this Mac.)
    pub fn writes(&self, i: usize, _n: usize) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        (i == 0).then(|| wgpu::RenderPassTimestampWrites { query_set: &self.query, beginning_of_pass_write_index: Some(0), end_of_pass_write_index: None })
    }

    /// After the passes: an empty pass whose start is the frame's end, then resolve the two timestamps and
    /// copy them into a free readback slot.
    pub fn finish(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.current = self.slots.iter().position(|(_, s)| s.load(Ordering::Acquire) == FREE);
        let Some(i) = self.current else { return };
        {
            let _end = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame end"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.end_target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: Some(wgpu::RenderPassTimestampWrites { query_set: &self.query, beginning_of_pass_write_index: Some(1), end_of_pass_write_index: None }),
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        encoder.resolve_query_set(&self.query, 0..2, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.slots[i].0, 0, 16);
    }

    /// After the submit: start mapping that slot.
    pub fn after_submit(&mut self) {
        let Some(i) = self.current.take() else { return };
        let (buf, state) = &self.slots[i];
        state.store(MAPPING, Ordering::Release);
        let st = state.clone();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            st.store(if r.is_ok() { READY } else { FREE }, Ordering::Release);
        });
    }

    /// One line for the bench.
    pub fn status(&self) -> String {
        format!("on, {:.3} ns/tick, {} readbacks, last raw {:?}, last {:?} ms", self.period, self.reads, self.raw, self.last_ms)
    }

    /// Reads any slot that finished mapping (call once per frame, before encoding).
    pub fn poll(&mut self) {
        for (buf, state) in &self.slots {
            if state.load(Ordering::Acquire) != READY {
                continue;
            }
            if let Ok(data) = buf.slice(..).get_mapped_range() {
                let t0 = u64::from_le_bytes(data[0..8].try_into().unwrap());
                let t1 = u64::from_le_bytes(data[8..16].try_into().unwrap());
                self.reads += 1;
                self.raw = (t0, t1);
                if t1 > t0 {
                    self.last_ms = Some((t1 - t0) as f32 * self.period / 1.0e6);
                }
            }
            buf.unmap();
            state.store(FREE, Ordering::Release);
        }
    }
}
