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
    /// Index track: each step picks one of this many choices (a clip, a blend mode, a direction…); its value is
    /// the middle of that choice's slice of 0..1, the way pick-one controls read it. 0 = plain levels.
    pub choices: u32,
    /// Gate tracks: what each lit step sets to its range's top as it starts, and back as it ends (clips play and
    /// let go, columns fire, anything else switches on and off with the steps).
    pub fires: Vec<super::Binding>,
    /// Colour track: each step's colour (empty = not a colour track), and the colours it sets (each one a colour's
    /// red setting; its green and blue follow). Its own value is the step's brightness.
    pub colours: Vec<[f32; 3]>,
    pub paint: Vec<super::ControlTarget>,
}

impl Default for StepTrack {
    fn default() -> Self {
        Self { steps: [1.0, 0.0, 0.5, 0.0].repeat(4), per_beat: 4.0, gate: false, glide: false, choices: 0, fires: Vec::new(), colours: Vec::new(), paint: Vec::new() }
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

    /// Is step `i` lit (gate tracks)?
    pub fn lit(&self, i: usize) -> bool {
        self.steps.get(i).is_some_and(|v| *v >= 0.5)
    }

    /// What firing does between the last step seen and the one at `beats`: (lit step ended, lit step started).
    /// Nothing while the step is the same.
    pub fn edges(&self, last: Option<usize>, beats: f32) -> (bool, bool) {
        let now = self.step_at(beats);
        if last == Some(now) {
            return (false, false);
        }
        (last.is_some_and(|l| self.lit(l)), self.lit(now))
    }

    pub fn is_colour(&self) -> bool {
        !self.colours.is_empty()
    }

    /// Makes it a colour track: a rainbow across its steps (or plain levels again).
    pub fn set_colour(&mut self, on: bool) {
        self.colours = if on { (0..self.len()).map(|i| hue(i as f32 / self.len() as f32)).collect() } else { Vec::new() };
        if on {
            self.gate = false;
            self.choices = 0;
        }
    }

    /// A colour track's colour at this beat (red, green, blue 0..1); gliding into the next step's.
    pub fn colour_at(&self, beats: f32) -> [f32; 3] {
        if self.colours.is_empty() {
            return [0.0; 3];
        }
        let i = self.step_at(beats) % self.colours.len();
        let c = self.colours[i];
        if !self.glide {
            return c;
        }
        let next = self.colours[(i + 1) % self.colours.len()];
        let pos = beats * self.rate();
        let f = pos - pos.floor();
        [0, 1, 2].map(|k| c[k] + (next[k] - c[k]) * f)
    }

    /// The track's value at this beat (0..1).
    pub fn value_at(&self, beats: f32) -> f32 {
        if self.is_colour() {
            let [r, g, b] = self.colour_at(beats);
            return (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0);
        }
        if self.steps.is_empty() {
            return 0.0;
        }
        let pos = beats * self.rate();
        let i = self.step_at(beats);
        let v = self.steps[i];
        if self.gate {
            return if v >= 0.5 { 1.0 } else { 0.0 };
        }
        if self.choices >= 2 {
            let n = self.choices as usize;
            return crate::pick_unit(crate::pick(v, n), n);
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
        if !self.colours.is_empty() {
            let old = self.colours.clone();
            self.colours = (0..n).map(|i| old[i % old.len()]).collect();
        }
    }

    /// Rotates the pattern by `by` steps (positive = later).
    pub fn shift(&mut self, by: i32) {
        let n = self.steps.len() as i32;
        if n > 0 {
            self.steps.rotate_right(by.rem_euclid(n) as usize);
        }
        let n = self.colours.len() as i32;
        if n > 0 {
            self.colours.rotate_right(by.rem_euclid(n) as usize);
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
        // Colour tracks: a random hue on each step.
        for (c, s) in self.colours.iter_mut().zip(&self.steps) {
            *c = hue(*s);
        }
    }
}

/// A fully saturated colour around the colour wheel (0..1).
fn hue(h: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let f = |n: f32| {
        let k = (n + h) % 6.0;
        1.0 - k.min(4.0 - k).clamp(0.0, 1.0)
    };
    [f(5.0), f(3.0), f(1.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_track_plays_its_colours() {
        let mut t = StepTrack { steps: vec![0.0; 4], per_beat: 1.0, ..Default::default() };
        t.set_colour(true);
        assert_eq!(t.colours.len(), 4);
        assert_eq!(t.colour_at(0.0), [1.0, 0.0, 0.0], "a rainbow from red");
        t.colours[1] = [0.0, 0.0, 1.0];
        assert_eq!(t.colour_at(1.5), [0.0, 0.0, 1.0]);
        assert!((t.value_at(1.5) - 0.0722).abs() < 1e-4, "its value is the brightness");
        t.glide = true;
        t.colours[2] = [1.0, 1.0, 1.0];
        assert_eq!(t.colour_at(1.5), [0.5, 0.5, 1.0], "halfway to the next step");
        t.resize(8);
        assert_eq!((t.colours.len(), t.colours[5]), (8, [0.0, 0.0, 1.0]), "a longer track repeats its colours");
        t.shift(1);
        assert_eq!(t.colours[2], [0.0, 0.0, 1.0]);
        t.set_colour(false);
        assert!(!t.is_colour());
    }

    #[test]
    fn index_steps_pick_whole_choices() {
        let t = StepTrack { steps: vec![0.0, 0.4, 0.99], per_beat: 1.0, choices: 3, glide: true, ..Default::default() };
        // 3 choices: slices 0–⅓, ⅓–⅔, ⅔–1; each step gives its slice's middle (no glide between).
        let picks: Vec<usize> = (0..3).map(|b| crate::pick(t.value_at(b as f32 + 0.5), 3)).collect();
        assert_eq!(picks, vec![0, 1, 2]);
        assert!((t.value_at(1.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn lit_steps_start_and_end_once() {
        let t = StepTrack { steps: vec![1.0, 1.0, 0.0, 1.0], per_beat: 1.0, gate: true, ..Default::default() };
        assert_eq!(t.edges(None, 0.2), (false, true), "first lit step starts");
        assert_eq!(t.edges(Some(0), 0.7), (false, false), "same step: nothing");
        assert_eq!(t.edges(Some(0), 1.1), (true, true), "two lit steps in a row: let go, then again");
        assert_eq!(t.edges(Some(1), 2.0), (true, false), "into a dark step");
        assert_eq!(t.edges(Some(2), 3.5), (false, true));
        assert_eq!(t.edges(Some(3), 4.1), (true, true), "wraps round");
    }

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
