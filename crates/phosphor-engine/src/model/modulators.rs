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
}

impl ChainMode {
    pub const ALL: [ChainMode; 4] = [ChainMode::Multiply, ChainMode::Add, ChainMode::Min, ChainMode::Max];

    pub fn name(self) -> &'static str {
        match self {
            ChainMode::Multiply => "scaled by",
            ChainMode::Add => "plus",
            ChainMode::Min => "lower of it and",
            ChainMode::Max => "higher of it and",
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

/// Every modulator's value now (0..1), in list order: each one may follow (chain) any earlier one.
pub fn evaluate(mods: &[Modulator], inputs: &ModInputs) -> [(u64, f32); MAX_SHARED] {
    let mut out = [(0u64, 0.0f32); MAX_SHARED];
    let mut inp = *inputs;
    for (k, m) in mods.iter().take(MAX_SHARED).enumerate() {
        let mut v = source_value(&m.signal, &inp);
        if let Some(c) = m.chain
            && let Some(w) = out[..k].iter().find(|(id, _)| *id == c.from).map(|x| x.1)
        {
            let a = c.amount.clamp(0.0, 1.0);
            v = match c.mode {
                ChainMode::Multiply => v * (1.0 - a + a * w),
                ChainMode::Add => v + a * w,
                ChainMode::Min => v.min(w + (1.0 - a)),
                ChainMode::Max => v.max(w * a),
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
        let on = evaluate(&[square.clone(), saw.clone()], &ModInputs { time: 0.25, ..Default::default() });
        assert_eq!(on[0], (1, 1.0));
        assert!((on[1].1 - 0.25).abs() < 1e-5, "{:?}", on[1]);
        // t = 0.75 s: the square is off, so the chained saw is closed.
        let off = evaluate(&[square, saw], &ModInputs { time: 0.75, ..Default::default() });
        assert_eq!(off[1], (2, 0.0));
        // A control following modulator 2 reads its value (invert applies on top).
        let inputs = ModInputs { time: 0.25, shared: on, ..Default::default() };
        let follow = Modulation { link: 2, ..Default::default() };
        assert!((source_value(&follow, &inputs) - 0.25).abs() < 1e-5);
        assert!((source_value(&Modulation { invert: true, ..follow }, &inputs) - 0.75).abs() < 1e-5);
    }
}
