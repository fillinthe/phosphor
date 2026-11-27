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

/// Cheap change detection for point lists, run for every slice and mask of every output each frame: one
/// rotate-xor-multiply per point (100 slices of 16 x 16 mesh: 0.04 ms a frame, the standard hasher 0.06 ms). Each
/// step is one-to-one in what came before, so changing any one value always changes the result.
pub(super) fn hash_points<'a>(points: impl IntoIterator<Item = &'a [f32; 2]>, extra: &[u32]) -> u64 {
    const K: u64 = 0x517c_c1b7_2722_0a95;
    let step = |h: u64, word: u64| (h.rotate_left(5) ^ word).wrapping_mul(K);
    let mut h = points.into_iter().fold(0, |h, p| step(h, (u64::from(p[0].to_bits()) << 32) | u64::from(p[1].to_bits())));
    h = step(h, extra.len() as u64);
    extra.iter().fold(h, |h, e| step(h, u64::from(*e)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn any_moved_point_changes_the_hash() {
        let points: Vec<[f32; 2]> = (0..300).map(|i| [i as f32 * 0.01, 1.0 - i as f32 * 0.003]).collect();
        let base = super::hash_points(&points, &[16, 16, 0]);
        assert_eq!(base, super::hash_points(&points, &[16, 16, 0]), "the same points: the same hash");
        for i in [0, 150, 299] {
            let mut moved = points.clone();
            moved[i][1] = f32::from_bits(moved[i][1].to_bits() + 1);
            assert_ne!(base, super::hash_points(&moved, &[16, 16, 0]), "point {i} nudged by the smallest step");
        }
        assert_ne!(base, super::hash_points(&points, &[16, 16, 1]), "a flag");
        assert_ne!(super::hash_points(&points[..2], &[]), super::hash_points(&points[..1], &[]), "a point fewer");
    }
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

    /// The stage scanner's pattern over output `output` (when it's the one being scanned): replaces the picture.
    pub(super) fn push_scan(&self, output: u64, view: &wgpu::TextureView, size: (u32, u32), passes: &mut Vec<Pass>) {
        let Some((_, p)) = self.scan.filter(|(o, _)| *o == output) else { return };
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [size.0 as f32, size.1 as f32];
        u.p[0] = p;
        passes.push(Pass { entry: "fs_scan", target: view.clone(), tex0: self.gpu.blank.clone(), tex1: None, tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
    }

    /// A slice drawn on `scratch`: its own masks rub it out there, then it's pasted onto the output with its blend.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish_cut(&mut self, output: u64, mapping: &mapping::Mapping, scratch: &wgpu::TextureView, view: &wgpu::TextureView, size: (u32, u32), mods: &crate::ModInputs, slice: u64, blend: mapping::SliceBlend, passes: &mut Vec<Pass>) {
        self.push_mask_set(output, mapping, scratch, size, mods, passes, MaskSet::On(slice));
        let u = Uniforms { resolution: [size.0 as f32, size.1 as f32], ..Uniforms::with_params(&[]) };
        passes.push(Pass { entry: blend.paste_entry(), target: view.clone(), tex0: scratch.clone(), tex1: None, tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
    }

    pub(super) fn push_masks(&mut self, output: u64, mapping: &mapping::Mapping, view: &wgpu::TextureView, size: (u32, u32), mods: &crate::ModInputs, passes: &mut Vec<Pass>) {
        self.push_mask_set(output, mapping, view, size, mods, passes, MaskSet::All);
    }

    /// The masks of `set` over `view`: every one (an output without slices), the ones on no slice (over the finished
    /// output), or one slice's own (rubbing the slice out of its own picture: `fs_mask_cut`).
    pub(super) fn push_mask_set(&mut self, output: u64, mapping: &mapping::Mapping, view: &wgpu::TextureView, size: (u32, u32), mods: &crate::ModInputs, passes: &mut Vec<Pass>, set: MaskSet) {
        let wanted = |m: &mapping::Mask| match set {
            MaskSet::All => true,
            MaskSet::Loose => m.attached.is_none(),
            MaskSet::On(slice) => m.attached == Some(slice),
        };
        if !mapping.enabled || !mapping.masks.iter().any(|m| m.enabled && wanted(m)) {
            return;
        }
        let (key, entry) = match set {
            MaskSet::On(slice) => ((output, slice | 1 << 60), "fs_mask_cut"),
            _ => ((output, u64::MAX), "fs_mask"),
        };
        // Attached masks are placed on their slice first, then moved by their (maybe animated) transform: the hash
        // follows both, so the shape is rebuilt only on frames where it changed.
        let placed = mapping.placed_masks_at_where(mods, size.0 as f32 / size.1.max(1) as f32, wanted);
        // Everything that changes the outline: points, handles, smooth, corners, invert, feather.
        let handles: Vec<[f32; 2]> = placed.iter().flat_map(|m| m.handles.iter().map(|h| h.unwrap_or([f32::NAN; 2]))).collect();
        let flags: Vec<u32> = placed
            .iter()
            .flat_map(|m| {
                let corners = (0..m.points.len()).filter(|i| m.is_corner(*i)).fold(0u32, |a, i| a.rotate_left(5) ^ (i as u32 + 1));
                [m.points.len() as u32 * 4 + m.invert as u32 + m.smooth as u32 * 2, m.feather.to_bits() ^ m.opacity.to_bits().rotate_left(13), corners, m.handles.len() as u32 * 4 + m.edge as u32]
            })
            .collect();
        let hash = hash_points(placed.iter().flat_map(|m| m.points.iter()).chain(handles.iter()), &flags);
        let Some(draw) = self.meshes.get(&self.gpu.device, &self.gpu.queue, key, hash, || mapping::masks_triangles(&placed)) else { return };
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [size.0 as f32, size.1 as f32];
        passes.push(Pass { entry, target: view.clone(), tex0: self.gpu.blank.clone(), tex1: None, tex2: None, uniforms: u, mesh: Some(draw), bufs: Vec::new() });
    }
}

/// Which masks `push_mask_set` draws.
#[derive(Clone, Copy)]
pub(super) enum MaskSet {
    All,
    /// The ones on no slice.
    Loose,
    /// One slice's own.
    On(u64),
}
