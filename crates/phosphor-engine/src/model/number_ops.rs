//! Number operators: a Designer Number box that works on the number wired into it instead of making one. The
//! node-tool family of "channel" boxes: maths on two numbers, smoothing, a new range, a gate, steps, hold, count,
//! an envelope, a ramp whose speed follows the number, a delay, and a formula. What an operator remembers between
//! frames (`OpState`) lives beside the modulators' phases, never in the project.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// How a Math box combines A and B.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MathMode {
    #[default]
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    Difference,
    Average,
    Greater,
    Less,
}

impl MathMode {
    pub const ALL: [MathMode; 10] = [MathMode::Add, MathMode::Subtract, MathMode::Multiply, MathMode::Divide, MathMode::Min, MathMode::Max, MathMode::Difference, MathMode::Average, MathMode::Greater, MathMode::Less];

    pub fn name(self) -> &'static str {
        match self {
            MathMode::Add => "A plus B",
            MathMode::Subtract => "A minus B",
            MathMode::Multiply => "A times B",
            MathMode::Divide => "A divided by B",
            MathMode::Min => "The lower one",
            MathMode::Max => "The higher one",
            MathMode::Difference => "How far apart",
            MathMode::Average => "Halfway between",
            MathMode::Greater => "1 while A is above B",
            MathMode::Less => "1 while A is below B",
        }
    }

    fn apply(self, a: f32, b: f32) -> f32 {
        match self {
            MathMode::Add => a + b,
            MathMode::Subtract => a - b,
            MathMode::Multiply => a * b,
            MathMode::Divide => {
                if b.abs() < 1e-4 {
                    1.0
                } else {
                    a / b
                }
            }
            MathMode::Min => a.min(b),
            MathMode::Max => a.max(b),
            MathMode::Difference => (a - b).abs(),
            MathMode::Average => (a + b) * 0.5,
            MathMode::Greater => (a > b) as u8 as f32,
            MathMode::Less => (a < b) as u8 as f32,
        }
    }
}

/// What a Number box does to the number wired into it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NumberOp {
    /// A and B combined; `b` stands in while nothing is wired into B.
    Math { mode: MathMode, b: f32 },
    /// Follows the number slowly: seconds to rise the whole way, seconds to fall.
    Smooth { rise: f32, fall: f32 },
    /// A part of the number's travel (`from`) stretched onto a new range (`to`); outside it, held at the ends.
    Range { from: [f32; 2], to: [f32; 2] },
    /// 1 while the number is at or above the line, else 0.
    Gate { line: f32 },
    /// The number in that many even steps.
    Steps { count: u32 },
    /// Keeps the number as it was when the second input last rose past the middle.
    Hold,
    /// One step up each time the number rises past the middle, back to 0 after the last step (or when the second
    /// input rises).
    Count { steps: u32 },
    /// Each time the number rises past the middle: up to 1 over `attack` seconds, back down over `release`.
    Envelope { attack: f32, release: f32 },
    /// A ramp from 0 to 1, again and again, as fast as the number says (`rate` cycles a second at 1).
    Speed { rate: f32 },
    /// The number as it was that many seconds ago.
    Delay { seconds: f32 },
    /// A formula of `a`, `b`, `t` (seconds) and `beat`.
    Formula(String),
}

impl NumberOp {
    /// Every operator with its starting settings, in the menu's order.
    pub fn all() -> Vec<NumberOp> {
        vec![
            NumberOp::Math { mode: MathMode::Add, b: 0.5 },
            NumberOp::Smooth { rise: 0.3, fall: 0.3 },
            NumberOp::Range { from: [0.0, 1.0], to: [0.0, 1.0] },
            NumberOp::Gate { line: 0.5 },
            NumberOp::Steps { count: 4 },
            NumberOp::Hold,
            NumberOp::Count { steps: 4 },
            NumberOp::Envelope { attack: 0.05, release: 0.5 },
            NumberOp::Speed { rate: 1.0 },
            NumberOp::Delay { seconds: 0.5 },
            NumberOp::Formula("a * b".into()),
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            NumberOp::Math { .. } => "Math",
            NumberOp::Smooth { .. } => "Smooth",
            NumberOp::Range { .. } => "Range",
            NumberOp::Gate { .. } => "Gate",
            NumberOp::Steps { .. } => "Steps",
            NumberOp::Hold => "Hold",
            NumberOp::Count { .. } => "Count",
            NumberOp::Envelope { .. } => "Envelope",
            NumberOp::Speed { .. } => "Speed",
            NumberOp::Delay { .. } => "Delay",
            NumberOp::Formula(_) => "Formula",
        }
    }

    pub fn about(&self) -> &'static str {
        match self {
            NumberOp::Math { .. } => "Two numbers into one: plus, minus, times, the lower or higher one, or 1 while A is above B",
            NumberOp::Smooth { .. } => "Follows the number slowly: its own time to rise and to fall (takes the jitter out of sound or a sensor)",
            NumberOp::Range { .. } => "Takes a part of the number's travel and stretches it onto a new range (swap the ends to turn it upside down)",
            NumberOp::Gate { .. } => "1 while the number is above a line, else 0: turns a level into on / off",
            NumberOp::Steps { .. } => "The number in even steps instead of a smooth slide",
            NumberOp::Hold => "Keeps the number as it was when the When input last rose: a new value on every beat or kick",
            NumberOp::Count { .. } => "One step up each time the number rises, then around again: step through looks on a beat or a kick",
            NumberOp::Envelope { .. } => "A rise and a fall each time the number rises: a flash on a kick, with your own attack and release",
            NumberOp::Speed { .. } => "A ramp that runs as fast as the number says: a spin that speeds up with the sound",
            NumberOp::Delay { .. } => "The number a moment ago: things that follow one another",
            NumberOp::Formula(_) => "Your own formula of a, b, t (seconds) and beat: sin(t * 0.2) * a, a > b, max(a, b)",
        }
    }

    /// The names of its number inputs.
    pub fn inputs(&self) -> &'static [&'static str] {
        match self {
            NumberOp::Math { .. } => &["A", "B"],
            NumberOp::Hold => &["In", "When"],
            NumberOp::Count { .. } => &["In", "Reset"],
            NumberOp::Formula(_) => &["a", "b"],
            _ => &["In"],
        }
    }

    /// The same operator (whatever its settings).
    pub fn same(&self, other: &NumberOp) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// This frame's value (0..1): `a` = the first input (0 with nothing wired in), `b` = the second while one is
    /// wired, `dt` = seconds since the last frame, `time` / `beats` = the show's clock.
    pub fn run(&self, a: f32, b: Option<f32>, dt: f32, time: f32, beats: f32, s: &mut OpState) -> f32 {
        let first = !s.started;
        s.started = true;
        let rose = |now: f32, before: f32| !first && before < 0.5 && now >= 0.5;
        let (a_rose, b_rose) = (rose(a, s.last_a), b.is_some_and(|b| rose(b, s.last_b)));
        s.last_a = a;
        s.last_b = b.unwrap_or(0.0);
        let v = match self {
            NumberOp::Math { mode, b: fixed } => mode.apply(a, b.unwrap_or(*fixed)),
            NumberOp::Smooth { rise, fall } => {
                let seconds = if a > s.value { *rise } else { *fall };
                s.value = if first || seconds <= 0.001 { a } else { s.value + (a - s.value) * (1.0 - (-dt / seconds * 4.0).exp()) };
                s.value
            }
            NumberOp::Range { from, to } => {
                let span = from[1] - from[0];
                let t = if span.abs() < 1e-5 { (a >= from[0]) as u8 as f32 } else { ((a - from[0]) / span).clamp(0.0, 1.0) };
                to[0] + (to[1] - to[0]) * t
            }
            NumberOp::Gate { line } => (a >= *line) as u8 as f32,
            NumberOp::Steps { count } => {
                let n = (*count).clamp(2, 64) as f32 - 1.0;
                (a * n).round() / n
            }
            NumberOp::Hold => {
                if first || b_rose {
                    s.value = a;
                }
                s.value
            }
            NumberOp::Count { steps } => {
                let n = (*steps).clamp(2, 64) as f32;
                if b_rose {
                    s.value = 0.0;
                } else if a_rose {
                    s.value = (s.value + 1.0) % n;
                }
                s.value / (n - 1.0)
            }
            NumberOp::Envelope { attack, release } => {
                let (attack, release) = (attack.max(0.0), release.max(0.0));
                if first {
                    s.clock = f32::MAX;
                } else if a_rose {
                    s.clock = 0.0;
                } else if s.clock < f32::MAX {
                    s.clock += dt;
                }
                if s.clock < attack {
                    s.clock / attack
                } else if release > 0.0 && s.clock < attack + release {
                    1.0 - (s.clock - attack) / release
                } else {
                    s.clock = f32::MAX;
                    0.0
                }
            }
            NumberOp::Speed { rate } => {
                s.value = (s.value + a * rate * dt).rem_euclid(1.0);
                s.value
            }
            NumberOp::Delay { seconds } => {
                // (A clock that went back, a project opened again: what was kept means nothing now.)
                if s.past.back().is_some_and(|(t, _)| *t > time) {
                    s.past.clear();
                }
                s.past.push_back((time, a));
                let then = time - seconds.clamp(0.0, MAX_DELAY);
                while s.past.len() > 1 && (s.past[1].0 <= then || s.past.len() > MAX_PAST) {
                    s.past.pop_front();
                }
                s.past.front().map_or(a, |x| x.1)
            }
            NumberOp::Formula(f) => {
                let var = |name: &str| match name {
                    "a" | "A" => Some(a as f64),
                    "b" | "B" => Some(b.unwrap_or(0.0) as f64),
                    "t" | "T" => Some(time as f64),
                    "beat" | "BEAT" => Some(beats as f64),
                    _ => None,
                };
                crate::expr::eval(f, &var).map_or(a, |r| r as f32)
            }
        };
        if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }
    }
}

/// The longest a Delay box waits (seconds).
pub const MAX_DELAY: f32 = 10.0;
/// The most frames a Delay box keeps.
const MAX_PAST: usize = 2400;

/// What one operator remembers between frames.
#[derive(Clone, Debug, Default)]
pub struct OpState {
    value: f32,
    last_a: f32,
    last_b: f32,
    started: bool,
    clock: f32,
    past: VecDeque<(f32, f32)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(op: &NumberOp, frames: &[(f32, Option<f32>)]) -> Vec<f32> {
        let mut s = OpState::default();
        frames.iter().enumerate().map(|(i, (a, b))| op.run(*a, *b, 0.1, i as f32 * 0.1, 0.0, &mut s)).collect()
    }

    #[test]
    fn maths_gates_steps_and_ranges() {
        let one = |op: NumberOp, a: f32, b: Option<f32>| run(&op, &[(a, b)])[0];
        assert_eq!(one(NumberOp::Math { mode: MathMode::Add, b: 0.25 }, 0.5, None), 0.75);
        assert_eq!(one(NumberOp::Math { mode: MathMode::Add, b: 0.25 }, 0.5, Some(0.1)), 0.6, "a wire into B wins over the number typed");
        assert_eq!(one(NumberOp::Math { mode: MathMode::Subtract, b: 0.75 }, 0.5, None), 0.0, "never below 0");
        assert_eq!(one(NumberOp::Math { mode: MathMode::Divide, b: 0.0 }, 0.5, None), 1.0, "nothing divided by nothing");
        assert_eq!(one(NumberOp::Math { mode: MathMode::Greater, b: 0.4 }, 0.5, None), 1.0);
        assert_eq!(one(NumberOp::Gate { line: 0.5 }, 0.49, None), 0.0);
        assert_eq!(one(NumberOp::Gate { line: 0.5 }, 0.5, None), 1.0);
        assert_eq!(one(NumberOp::Steps { count: 3 }, 0.4, None), 0.5);
        assert_eq!(one(NumberOp::Range { from: [0.5, 1.0], to: [1.0, 0.0] }, 0.75, None), 0.5);
        assert_eq!(one(NumberOp::Range { from: [0.5, 1.0], to: [1.0, 0.0] }, 0.2, None), 1.0, "held at the end");
        assert_eq!(one(NumberOp::Formula("a * 2 - b".into()), 0.4, Some(0.3)), 0.5);
        assert_eq!(one(NumberOp::Formula("nonsense(".into()), 0.4, None), 0.4, "a formula that can't be read passes a through");
    }

    #[test]
    fn smooth_follows_slowly_and_starts_where_the_number_is() {
        let v = run(&NumberOp::Smooth { rise: 1.0, fall: 0.0 }, &[(0.2, None), (1.0, None), (1.0, None), (0.0, None)]);
        assert_eq!(v[0], 0.2);
        assert!(v[1] > 0.2 && v[1] < 1.0 && v[2] > v[1], "{v:?}");
        assert_eq!(v[3], 0.0, "no fall time: straight down");
    }

    #[test]
    fn hold_count_and_envelope_act_on_a_rise() {
        let hold = run(&NumberOp::Hold, &[(0.1, Some(0.0)), (0.4, Some(0.0)), (0.6, Some(1.0)), (0.9, Some(1.0)), (0.2, Some(0.0))]);
        assert_eq!(hold, vec![0.1, 0.1, 0.6, 0.6, 0.6]);
        let count = run(&NumberOp::Count { steps: 3 }, &[(0.0, None), (1.0, None), (0.0, None), (1.0, None), (0.0, None), (1.0, None)]);
        assert_eq!(count, vec![0.0, 0.5, 0.5, 1.0, 1.0, 0.0], "around again after the last step");
        let reset = run(&NumberOp::Count { steps: 3 }, &[(0.0, Some(0.0)), (1.0, Some(0.0)), (0.0, Some(1.0))]);
        assert_eq!(reset, vec![0.0, 0.5, 0.0]);
        let env = run(&NumberOp::Envelope { attack: 0.1, release: 0.2 }, &[(0.0, None), (1.0, None), (1.0, None), (1.0, None), (1.0, None), (1.0, None)]);
        assert_eq!(env[0], 0.0, "at rest until something rises");
        assert_eq!(env[1], 0.0);
        assert!((env[2] - 1.0).abs() < 1e-4 && env[3] < env[2] && env[3] > 0.0, "{env:?}");
        assert_eq!(env[5], 0.0);
    }

    #[test]
    fn speed_ramps_and_delay_looks_back() {
        let ramp = run(&NumberOp::Speed { rate: 2.0 }, &[(1.0, None), (1.0, None), (0.0, None), (1.0, None)]);
        assert!((ramp[0] - 0.2).abs() < 1e-5 && (ramp[1] - 0.4).abs() < 1e-5 && (ramp[2] - 0.4).abs() < 1e-5, "{ramp:?}");
        let late = run(&NumberOp::Delay { seconds: 0.2 }, &[(0.1, None), (0.2, None), (0.3, None), (0.4, None), (0.5, None)]);
        assert_eq!(late[4], 0.3);
        assert_eq!(late[0], 0.1, "nothing older yet: the number itself");
    }
}
