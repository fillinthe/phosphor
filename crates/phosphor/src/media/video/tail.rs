//! The last frames a movie showed going forward, kept as cheap handles (GPU surfaces, Hap blocks) so a turn to reverse
//! plays backwards at once from them, without decoding. A second of 4K frames is 400–500 MB, so a player keeps them
//! only while a turn can come (ping-pong, or its layer's direction is mapped to a controller: `VideoPlayer::
//! set_expect_reverse`), and every player's tail shares one budget (`BUDGET`, split evenly between the players keeping
//! one).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::VideoFrame;

/// Bytes all players' tails may hold together.
pub(super) const BUDGET: usize = 256 << 20;

/// Bytes held in tails now, and how many players keep one (each may hold `limit / keepers`).
pub(super) struct Budget {
    limit: usize,
    bytes: AtomicUsize,
    keepers: AtomicUsize,
}

impl Budget {
    pub(super) const fn new(limit: usize) -> Self {
        Self { limit, bytes: AtomicUsize::new(0), keepers: AtomicUsize::new(0) }
    }
}

/// The one budget every player's tail shares.
static SHARED: Budget = Budget::new(BUDGET);

/// A kept frame: (media time, duration, frame).
pub(super) type Kept = (f64, f64, VideoFrame);

pub(super) struct Tail {
    frames: VecDeque<(Kept, usize)>,
    bytes: usize,
    /// Counted in the budget's keepers.
    keeping: bool,
    budget: &'static Budget,
}

impl Default for Tail {
    fn default() -> Self {
        Self::within(&SHARED)
    }
}

impl Tail {
    pub(super) fn within(budget: &'static Budget) -> Self {
        Self { frames: VecDeque::new(), bytes: 0, keeping: false, budget }
    }

    /// Keeps `kept` and drops the oldest frames past `max_frames` or past this player's share of the budget.
    /// `max_frames == 0` keeps nothing.
    pub(super) fn push(&mut self, kept: Kept, max_frames: usize) {
        if max_frames == 0 {
            self.clear();
            return;
        }
        if !self.keeping {
            self.keeping = true;
            self.budget.keepers.fetch_add(1, Ordering::Relaxed);
        }
        let size = kept.2.bytes();
        self.frames.push_back((kept, size));
        self.bytes += size;
        self.budget.bytes.fetch_add(size, Ordering::Relaxed);
        let share = self.budget.limit / self.budget.keepers.load(Ordering::Relaxed).max(1);
        while self.frames.len() > max_frames || (self.bytes > share && !self.frames.is_empty()) {
            self.pop_oldest();
        }
    }

    fn pop_oldest(&mut self) {
        if let Some((_, size)) = self.frames.pop_front() {
            self.bytes -= size;
            self.budget.bytes.fetch_sub(size, Ordering::Relaxed);
        }
    }

    /// Hands the frames over (oldest first) to play backwards; the tail is empty and off the budget afterwards.
    pub(super) fn take(&mut self) -> Vec<Kept> {
        let frames = std::mem::take(&mut self.frames).into_iter().map(|(k, _)| k).collect();
        self.release();
        frames
    }

    /// Drops every kept frame (they are cheap handles: nothing to recycle).
    pub(super) fn clear(&mut self) {
        self.frames.clear();
        self.release();
    }

    fn release(&mut self) {
        self.budget.bytes.fetch_sub(self.bytes, Ordering::Relaxed);
        self.bytes = 0;
        if self.keeping {
            self.keeping = false;
            self.budget.keepers.fetch_sub(1, Ordering::Relaxed);
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.frames.len()
    }
}

impl Drop for Tail {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hap(bytes: usize) -> VideoFrame {
        VideoFrame::Compressed { width: 4, height: 4, format: phosphor_engine::CompressedFormat::Bc1, data: vec![0u8; bytes].into(), alpha: None }
    }

    #[test]
    fn tails_share_one_budget_and_give_it_back() {
        // Its own budget: players in other tests keep tails in the shared one meanwhile.
        static TEST: Budget = Budget::new(BUDGET);
        let frame = 16 << 20;
        let mut a = Tail::within(&TEST);
        for k in 0..40 {
            a.push((k as f64, 0.04, hap(frame)), 100);
        }
        assert!(a.bytes <= BUDGET, "one player keeps at most the whole budget");
        assert_eq!(a.len(), BUDGET / frame);
        let mut b = Tail::within(&TEST);
        for k in 0..40 {
            b.push((k as f64, 0.04, hap(frame)), 100);
            a.push((k as f64, 0.04, hap(frame)), 100);
        }
        assert!(a.bytes <= BUDGET / 2 && b.bytes <= BUDGET / 2, "two players split it: {} + {}", a.bytes, b.bytes);
        // The frame count limit holds under the budget too.
        b.push((99.0, 0.04, hap(1)), 3);
        assert!(b.len() <= 3);
        // Nothing to keep: the tail empties and leaves the budget.
        b.push((100.0, 0.04, hap(1)), 0);
        assert_eq!(b.len(), 0);
        assert_eq!(b.bytes, 0);
        let handed = a.take();
        assert!(!handed.is_empty() && handed.windows(2).all(|w| w[0].0 < w[1].0), "oldest first");
        drop(b);
        drop(a);
        assert_eq!(TEST.bytes.load(Ordering::Relaxed), 0, "every byte given back");
        assert_eq!(TEST.keepers.load(Ordering::Relaxed), 0);
    }
}
