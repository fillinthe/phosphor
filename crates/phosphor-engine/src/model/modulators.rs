//! Shared modulators: named signal sources (LFO, audio band, beat, random, steps) that any number of controls
//! follow, and that can be chained (one modulator scaling or adding to another), VDMX's data sources done the
//! Phosphor way. Saved with the project; evaluated once per frame into `ModInputs::shared`.

use serde::{Deserialize, Serialize};

use super::Modulation;
use crate::control::{MAX_SHARED, ModInputs, source_value};

/// How a chained modulator combines with the one it follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChainMode {
    /// Its value is scaled by the other one (the other one opens and closes it).
    #[default]
    Multiply,
    /// The other one is added on top.
    Add,
    /// The lower of the two.
    Min,
    /// The higher of the two.
    Max,
    /// The other one sets its speed: from a quarter (at 0) to four times (at 1), the middle = as set.
    Speed,
}

impl ChainMode {
    pub const ALL: [ChainMode; 5] = [ChainMode::Multiply, ChainMode::Add, ChainMode::Min, ChainMode::Max, ChainMode::Speed];

    pub fn name(self) -> &'static str {
        match self {
            ChainMode::Multiply => "scaled by",
            ChainMode::Add => "plus",
            ChainMode::Min => "lower of it and",
            ChainMode::Max => "higher of it and",
            ChainMode::Speed => "speed set by",
        }
    }
}

/// One modulator in the chain.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModChain {
    /// The modulator it takes from (must come earlier in the list).
    pub from: u64,
    pub mode: ChainMode,
    /// How much of the other one is used (0 = none, 1 = all of it).
    pub amount: f32,
}

impl Default for ModChain {
    fn default() -> Self {
        Self { from: 0, mode: ChainMode::Multiply, amount: 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Modulator {
    pub id: u64,
    pub name: String,
    /// Its signal (source, rate, sync, steps, invert); amount / range don't apply here, the controls set those.
    pub signal: Modulation,
    pub chain: Option<ModChain>,
}

impl Default for Modulator {
    fn default() -> Self {
        Self { id: 0, name: "Modulator".into(), signal: Modulation { source: super::ModSource::LfoSine, sync: true, rate: 4.0, ..Default::default() }, chain: None }
    }
}

/// Running phase of each modulator (cycles), so a speed change never makes it jump. Keep one per project and
/// pass it to `evaluate` every frame.
#[derive(Clone, Debug, Default)]
pub struct ModPhases {
    phases: Vec<(u64, f64)>,
    last_time: Option<f32>,
    last_beats: Option<f32>,
}

/// Every modulator's value now (0..1), in list order: each one may follow (chain) any earlier one. Phases advance
/// by the time / beats since the last call, at each modulator's current speed.
pub fn evaluate(mods: &[Modulator], inputs: &ModInputs, phases: &mut ModPhases) -> [(u64, f32); MAX_SHARED] {
    let dt = phases.last_time.map_or(0.0, |t| (inputs.time - t).clamp(0.0, 0.5)) as f64;
    let db = phases.last_beats.map_or(0.0, |b| (inputs.beats - b).clamp(0.0, 4.0)) as f64;
    phases.last_time = Some(inputs.time);
    phases.last_beats = Some(inputs.beats);
    phases.phases.retain(|(id, _)| mods.iter().any(|m| m.id == *id));
    let mut out = [(0u64, 0.0f32); MAX_SHARED];
    let mut inp = *inputs;
    for (k, m) in mods.iter().take(MAX_SHARED).enumerate() {
        let other = m.chain.and_then(|c| out[..k].iter().find(|(id, _)| *id == c.from).map(|x| (c, x.1)));
        // Speed: 0.25× … 4× around the set rate, by the other modulator (amount = how much of that range).
        let speed = match other {
            Some((c, w)) if c.mode == ChainMode::Speed => 4f64.powf(((w as f64) * 2.0 - 1.0) * c.amount.clamp(0.0, 1.0) as f64),
            _ => 1.0,
        };
        let s = &m.signal;
        let step = if s.sync { db / (s.rate.max(1.0 / 16.0) as f64) } else { dt * s.rate as f64 };
        let phase = match phases.phases.iter_mut().find(|(id, _)| *id == m.id) {
            Some((_, p)) => {
                *p += step * speed;
                *p
            }
            None => {
                // Start in step with the plain clock (what a control's own LFO would show).
                let p = if s.sync { inputs.beats as f64 / s.rate.max(1.0 / 16.0) as f64 } else { inputs.time as f64 * s.rate as f64 };
                phases.phases.push((m.id, p));
                p
            }
        };
        // The signal at that phase: a free-running one-cycle-per-second view of the same source.
        let at_phase = super::Modulation { sync: false, rate: 1.0, ..*s };
        let mut v = source_value(&at_phase, &ModInputs { time: phase.rem_euclid(1_000_000.0) as f32, ..inp });
        if let Some((c, w)) = other {
            let a = c.amount.clamp(0.0, 1.0);
            v = match c.mode {
                ChainMode::Multiply => v * (1.0 - a + a * w),
                ChainMode::Add => v + a * w,
                ChainMode::Min => v.min(w + (1.0 - a)),
                ChainMode::Max => v.max(w * a),
                ChainMode::Speed => v,
            }
            .clamp(0.0, 1.0);
        }
        out[k] = (m.id, v);
        inp.shared = out;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModSource;

    #[test]
    fn modulators_chain_in_order_and_controls_follow_them() {
        let square = Modulator { id: 1, name: "gate".into(), signal: Modulation { source: ModSource::LfoSquare, rate: 1.0, ..Default::default() }, chain: None };
        let saw = Modulator { id: 2, name: "ramp".into(), signal: Modulation { source: ModSource::LfoSaw, rate: 1.0, ..Default::default() }, chain: Some(ModChain { from: 1, mode: ChainMode::Multiply, amount: 1.0 }) };
        // t = 0.25 s: the square is on (1), the saw at 0.25: 0.25 × 1.
        let on = evaluate(&[square.clone(), saw.clone()], &ModInputs { time: 0.25, ..Default::default() }, &mut ModPhases::default());
        assert_eq!(on[0], (1, 1.0));
        assert!((on[1].1 - 0.25).abs() < 1e-5, "{:?}", on[1]);
        // t = 0.75 s: the square is off, so the chained saw is closed.
        let off = evaluate(&[square, saw], &ModInputs { time: 0.75, ..Default::default() }, &mut ModPhases::default());
        assert_eq!(off[1], (2, 0.0));
        // A control following modulator 2 reads its value (invert applies on top).
        let inputs = ModInputs { time: 0.25, shared: on, ..Default::default() };
        let follow = Modulation { link: 2, ..Default::default() };
        assert!((source_value(&follow, &inputs) - 0.25).abs() < 1e-5);
        assert!((source_value(&Modulation { invert: true, ..follow }, &inputs) - 0.75).abs() < 1e-5);
    }

    #[test]
    fn speed_chaining_changes_the_rate_without_jumps() {
        let knob = Modulator { id: 1, name: "knob".into(), signal: Modulation { source: ModSource::Steps, steps: [1.0; 8], rate: 1.0, ..Default::default() }, chain: None };
        let saw = Modulator { id: 2, name: "saw".into(), signal: Modulation { source: ModSource::LfoSaw, rate: 0.1, ..Default::default() }, chain: Some(ModChain { from: 1, mode: ChainMode::Speed, amount: 1.0 }) };
        let mods = [knob, saw];
        let mut ph = ModPhases::default();
        let mut prev = evaluate(&mods, &ModInputs { time: 0.0, ..Default::default() }, &mut ph)[1].1;
        // The knob is at 1: four times 0.1 Hz = 0.4 cycles a second. Small steps never jump.
        for k in 1..=10 {
            let v = evaluate(&mods, &ModInputs { time: k as f32 * 0.1, ..Default::default() }, &mut ph)[1].1;
            assert!((v - prev - 0.04).abs() < 1e-3, "step {k}: {prev} -> {v}");
            prev = v;
        }
    }
}
