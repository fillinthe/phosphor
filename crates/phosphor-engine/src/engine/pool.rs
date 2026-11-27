//! Effect-chain pictures shared by every chain of the same size and format.
//!
//! An effect chain ping-pongs across three pictures that only matter while the chain runs and until what it made has
//! been drawn on (blended, copied to a thumbnail / feed / monitor). Each layer used to own three of its own for good:
//! twenty layers with effects held sixty full-size pictures. Now a chain borrows a set for the frame and hands it back
//! once its picture has been used, and the next chain of that size draws in the same three. Passes run in the order
//! they're recorded (one encoder per frame), so the next chain overwrites them only after the last one's picture was
//! read. What has to outlive a frame (trails, multi-pass buffers, held pictures, feeds) stays with its layer.

use std::collections::HashMap;

use crate::gpu::Target;

/// Size and format of a set of chain pictures.
type SetKey = (u32, u32, wgpu::TextureFormat);

/// A set nobody borrowed for this many frames is freed (a layer switched off, a canvas resized).
const KEEP_FRAMES: u64 = 300;

struct PoolSet {
    targets: [Target; 3],
    busy: bool,
    last_used: u64,
}

/// Which set a chain borrowed.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Lease {
    key: SetKey,
    index: usize,
}

#[derive(Default)]
pub(super) struct ChainPool {
    sets: HashMap<SetKey, Vec<PoolSet>>,
    /// What each chain (by its resource id) holds this frame, until `release`.
    leases: HashMap<u64, Vec<Lease>>,
}

impl ChainPool {
    /// Three pictures of `size` and `format` for the chain `owner`, kept from the other chains until `release(owner)`
    /// (or the end of the frame).
    pub(super) fn lease(&mut self, device: &wgpu::Device, owner: u64, (w, h): (u32, u32), format: wgpu::TextureFormat, frame: u64) -> [wgpu::TextureView; 3] {
        let key = (w.max(1), h.max(1), format);
        let sets = self.sets.entry(key).or_default();
        let index = match sets.iter().position(|s| !s.busy) {
            Some(i) => i,
            None => {
                let make = || Target::with_format(device, "fx", key.0, key.1, format);
                sets.push(PoolSet { targets: [make(), make(), make()], busy: false, last_used: frame });
                sets.len() - 1
            }
        };
        let set = &mut sets[index];
        set.busy = true;
        set.last_used = frame;
        self.leases.entry(owner).or_default().push(Lease { key, index });
        std::array::from_fn(|i| set.targets[i].view.clone())
    }

    /// The chain `owner`'s picture has been used: its sets go back for the next chain to draw in.
    pub(super) fn release(&mut self, owner: u64) {
        for lease in self.leases.remove(&owner).unwrap_or_default() {
            if let Some(set) = self.sets.get_mut(&lease.key).and_then(|s| s.get_mut(lease.index)) {
                set.busy = false;
            }
        }
    }

    /// After the frame is recorded: every set is free again, and sets nobody used for a while are dropped.
    pub(super) fn end_frame(&mut self, frame: u64) {
        self.leases.clear();
        for sets in self.sets.values_mut() {
            for s in sets.iter_mut() {
                s.busy = false;
            }
            // From the end only, so the sets still in use keep their place (a lease is an index).
            while sets.last().is_some_and(|s| frame.saturating_sub(s.last_used) > KEEP_FRAMES) {
                sets.pop();
            }
        }
        self.sets.retain(|_, sets| !sets.is_empty());
    }

    /// How many sets exist (tests: chains share them).
    #[cfg(test)]
    pub(super) fn set_count(&self) -> usize {
        self.sets.values().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease_of(pool: &ChainPool, owner: u64) -> Vec<Lease> {
        pool.leases.get(&owner).cloned().unwrap_or_default()
    }

    #[test]
    fn chains_after_each_other_share_one_set_and_chains_at_once_get_their_own() {
        let Some(engine) = super::super::gpu_tests::test_engine() else { return };
        let device = engine.device().clone();
        let mut pool = ChainPool::default();
        let size = (64, 32);
        // One after the other (each released before the next): one set.
        for layer in 1..=5 {
            pool.lease(&device, layer, size, crate::gpu::FORMAT, 0);
            pool.release(layer);
        }
        assert_eq!(pool.set_count(), 1);
        // Two held at once (a transition's two sides): two sets, never the same one.
        pool.lease(&device, 10, size, crate::gpu::FORMAT, 0);
        pool.lease(&device, 11, size, crate::gpu::FORMAT, 0);
        assert_ne!(lease_of(&pool, 10), lease_of(&pool, 11));
        assert_eq!(pool.set_count(), 2);
        // Another size or format is a set of its own.
        pool.lease(&device, 12, (32, 32), crate::gpu::FORMAT, 0);
        pool.lease(&device, 13, size, crate::gpu::DATA_FORMAT, 0);
        assert_eq!(pool.set_count(), 4);
        // The next frame everything is free again; sets unused for long go.
        pool.end_frame(1);
        pool.lease(&device, 1, size, crate::gpu::FORMAT, 1);
        assert_eq!(lease_of(&pool, 1)[0].index, 0);
        pool.end_frame(KEEP_FRAMES + 1);
        assert_eq!(pool.set_count(), 1, "only the set used on frame 1 is left (the newest of its size)");
        pool.end_frame(2 * KEEP_FRAMES + 3);
        assert_eq!(pool.set_count(), 0);
    }
}
