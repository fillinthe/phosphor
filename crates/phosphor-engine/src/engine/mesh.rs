//! Triangle lists from a cached vertex buffer (mesh warp, masks): rebuilt only when their points change.

use super::*;

#[derive(Clone)]
pub(super) struct MeshDraw {
    pub(super) buffer: std::sync::Arc<wgpu::Buffer>,
    pub(super) vertices: u32,
}

/// Vertex buffers for meshes and masks, rebuilt only when their points change.
#[derive(Default)]
pub(super) struct MeshCache {
    entries: HashMap<(u64, u64), (u64, MeshDraw)>,
    used: std::collections::HashSet<(u64, u64)>,
}

impl MeshCache {
    /// `key` names the geometry (output, surface/mask); `hash` says whether it changed;
    /// `build` makes the (x, y, s, t) triangles when it did.
    pub(super) fn get(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, key: (u64, u64), hash: u64, build: impl FnOnce() -> Vec<[f32; 4]>) -> Option<MeshDraw> {
        self.used.insert(key);
        if let Some((h, draw)) = self.entries.get(&key)
            && *h == hash
        {
            return Some(draw.clone());
        }
        let verts = build();
        if verts.is_empty() {
            self.entries.remove(&key);
            return None;
        }
        let bytes: Vec<u8> = verts.iter().flat_map(|v| v.iter().flat_map(|f| f.to_le_bytes())).collect();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("mesh"), size: bytes.len() as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&buffer, 0, &bytes);
        let draw = MeshDraw { buffer: std::sync::Arc::new(buffer), vertices: verts.len() as u32 };
        self.entries.insert(key, (hash, draw.clone()));
        Some(draw)
    }

    /// Frees buffers nothing used this frame.
    pub(super) fn sweep(&mut self) {
        let used = std::mem::take(&mut self.used);
        self.entries.retain(|k, _| used.contains(k));
    }
}

/// Cheap change detection for point lists.
pub(super) fn hash_points<'a>(points: impl IntoIterator<Item = &'a [f32; 2]>, extra: &[u32]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in points {
        p[0].to_bits().hash(&mut h);
        p[1].to_bits().hash(&mut h);
    }
    extra.hash(&mut h);
    h.finish()
}

impl Engine {
    /// Masks: one pass blacking out every enabled mask's shape on this output (nothing if none).
    /// The alignment grid over an output, when it's on (one full-screen blended pass).
    pub(super) fn push_grid(&self, grid: &crate::model::GridOverlay, view: &wgpu::TextureView, size: (u32, u32), passes: &mut Vec<Pass>) {
        if !grid.on {
            return;
        }
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [size.0 as f32, size.1 as f32];
        let [a, b] = grid.uniforms();
        u.p[0] = a;
        u.p[1] = b;
        passes.push(Pass { entry: "fs_grid", target: view.clone(), tex0: self.gpu.blank.clone(), tex1: None, tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
    }

    pub(super) fn push_masks(&mut self, output: u64, mapping: &mapping::Mapping, view: &wgpu::TextureView, size: (u32, u32), passes: &mut Vec<Pass>) {
        if !mapping.enabled || !mapping.masks.iter().any(|m| m.enabled) {
            return;
        }
        // Attached masks are placed on their slice first, so the hash follows the slice too.
        let placed = mapping.placed_masks();
        let hash = hash_points(placed.iter().flat_map(|m| m.points.iter()), &placed.iter().flat_map(|m| [m.points.len() as u32 * 2 + m.invert as u32, m.feather.to_bits()]).collect::<Vec<_>>());
        let Some(draw) = self.meshes.get(&self.gpu.device, &self.gpu.queue, (output, u64::MAX), hash, || placed.iter().flat_map(mapping::Mask::triangles).collect()) else { return };
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [size.0 as f32, size.1 as f32];
        passes.push(Pass { entry: "fs_mask", target: view.clone(), tex0: self.gpu.blank.clone(), tex1: None, tex2: None, uniforms: u, mesh: Some(draw), bufs: Vec::new() });
    }
}
