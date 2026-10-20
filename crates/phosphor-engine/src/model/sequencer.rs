//! Step sequencer tracks (VDMX's Step Sequencer): a row of steps played on the beat grid, as values or on/off
//! gates. A track lives on a shared modulator (`Modulator::sequence`), so any number of controls can follow it,
//! and several tracks side by side make a multi-track sequencer.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StepTrack {
    /// Each step's value (0..1); a gate step is on at 0.5 and up.
    pub steps: Vec<f32>,
    /// Steps per beat: 1 = quarter notes, 4 = sixteenths (0 = 4).
    pub per_beat: f32,
    /// On/off steps (pads) instead of levels.
    pub gate: bool,
    /// Slide from one step to the next instead of jumping (levels only).
    pub glide: bool,
}

impl Default for StepTrack {
    fn default() -> Self {
        Self { steps: [1.0, 0.0, 0.5, 0.0].repeat(4), per_beat: 4.0, gate: false, glide: false }
    }
}

/// The step counts and rates the editor offers.
pub const STEP_COUNTS: [usize; 5] = [4, 8, 16, 32, 64];
pub const STEP_RATES: [(f32, &str); 5] = [(0.25, "bar"), (1.0, "beat"), (2.0, "1/8"), (4.0, "1/16"), (8.0, "1/32")];

impl StepTrack {
    pub fn len(&self) -> usize {
        self.steps.len().max(1)
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn rate(&self) -> f32 {
        if self.per_beat > 0.0 { self.per_beat } else { 4.0 }
    }

    /// The step playing at this beat.
    pub fn step_at(&self, beats: f32) -> usize {
        ((beats * self.rate()).floor() as i64).rem_euclid(self.len() as i64) as usize
    }

    /// The track's value at this beat (0..1).
    pub fn value_at(&self, beats: f32) -> f32 {
        if self.steps.is_empty() {
            return 0.0;
        }
        let pos = beats * self.rate();
        let i = self.step_at(beats);
        let v = self.steps[i];
        if self.gate {
            return if v >= 0.5 { 1.0 } else { 0.0 };
        }
        if self.glide {
            let next = self.steps[(i + 1) % self.steps.len()];
            let f = pos - pos.floor();
            return (v * (1.0 - f) + next * f).clamp(0.0, 1.0);
        }
        v.clamp(0.0, 1.0)
    }

    /// A new length; a longer track repeats the pattern it had.
    pub fn resize(&mut self, n: usize) {
        let n = n.clamp(1, 64);
        let old = if self.steps.is_empty() { vec![0.0] } else { self.steps.clone() };
        self.steps = (0..n).map(|i| old[i % old.len()]).collect();
    }

    /// Rotates the pattern by `by` steps (positive = later).
    pub fn shift(&mut self, by: i32) {
        let n = self.steps.len() as i32;
        if n > 0 {
            self.steps.rotate_right(by.rem_euclid(n) as usize);
        }
    }

    /// A random pattern from `seed`: levels anywhere, gates on about four steps in ten.
    pub fn randomize(&mut self, seed: u64) {
        let mut x = seed | 1;
        for s in &mut self.steps {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let r = (x >> 40) as f32 / (1u64 << 24) as f32;
            *s = if self.gate { (r < 0.4) as u8 as f32 } else { r };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_follow_the_beat() {
        let t = StepTrack { steps: vec![0.0, 0.25, 0.5, 1.0], per_beat: 4.0, ..Default::default() };
        assert_eq!(t.step_at(0.0), 0);
        assert_eq!(t.step_at(0.3), 1);
        assert_eq!(t.step_at(0.99), 3);
        assert_eq!(t.step_at(1.0), 0, "four sixteenths = one beat, then round again");
        assert_eq!(t.value_at(0.55), 0.5);
        let glide = StepTrack { glide: true, ..t.clone() };
        assert!((glide.value_at(0.125) - 0.125).abs() < 1e-5, "half way from 0 to 0.25");
        let gate = StepTrack { gate: true, ..t };
        assert_eq!((gate.value_at(0.3), gate.value_at(0.55)), (0.0, 1.0));
    }

    #[test]
    fn resize_shift_and_random() {
        let mut t = StepTrack { steps: vec![1.0, 0.0], ..Default::default() };
        t.resize(5);
        assert_eq!(t.steps, [1.0, 0.0, 1.0, 0.0, 1.0]);
        t.shift(1);
        assert_eq!(t.steps, [1.0, 1.0, 0.0, 1.0, 0.0]);
        t.gate = true;
        t.resize(64);
        t.randomize(7);
        assert!(t.steps.iter().all(|&v| v == 0.0 || v == 1.0));
        let on = t.steps.iter().filter(|&&v| v == 1.0).count();
        assert!((10..=45).contains(&on), "{on} of 64 on");
    }
}
