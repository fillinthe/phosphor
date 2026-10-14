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
}

pub fn source_value(m: &Modulation, inputs: &ModInputs) -> f32 {
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
        ModSource::AudioBeat => inputs.audio_beat,
    };
    if m.invert { 1.0 - v } else { v }
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
