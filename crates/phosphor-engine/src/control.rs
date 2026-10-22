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
pub const MAX_SHARED: usize = 16;

pub fn source_value(m: &Modulation, inputs: &ModInputs) -> f32 {
    // Following a shared modulator: its value, then this control's own invert.
    if m.link != 0 {
        let v = inputs.shared.iter().find(|(id, _)| *id == m.link).map_or(0.0, |x| x.1);
        return if m.invert { 1.0 - v } else { v };
    }
    let cycles = if m.sync { inputs.beats / m.rate.max(1.0 / 16.0) } else { inputs.time * m.rate };
    let phase = cycles.rem_euclid(1.0);
    let v = match m.source {
        ModSource::AudioLevel => inputs.audio_level,
        ModSource::AudioLow => inputs.audio_low,
        ModSource::AudioMid => inputs.audio_mid,
        ModSource::AudioHigh => inputs.audio_high,
        ModSource::LfoSine => 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos(),
        ModSource::LfoTriangle => 1.0 - (phase * 2.0 - 1.0).abs(),
        ModSource::LfoSaw => phase,
        ModSource::LfoSquare => if phase < 0.5 { 1.0 } else { 0.0 },
        ModSource::Beat => inputs.beat_phase,
        ModSource::Random => {
            let n = cycles.floor();
            (n * 12.9898).sin().mul_add(43_758.547, 0.0).rem_euclid(1.0)
        }
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
