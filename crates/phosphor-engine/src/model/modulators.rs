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
    /// Halfway between the two (weighted by the amount).
    Average,
}

impl ChainMode {
    pub const ALL: [ChainMode; 6] = [ChainMode::Multiply, ChainMode::Add, ChainMode::Min, ChainMode::Max, ChainMode::Average, ChainMode::Speed];

    pub fn name(self) -> &'static str {
        match self {
            ChainMode::Multiply => "scaled by",
            ChainMode::Add => "plus",
            ChainMode::Min => "lower of it and",
            ChainMode::Max => "higher of it and",
            ChainMode::Speed => "speed set by",
            ChainMode::Average => "averaged with",
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
    /// The "Hand / MIDI knob" source's value (0..1): set by its fader, a mapped knob, OSC or the remote.
    pub value: f32,
    /// Data looper (Hand source): a recorded take of the value that plays back in time with the beat.
    #[serde(default)]
    pub looper: Looper,
    /// Step sequencer track (Steps source): played on the beat grid instead of the 8 simple steps.
    #[serde(default)]
    pub sequence: Option<super::StepTrack>,
    /// A sound band (Hz, from–to): the app sets `value` to how loud it is (Hand source).
    #[serde(default)]
    pub band: Option<[f32; 2]>,
    /// Behind a Designer Number box (`Comp::network`): made and removed with the box.
    #[serde(default)]
    pub designer: bool,
    /// Its value comes from data (a live reading, the web, a timer, the clock, a table): the app fills `value`
    /// (Hand source) and `live`.
    #[serde(default)]
    pub data: Option<super::DataFeed>,
    /// Fires when its value crosses a threshold (sets a control).
    #[serde(default)]
    pub event: Option<super::Trigger>,
    /// What the data last read (not saved).
    #[serde(skip)]
    pub live: super::DataLive,
    /// A Designer number operator: its value is worked out from the number it takes (`chain.from`, and `second`)
    /// instead of from its own signal.
    #[serde(default)]
    pub op: Option<super::NumberOp>,
    /// An operator's second input (a modulator earlier in the list; 0 = nothing wired in).
    #[serde(default)]
    pub second: u64,
    /// Held where it is (its cycle doesn't move).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub paused: bool,
    /// Runs its cycle backwards.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
}

/// Samples per beat in a looper take.
pub const LOOPER_RES: usize = 32;

/// Calls `$f(what it moves, modulation)` for every modulation set on a control in the project (MASTER fade, comps,
/// groups, layers' opacity / transform / generator settings, every effect), read-only or (`mut`) to change.
macro_rules! walk_modulations {
    ($comp:expr, $f:ident, $effects:ident $(, $m:tt)?) => {{
        let comp = $comp;
        if let Some(m) = &$($m)? comp.master_fade_mod {
            $f("Master fade".to_string(), m);
        }
        for c in &$($m)? comp.comps {
            let what = format!("Comp {} · opacity", c.name);
            if let Some(m) = &$($m)? c.opacity_mod {
                $f(what, m);
            }
        }
        for g in &$($m)? comp.groups {
            let what = format!("{} · opacity", g.name);
            if let Some(m) = &$($m)? g.opacity_mod {
                $f(what, m);
            }
        }
        for l in &$($m)? comp.layers {
            let name = l.name.clone();
            if let Some(m) = &$($m)? l.opacity_mod {
                $f(format!("{name} · opacity"), m);
            }
            let t = &$($m)? l.transform;
            for (part, m) in [("position X", &$($m)? t.x_mod), ("position Y", &$($m)? t.y_mod), ("scale", &$($m)? t.scale_mod), ("rotation", &$($m)? t.rotation_mod)] {
                if let Some(m) = m {
                    $f(format!("{name} · {part}"), m);
                }
            }
            let desc = match &l.source {
                super::SourceSpec::Generator(g) => crate::shaders::generator(g),
                _ => None,
            };
            if let Some(desc) = desc {
                for pm in &$($m)? l.generator_mods {
                    let what = format!("{name} · {} · {}", desc.name, desc.params.get(pm.index).map_or("setting", |p| p.name));
                    $f(what, &$($m)? pm.m);
                }
            }
        }
        for fx in comp.$effects() {
            let Some(desc) = crate::shaders::effect(&fx.effect) else { continue };
            for pm in &$($m)? fx.mods {
                let what = format!("{} · {}", desc.name, desc.params.get(pm.index).map_or("setting", |p| p.name));
                $f(what, &$($m)? pm.m);
            }
        }
    }};
}

impl super::Composition {
    /// Sets (or first makes) the Hand modulator called `name` to `v` (preview XY pads, video tracking). Does nothing
    /// when the modulator list is full.
    pub fn set_hand(&mut self, name: &str, v: f32) {
        if let Some(m) = self.modulators.iter_mut().find(|m| m.name == name) {
            m.value = v;
            return;
        }
        if self.modulators.len() >= crate::control::MAX_SHARED {
            return;
        }
        let id = self.new_id();
        let signal = super::Modulation { source: super::ModSource::Hand, ..Default::default() };
        self.modulators.push(Modulator { id, name: name.to_string(), signal, value: v, ..Default::default() });
    }

    /// What follows shared modulator `id`, in words ("Layer 1 · opacity", "Blur · Amount"): the controls whose
    /// modulation is linked to it, and the modulators chained from it.
    pub fn modulator_followers(&self, id: u64) -> Vec<String> {
        let mut out = Vec::new();
        let mut add = |what: String, m: &super::Modulation| {
            if m.link == id {
                out.push(what);
            }
        };
        walk_modulations!(self, add, all_effects);
        for m in &self.modulators {
            if m.chain.is_some_and(|c| c.from == id) {
                out.push(format!("{} (modulator)", m.name));
            }
        }
        out
    }

    /// Every control's modulation linked to shared modulator `id`, with what it moves in words, to change how much
    /// each one follows it (the modulator's window).
    pub fn linked_modulations_mut(&mut self, id: u64, mut f: impl FnMut(String, &mut super::Modulation)) {
        let mut each = |what: String, m: &mut super::Modulation| {
            if m.link == id {
                f(what, m);
            }
        };
        walk_modulations!(self, each, all_effects_mut, mut);
    }

    /// Adds saved modulators (a template: one, or a set chained together) with fresh ids, their chains kept
    /// among themselves. Returns how many were added (fewer than asked once the list is full).
    pub fn add_modulators(&mut self, saved: Vec<Modulator>) -> usize {
        let room = crate::control::MAX_SHARED.saturating_sub(self.modulators.len());
        let ids: Vec<(u64, u64)> = saved.iter().take(room).map(|m| (m.id, self.new_id())).collect();
        for (mut m, (_, id)) in saved.into_iter().zip(&ids) {
            m.id = *id;
            m.designer = false;
            m.chain = m.chain.and_then(|c| ids.iter().find(|(old, _)| *old == c.from && *old != 0).map(|(_, new)| ModChain { from: *new, ..c }));
            self.modulators.push(m);
        }
        ids.len()
    }

    /// The value of the Hand modulator called `name`, if there is one.
    pub fn hand(&self, name: &str) -> Option<f32> {
        self.modulators.iter().find(|m| m.name == name).map(|m| m.value)
    }
}

/// A Hand modulator's looper (VDMX's Data Looper): record the knob's moves for a number of beats, starting on the
/// next multiple of that length so the loop lines up with the music, then play the take back looped.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Looper {
    /// Loop length in beats (0 = 4).
    pub beats: f32,
    /// The take: `LOOPER_RES` values per beat (empty = nothing recorded).
    pub samples: Vec<f32>,
    pub playing: bool,
    /// Recording, from this beat (it waits until then).
    #[serde(skip)]
    pub recording: Option<f32>,
}

impl Looper {
    pub fn len(&self) -> f32 {
        if self.beats > 0.0 { self.beats } else { 4.0 }
    }

    /// Record from the next multiple of the loop length (now, if the clock is right on one).
    pub fn arm(&mut self, beats: f32) {
        let len = self.len();
        self.recording = Some((beats / len - 1e-4).ceil() * len);
        self.samples.clear();
        self.playing = false;
    }

    /// Every frame: while recording, keep `value` at this point of the take; at the end it starts playing.
    pub fn record(&mut self, beats: f32, value: f32) {
        let Some(start) = self.recording else { return };
        if beats < start {
            return;
        }
        let total = (self.len() * LOOPER_RES as f32) as usize;
        let at = ((beats - start) * LOOPER_RES as f32) as usize;
        if at >= total {
            let last = self.samples.last().copied().unwrap_or(value);
            self.samples.resize(total, last);
            self.recording = None;
            self.playing = true;
            return;
        }
        // Frames are slower than the take's resolution: fill the gap with the last value.
        let last = self.samples.last().copied().unwrap_or(value);
        self.samples.resize(at, last);
        self.samples.push(value.clamp(0.0, 1.0));
    }

    /// The take's value at this beat while playing (None while recording or with nothing recorded).
    pub fn value_at(&self, beats: f32) -> Option<f32> {
        if !self.playing || self.recording.is_some() || self.samples.is_empty() {
            return None;
        }
        let pos = beats.rem_euclid(self.len()) * LOOPER_RES as f32;
        let i = (pos as usize).min(self.samples.len() - 1);
        let j = (i + 1) % self.samples.len();
        let f = pos.fract();
        Some(self.samples[i] * (1.0 - f) + self.samples[j] * f)
    }
}

impl Default for Modulator {
    fn default() -> Self {
        Self { id: 0, name: "Modulator".into(), signal: Modulation { source: super::ModSource::LfoSine, sync: true, rate: 4.0, ..Default::default() }, chain: None, value: 0.5, looper: Looper::default(), sequence: None, band: None, designer: false, data: None, event: None, live: Default::default(), op: None, second: 0, paused: false, reverse: false }
    }
}

/// Running phase of each modulator (cycles), so a speed change never makes it jump. Keep one per project and
/// pass it to `evaluate` every frame.
#[derive(Clone, Debug, Default)]
pub struct ModPhases {
    phases: Vec<(u64, f64)>,
    /// What each number operator remembers (a smoothed value, a count, an envelope's clock).
    ops: Vec<(u64, super::OpState)>,
    last_time: Option<f32>,
    last_beats: Option<f32>,
}

impl ModPhases {
    /// Where modulator `id` is in its cycle now (0..1), for drawing its playhead. None before its first frame.
    pub fn cycle_of(&self, id: u64) -> Option<f32> {
        self.phases.iter().find(|(i, _)| *i == id).map(|(_, p)| p.rem_euclid(1.0) as f32)
    }

    /// Puts modulator `id` at this point of its cycle (0..1): 0 = retrigger (back to the start), anything else =
    /// scrub. It goes on from there at its own speed.
    pub fn set_cycle(&mut self, id: u64, at: f32) {
        let at = at.clamp(0.0, 0.9999) as f64;
        match self.phases.iter_mut().find(|(i, _)| *i == id) {
            Some((_, p)) => *p = p.floor() + at,
            None => self.phases.push((id, at)),
        }
    }
}

/// One cycle of a signal's shape for drawing: its value at `x` (0..1 of a cycle; its phase offset applies).
pub fn wave_at(m: &Modulation, x: f32) -> f32 {
    source_value(&Modulation { sync: false, rate: 1.0, link: 0, ..*m }, &ModInputs { time: x, ..Default::default() })
}

/// Every modulator's value now (0..1), in list order: each one may follow (chain) any earlier one. Phases advance
/// by the time / beats since the last call, at each modulator's current speed.
pub fn evaluate(mods: &[Modulator], inputs: &ModInputs, phases: &mut ModPhases) -> [(u64, f32); MAX_SHARED] {
    let dt = phases.last_time.map_or(0.0, |t| (inputs.time - t).clamp(0.0, 0.5)) as f64;
    let db = phases.last_beats.map_or(0.0, |b| (inputs.beats - b).clamp(0.0, 4.0)) as f64;
    phases.last_time = Some(inputs.time);
    phases.last_beats = Some(inputs.beats);
    phases.phases.retain(|(id, _)| mods.iter().any(|m| m.id == *id));
    phases.ops.retain(|(id, _)| mods.iter().any(|m| m.id == *id && m.op.is_some()));
    let mut out = [(0u64, 0.0f32); MAX_SHARED];
    let mut inp = *inputs;
    for (k, m) in mods.iter().take(MAX_SHARED).enumerate() {
        // An operator: worked out from the numbers it takes (earlier in the list), nothing of its own.
        if let Some(op) = &m.op {
            let find = |id: u64| out[..k].iter().find(|(i, _)| *i == id && id != 0).map(|x| x.1);
            let a = m.chain.and_then(|c| find(c.from)).unwrap_or(0.0);
            let b = find(m.second);
            let at = match phases.ops.iter().position(|(id, _)| *id == m.id) {
                Some(at) => at,
                None => {
                    phases.ops.push((m.id, Default::default()));
                    phases.ops.len() - 1
                }
            };
            out[k] = (m.id, op.run(a, b, dt as f32, inputs.time, inputs.beats, &mut phases.ops[at].1));
            inp.shared = out;
            continue;
        }
        let other = m.chain.and_then(|c| out[..k].iter().find(|(id, _)| *id == c.from).map(|x| (c, x.1)));
        // Speed: 0.25× … 4× around the set rate, by the other modulator (amount = how much of that range).
        let speed = match other {
            Some((c, w)) if c.mode == ChainMode::Speed => 4f64.powf(((w as f64) * 2.0 - 1.0) * c.amount.clamp(0.0, 1.0) as f64),
            _ => 1.0,
        };
        let s = &m.signal;
        let step = if s.sync { db / (s.rate.max(1.0 / 16.0) as f64) } else { dt * s.rate as f64 };
        // Paused: held; reversed: the cycle runs backwards.
        let step = if m.paused { 0.0 } else if m.reverse { -step } else { step };
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
        let mut v = if s.source == super::ModSource::Hand {
            let v = m.looper.value_at(inputs.beats).unwrap_or(m.value).clamp(0.0, 1.0);
            if s.invert { 1.0 - v } else { v }
        } else if let (super::ModSource::Steps, Some(track)) = (s.source, &m.sequence) {
            let v = track.value_at(inputs.beats);
            if s.invert { 1.0 - v } else { v }
        } else {
            source_value(&at_phase, &ModInputs { time: phase.rem_euclid(1_000_000.0) as f32, ..inp })
        };
        if let Some((c, w)) = other {
            let a = c.amount.clamp(0.0, 1.0);
            v = match c.mode {
                ChainMode::Multiply => v * (1.0 - a + a * w),
                ChainMode::Add => v + a * w,
                ChainMode::Min => v.min(w + (1.0 - a)),
                ChainMode::Max => v.max(w * a),
                ChainMode::Average => v + (w - v) * a * 0.5,
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
        let square = Modulator { id: 1, name: "gate".into(), signal: Modulation { source: ModSource::LfoSquare, rate: 1.0, ..Default::default() }, chain: None, value: 0.0, looper: Default::default(), sequence: None, band: None, designer: false, data: None, event: None, live: Default::default(), op: None, second: 0, paused: false, reverse: false };
        let saw = Modulator { id: 2, name: "ramp".into(), signal: Modulation { source: ModSource::LfoSaw, rate: 1.0, ..Default::default() }, chain: Some(ModChain { from: 1, mode: ChainMode::Multiply, amount: 1.0 }), value: 0.0, looper: Default::default(), sequence: None, band: None, designer: false, data: None, event: None, live: Default::default(), op: None, second: 0, paused: false, reverse: false };
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
        let knob = Modulator { id: 1, name: "knob".into(), signal: Modulation { source: ModSource::Steps, steps: [1.0; 8], rate: 1.0, ..Default::default() }, chain: None, value: 0.0, looper: Default::default(), sequence: None, band: None, designer: false, data: None, event: None, live: Default::default(), op: None, second: 0, paused: false, reverse: false };
        let saw = Modulator { id: 2, name: "saw".into(), signal: Modulation { source: ModSource::LfoSaw, rate: 0.1, ..Default::default() }, chain: Some(ModChain { from: 1, mode: ChainMode::Speed, amount: 1.0 }), value: 0.0, looper: Default::default(), sequence: None, band: None, designer: false, data: None, event: None, live: Default::default(), op: None, second: 0, paused: false, reverse: false };
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

    #[test]
    fn pause_reverse_and_retrigger() {
        let saw = Modulator { id: 1, signal: Modulation { source: ModSource::LfoSaw, rate: 1.0, ..Default::default() }, ..Default::default() };
        let mut ph = ModPhases::default();
        let at = |t: f32, mods: &[Modulator], ph: &mut ModPhases| evaluate(mods, &ModInputs { time: t, ..Default::default() }, ph)[0].1;
        let mut mods = [saw];
        at(0.0, &mods, &mut ph);
        assert!((at(0.2, &mods, &mut ph) - 0.2).abs() < 1e-4);
        mods[0].paused = true;
        assert!((at(0.5, &mods, &mut ph) - 0.2).abs() < 1e-4, "paused: held");
        mods[0].paused = false;
        mods[0].reverse = true;
        assert!((at(0.6, &mods, &mut ph) - 0.1).abs() < 1e-4, "reversed: back down");
        ph.set_cycle(1, 0.0);
        assert_eq!(ph.cycle_of(1), Some(0.0), "retriggered");
        mods[0].reverse = false;
        ph.set_cycle(1, 0.75);
        assert!((at(0.7, &mods, &mut ph) - 0.85).abs() < 1e-4, "scrubbed, then on from there");
    }

    #[test]
    fn a_window_finds_and_changes_what_follows_a_modulator() {
        let mut comp = crate::Composition::default();
        let l = comp.add_layer().id;
        comp.layer_mut(l).unwrap().opacity_mod = Some(Modulation { link: 9, amount: 0.5, ..Default::default() });
        comp.layer_mut(l).unwrap().transform.scale_mod = Some(Modulation { link: 3, ..Default::default() });
        let name = comp.layer(l).unwrap().name.clone();
        assert_eq!(comp.modulator_followers(9), vec![format!("{name} · opacity")]);
        comp.linked_modulations_mut(9, |_, m| m.amount = 0.25);
        assert_eq!(comp.layer(l).unwrap().opacity_mod.unwrap().amount, 0.25);
        assert_eq!(comp.layer(l).unwrap().transform.scale_mod.unwrap().amount, 0.5, "another modulator's follower is left alone");
    }

    #[test]
    fn a_hand_modulator_feeds_the_chain() {
        let knob = Modulator { id: 1, name: "knob".into(), signal: Modulation { source: ModSource::Hand, ..Default::default() }, chain: None, value: 0.3, looper: Default::default(), sequence: None, band: None, designer: false, data: None, event: None, live: Default::default(), op: None, second: 0, paused: false, reverse: false };
        let lfo = Modulator { id: 2, name: "lfo".into(), signal: Modulation { source: ModSource::LfoSquare, rate: 1.0, ..Default::default() }, chain: Some(ModChain { from: 1, mode: ChainMode::Multiply, amount: 1.0 }), value: 0.0, looper: Default::default(), sequence: None, band: None, designer: false, data: None, event: None, live: Default::default(), op: None, second: 0, paused: false, reverse: false };
        let out = evaluate(&[knob, lfo], &ModInputs { time: 0.25, ..Default::default() }, &mut ModPhases::default());
        assert_eq!(out[0], (1, 0.3));
        assert!((out[1].1 - 0.3).abs() < 1e-6, "{:?}", out[1]);
    }
}

#[cfg(test)]
mod looper_tests {
    use super::*;

    #[test]
    fn a_take_starts_on_the_loop_and_plays_back() {
        let mut l = Looper { beats: 2.0, ..Default::default() };
        l.arm(3.1);
        assert_eq!(l.recording, Some(4.0), "waits for the next 2-beat boundary");
        // A knob going 0 → 1 over the two beats, at 60 fps / 120 bpm (half a beat a second).
        let mut b = 3.1f32;
        while l.recording.is_some() {
            l.record(b, ((b - 4.0) / 2.0).clamp(0.0, 1.0));
            b += 0.5 / 60.0;
        }
        assert!(l.playing && l.samples.len() == 2 * LOOPER_RES);
        assert!(l.value_at(4.0).unwrap() < 0.05, "loop start");
        assert!((l.value_at(5.0).unwrap() - 0.5).abs() < 0.05, "half way");
        assert!((l.value_at(7.0).unwrap() - 0.5).abs() < 0.05, "and again one loop later");
        l.arm(7.5);
        assert_eq!(l.value_at(8.0), None, "recording again: the live value");
    }
}
