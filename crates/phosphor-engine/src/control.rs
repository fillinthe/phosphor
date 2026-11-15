//! Modulation: turns live signals (audio bands, LFOs) into parameter offsets.

use crate::model::{ModSource, Modulation, ParamMod};
use crate::shaders::ShaderDesc;

/// Signals available this frame, each 0..1 (except `time`, in seconds).
#[derive(Clone, Copy, Debug, Default)]
pub struct ModInputs {
    pub time: f32,
    pub audio_level: f32,
    pub audio_low: f32,
    pub audio_mid: f32,
    pub audio_high: f32,
    /// 0..1 within the current beat.
    pub beat_phase: f32,
    /// Beats elapsed on the BPM clock.
    pub beats: f32,
    /// 1 on each detected audio beat, decaying quickly.
    pub audio_beat: f32,
    /// Shared modulators' values this frame: (id, 0..1), see `model::modulators`.
    pub shared: [(u64, f32); MAX_SHARED],
}

/// How many shared modulators a project can have.
pub const MAX_SHARED: usize = 32;

pub fn source_value(m: &Modulation, inputs: &ModInputs) -> f32 {
    // Following a shared modulator: its value, then this control's own invert.
    if m.link != 0 {
        let v = inputs.shared.iter().find(|(id, _)| *id == m.link).map_or(0.0, |x| x.1);
        return if m.invert { 1.0 - v } else { v };
    }
    let cycles = if m.sync { inputs.beats / m.rate.max(1.0 / 16.0) } else { inputs.time * m.rate };
    let cycles = cycles + m.phase;
    let phase = cycles.rem_euclid(1.0);
    // One random value per whole cycle `n`.
    let random = |n: f32| (n * 12.9898).sin().mul_add(43_758.547, 0.0).rem_euclid(1.0);
    let v = match m.source {
        ModSource::AudioLevel => inputs.audio_level,
        ModSource::AudioLow => inputs.audio_low,
        ModSource::AudioMid => inputs.audio_mid,
        ModSource::AudioHigh => inputs.audio_high,
        ModSource::LfoSine => 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos(),
        ModSource::LfoTriangle => 1.0 - (phase * 2.0 - 1.0).abs(),
        ModSource::LfoSaw => phase,
        ModSource::LfoSquare => if phase < m.width.clamp(0.01, 0.99) { 1.0 } else { 0.0 },
        ModSource::RampDown => 1.0 - phase,
        // (Width 0.5 = phase cubed; wider = closer to a straight ramp, narrower = a later, sharper rush.)
        ModSource::Exp => phase.powf(1.0 + (1.0 - m.width.clamp(0.0, 1.0)) * 4.0),
        ModSource::SmoothRandom => {
            let (a, b) = (random(cycles.floor()), random(cycles.floor() + 1.0));
            let t = phase * phase * (3.0 - 2.0 * phase);
            a + (b - a) * t
        }
        ModSource::Noise => {
            // Three octaves of the smooth random above, each twice as fast and half as strong.
            let octave = |x: f32| {
                let (n, f) = (x.floor(), x.rem_euclid(1.0));
                let t = f * f * (3.0 - 2.0 * f);
                random(n) + (random(n + 1.0) - random(n)) * t
            };
            ((octave(cycles) * 4.0 + octave(cycles * 2.0 + 17.0) * 2.0 + octave(cycles * 4.0 + 43.0)) / 7.0).clamp(0.0, 1.0)
        }
        ModSource::Beat => inputs.beat_phase,
        ModSource::Random => random(cycles.floor()),
        ModSource::Steps => m.steps[((phase * 8.0) as usize).min(7)],
        ModSource::Drawn => drawn_wave(&m.steps, phase),
        ModSource::AudioBeat => inputs.audio_beat,
        // A control's own "hand" source has no value of its own: it follows a shared modulator instead.
        ModSource::Hand => 0.0,
    };
    if m.invert { 1.0 - v } else { v }
}

/// A smooth loop through `points` (evenly spaced, the last running back into the first): Catmull-Rom, kept in 0..1.
pub fn drawn_wave(points: &[f32], phase: f32) -> f32 {
    let n = points.len();
    if n == 0 {
        return 0.0;
    }
    let at = phase.rem_euclid(1.0) * n as f32;
    let i = at.floor() as usize % n;
    let t = at.fract();
    let p = |k: isize| points[(i as isize + k).rem_euclid(n as isize) as usize];
    let (p0, p1, p2, p3) = (p(-1), p(0), p(1), p(2));
    let v = 0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t);
    v.clamp(0.0, 1.0)
}

/// Applies an optional modulation to `base`, keeping the result inside `min..=max`.
pub fn modulate(base: f32, min: f32, max: f32, m: Option<&Modulation>, inputs: &ModInputs) -> f32 {
    let lo = min.min(max);
    let hi = max.max(min);
    match m {
        None => base,
        Some(m) if m.range => {
            let v = source_value(m, inputs);
            (min + (m.min + v * (m.max - m.min)) * (max - min)).clamp(lo, hi)
        }
        Some(m) => (base + m.amount * source_value(m, inputs) * (max - min)).clamp(lo, hi),
    }
}

/// The parameter values a shader should actually use this frame.
pub fn resolve_params(desc: &ShaderDesc, values: &[f32], mods: &[ParamMod], inputs: &ModInputs) -> Vec<f32> {
    desc.params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let base = values.get(i).copied().unwrap_or(p.default);
            let m = mods.iter().find(|pm| pm.index == i).map(|pm| &pm.m);
            modulate(base, p.min, p.max, m, inputs)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modulation_stays_in_range_and_lfos_cycle() {
        let inputs = ModInputs { time: 0.25, audio_level: 1.0, ..Default::default() };
        let m = Modulation { source: ModSource::AudioLevel, amount: 1.0, ..Default::default() };
        assert_eq!(modulate(0.5, 0.0, 1.0, Some(&m), &inputs), 1.0);
        let m = Modulation { source: ModSource::LfoSaw, amount: 1.0, ..Default::default() };
        assert!((modulate(0.0, 0.0, 1.0, Some(&m), &inputs) - 0.25).abs() < 1e-6);
        assert_eq!(modulate(0.3, 0.0, 1.0, None, &inputs), 0.3);
        // Range mode ignores the slider: saw at 0.25 of a 0.2..0.6 window on a 0..10 param.
        let m = Modulation { source: ModSource::LfoSaw, range: true, min: 0.2, max: 0.6, ..Default::default() };
        assert!((modulate(9.0, 0.0, 10.0, Some(&m), &inputs) - 3.0).abs() < 1e-5);
        // Tempo sync: 2 beats per cycle, 1 beat in → halfway.
        let synced = ModInputs { beats: 1.0, ..Default::default() };
        let m = Modulation { source: ModSource::LfoSaw, sync: true, rate: 2.0, ..Default::default() };
        assert!((source_value(&m, &synced) - 0.5).abs() < 1e-5);
        // A phase offset shifts the cycle; pulse width sets how long a square stays on.
        let at = |m: Modulation, time: f32| source_value(&m, &ModInputs { time, ..Default::default() });
        let saw = Modulation { source: ModSource::LfoSaw, ..Default::default() };
        assert!((at(Modulation { phase: 0.5, ..saw }, 0.25) - 0.75).abs() < 1e-5);
        assert!((at(Modulation { phase: 0.5, ..saw }, 0.75) - 0.25).abs() < 1e-5, "wraps");
        let square = Modulation { source: ModSource::LfoSquare, ..Default::default() };
        assert_eq!((at(square, 0.4), at(square, 0.6)), (1.0, 0.0), "old saves: half on");
        assert_eq!(at(Modulation { width: 0.25, ..square }, 0.4), 0.0);
        assert!((at(Modulation { source: ModSource::RampDown, ..saw }, 0.25) - 0.75).abs() < 1e-5);
        // The new shapes stay in 0..1, and the smooth ones don't jump from frame to frame.
        for source in [ModSource::SmoothRandom, ModSource::Noise, ModSource::Exp] {
            let m = Modulation { source, ..Default::default() };
            let mut last = at(m, 0.0);
            for k in 1..2000 {
                let v = at(m, k as f32 * 0.005);
                assert!((0.0..=1.0).contains(&v), "{source:?} {v}");
                assert!(source == ModSource::Exp || (v - last).abs() < 0.08, "{source:?} jumps {last} -> {v}");
                last = v;
            }
        }
        // A modulation saved before phase and width existed reads as before.
        let old: Modulation = serde_json::from_str(r#"{"source":"LfoSquare","amount":0.5,"rate":1.0}"#).unwrap();
        assert_eq!((old.phase, old.width), (0.0, 0.5));
    }
}

#[cfg(test)]
mod drawn_tests {
    #[test]
    fn a_drawn_wave_passes_through_its_points_and_loops() {
        let pts = [0.0, 1.0, 0.0, 1.0, 0.5, 0.5, 0.2, 0.8];
        for (k, p) in pts.iter().enumerate() {
            assert!((super::drawn_wave(&pts, k as f32 / 8.0) - p).abs() < 1e-5, "point {k}");
        }
        assert!((super::drawn_wave(&pts, 1.0) - super::drawn_wave(&pts, 0.0)).abs() < 1e-5, "loops");
        let mid = super::drawn_wave(&pts, 0.5 / 8.0);
        assert!(mid > 0.3 && mid < 0.7, "smooth between: {mid}");
    }
}
