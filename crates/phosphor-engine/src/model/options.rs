//! Small pick-one settings: blend, fit, crossfader side, play direction, transitions, launch timing.

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Difference,
    Lighten,
    Darken,
    Subtract,
    Exclusion,
}

impl BlendMode {
    pub const ALL: [BlendMode; 10] = [
        BlendMode::Normal,
        BlendMode::Add,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Difference,
        BlendMode::Lighten,
        BlendMode::Darken,
        BlendMode::Subtract,
        BlendMode::Exclusion,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Add => "Add",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::Difference => "Difference",
            BlendMode::Lighten => "Lighten",
            BlendMode::Darken => "Darken",
            BlendMode::Subtract => "Subtract",
            BlendMode::Exclusion => "Exclusion",
        }
    }

    /// Index understood by `blend()` in composite.wgsl.
    pub(crate) fn shader_index(self) -> u32 {
        self.index() as u32
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FitMode {
    Fit,
    #[default]
    Fill,
    Stretch,
}

impl FitMode {
    pub const ALL: [FitMode; 3] = [FitMode::Fit, FitMode::Fill, FitMode::Stretch];

    pub fn name(self) -> &'static str {
        match self {
            FitMode::Fit => "Fit",
            FitMode::Fill => "Fill",
            FitMode::Stretch => "Stretch",
        }
    }
}

/// Crossfader side of a layer: A is full with the fader left, B with it right; both are full at the centre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossGroup {
    #[default]
    None,
    A,
    B,
}

impl CrossGroup {
    pub const ALL: [CrossGroup; 3] = [CrossGroup::None, CrossGroup::A, CrossGroup::B];

    pub fn name(self) -> &'static str {
        match self {
            CrossGroup::None => "–",
            CrossGroup::A => "A",
            CrossGroup::B => "B",
        }
    }

    /// Opacity multiplier at crossfader position `x` (0 = A, 1 = B).
    pub fn gain(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        match self {
            CrossGroup::None => 1.0,
            CrossGroup::A => (2.0 * (1.0 - x)).min(1.0),
            CrossGroup::B => (2.0 * x).min(1.0),
        }
    }
}

/// Which way a movie plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayDirection {
    #[default]
    Forward,
    Reverse,
    /// Forward to the out point, back to the in point, and again.
    PingPong,
}

impl PlayDirection {
    pub const ALL: [PlayDirection; 3] = [PlayDirection::Forward, PlayDirection::Reverse, PlayDirection::PingPong];

    pub fn name(self) -> &'static str {
        match self {
            PlayDirection::Forward => "Forward",
            PlayDirection::Reverse => "Reverse",
            PlayDirection::PingPong => "Ping-pong",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    #[default]
    Crossfade,
    DipToBlack,
    Wipe,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 3] = [TransitionKind::Crossfade, TransitionKind::DipToBlack, TransitionKind::Wipe];

    pub fn name(self) -> &'static str {
        match self {
            TransitionKind::Crossfade => "Crossfade",
            TransitionKind::DipToBlack => "Dip to black",
            TransitionKind::Wipe => "Wipe",
        }
    }

    pub(crate) fn shader_index(self) -> u32 {
        self.index() as u32
    }
}

/// A source change in progress: the engine keeps rendering `from` and blends it with the new
/// source until `start + duration`. Never saved.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub from: SourceSpec,
    pub from_values: Vec<f32>,
    pub start: f32,
    pub duration: f32,
    pub kind: TransitionKind,
}

impl Transition {
    pub fn progress(&self, now: f32) -> f32 {
        ((now - self.start) / self.duration.max(0.001)).clamp(0.0, 1.0)
    }
}

/// When a clip/source change lands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchTiming {
    #[default]
    Now,
    Beat,
    Bar,
}

impl LaunchTiming {
    pub const ALL: [LaunchTiming; 3] = [LaunchTiming::Now, LaunchTiming::Beat, LaunchTiming::Bar];

    pub fn name(self) -> &'static str {
        match self {
            LaunchTiming::Now => "Now",
            LaunchTiming::Beat => "On beat",
            LaunchTiming::Bar => "On bar",
        }
    }
}

/// How clicking/triggering media puts it on a layer: cut or fade, now or on the beat/bar.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchSettings {
    pub fade: bool,
    pub fade_secs: f32,
    pub timing: LaunchTiming,
}

impl Default for LaunchSettings {
    fn default() -> Self {
        Self { fade: true, fade_secs: 0.5, timing: LaunchTiming::Now }
    }
}

#[cfg(test)]
mod crossfader_tests {
    use super::CrossGroup;

    #[test]
    fn both_sides_full_at_the_centre_and_alone_at_the_ends() {
        assert_eq!((CrossGroup::A.gain(0.0), CrossGroup::B.gain(0.0)), (1.0, 0.0));
        assert_eq!((CrossGroup::A.gain(0.5), CrossGroup::B.gain(0.5)), (1.0, 1.0));
        assert_eq!((CrossGroup::A.gain(1.0), CrossGroup::B.gain(1.0)), (0.0, 1.0));
        assert_eq!(CrossGroup::None.gain(1.0), 1.0);
    }
}
