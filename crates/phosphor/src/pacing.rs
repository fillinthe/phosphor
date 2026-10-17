//! Frame pacing for outputs that run slower than the display (recording, streaming, the network
//! session, NDI): "is a frame due at N fps?". One component so every output paces the same way.

#[derive(Clone, Copy, Debug, Default)]
pub struct FramePacer {
    next: f64,
}

impl FramePacer {
    /// True once per 1/fps seconds. After a stall it skips ahead rather than bursting frames.
    pub fn due(&mut self, now: f64, fps: u32) -> bool {
        let step = 1.0 / fps.max(1) as f64;
        // A frame that arrives a little early (display jitter at the same rate) still counts as this one.
        if now < self.next - 0.25 * step {
            return false;
        }
        // On time: keep the steady cadence. Late by more than a frame: restart from now.
        self.next = if self.next + step < now { now + step } else { self.next + step };
        true
    }

    /// Start pacing from `now` (e.g. when a recording begins).
    pub fn reset(&mut self, now: f64) {
        self.next = now;
    }
}

#[cfg(test)]
mod tests {
    use super::FramePacer;

    #[test]
    fn paces_and_recovers_from_stalls() {
        let mut p = FramePacer::default();
        // 120 Hz display, 30 fps output: one frame in four.
        let frames = (0..480).filter(|i| p.due(*i as f64 / 120.0, 30)).count();
        assert!((119..=121).contains(&frames), "{frames}");
        // A one-second stall gives one frame, not a burst of 30.
        let after = (0..4).filter(|i| p.due(5.0 + *i as f64 / 1000.0, 30)).count();
        assert_eq!(after, 1);
    }
}
