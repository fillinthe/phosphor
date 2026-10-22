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
    SoftLight,
    HardLight,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    ColorBurn,
    ColorDodge,
    Divide,
    /// The layer's hue / saturation / colour / brightness with the rest from below.
    Hue,
    Saturation,
    Color,
    Luminosity,
    /// Wipes: the layer comes in by a shape as its opacity goes from 0 to 1 (a transition you play with the fader).
    WipeRight,
    WipeDown,
    WipeRadial,
    WipeDiamond,
    WipeBlocks,
    Dissolve,
    /// Track mattes: the layer isn't shown; its brightness (or alpha) cuts out the layers below it.
    LumaMask,
    InverseLumaMask,
    AlphaMask,
}

impl BlendMode {
    pub const ALL: [BlendMode; 32] = [
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
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
        BlendMode::ColorBurn,
        BlendMode::ColorDodge,
        BlendMode::Divide,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
        BlendMode::WipeRight,
        BlendMode::WipeDown,
        BlendMode::WipeRadial,
        BlendMode::WipeDiamond,
        BlendMode::WipeBlocks,
        BlendMode::Dissolve,
        BlendMode::LumaMask,
        BlendMode::InverseLumaMask,
        BlendMode::AlphaMask,
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
            BlendMode::SoftLight => "Soft light",
            BlendMode::HardLight => "Hard light",
            BlendMode::VividLight => "Vivid light",
            BlendMode::LinearLight => "Linear light",
            BlendMode::PinLight => "Pin light",
            BlendMode::HardMix => "Hard mix",
            BlendMode::ColorBurn => "Colour burn",
            BlendMode::ColorDodge => "Colour dodge",
            BlendMode::Divide => "Divide",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::Color => "Colour",
            BlendMode::Luminosity => "Luminosity",
            BlendMode::WipeRight => "Wipe (left to right)",
            BlendMode::WipeDown => "Wipe (top down)",
            BlendMode::WipeRadial => "Wipe (from the centre)",
            BlendMode::WipeDiamond => "Wipe (diamond)",
            BlendMode::WipeBlocks => "Wipe (blocks)",
            BlendMode::Dissolve => "Dissolve",
            BlendMode::LumaMask => "Mask (brightness)",
            BlendMode::InverseLumaMask => "Mask (inverted)",
            BlendMode::AlphaMask => "Mask (alpha)",
        }
    }

    /// Comes in by a shape as the opacity rises (opacity = how far the wipe has gone).
    pub fn is_wipe(self) -> bool {
        (26..=31).contains(&self.shader_index())
    }

    /// Cuts the layers below instead of drawing itself.
    pub fn is_mask(self) -> bool {
        matches!(self, BlendMode::LumaMask | BlendMode::InverseLumaMask | BlendMode::AlphaMask)
    }

    /// Index understood by `blend()` / `fs_composite` in composite.wgsl (10–12 = masks; modes added later
    /// take 13 on, so the first ones keep their numbers).
    pub(crate) fn shader_index(self) -> u32 {
        match self {
            BlendMode::LumaMask => 10,
            BlendMode::InverseLumaMask => 11,
            BlendMode::AlphaMask => 12,
            m if m.index() >= 10 => m.index() as u32 + 3,
            m => m.index() as u32,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FitMode {
    Fit,
    #[default]
    Fill,
    Stretch,
    /// Pixel for pixel: the picture's own size, centred (cropped or bordered, never scaled).
    Copy,
}

impl FitMode {
    pub const ALL: [FitMode; 4] = [FitMode::Fit, FitMode::Fill, FitMode::Stretch, FitMode::Copy];

    pub fn name(self) -> &'static str {
        match self {
            FitMode::Fit => "Fit",
            FitMode::Fill => "Fill",
            FitMode::Stretch => "Stretch",
            FitMode::Copy => "1:1 pixels",
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

    /// Opacity multiplier at crossfader position `x` (0 = A, 1 = B) with that curve.
    pub fn gain(self, x: f32, curve: FaderCurve) -> f32 {
        let x = x.clamp(0.0, 1.0);
        // How far towards the other side this side is (0 = at its own end, 1 = at the far end).
        let away = match self {
            CrossGroup::None => return 1.0,
            CrossGroup::A => x,
            CrossGroup::B => 1.0 - x,
        };
        match curve {
            FaderCurve::FullInMiddle => (2.0 * (1.0 - away)).min(1.0),
            FaderCurve::Straight => 1.0 - away,
            FaderCurve::Smooth => (away * std::f32::consts::FRAC_PI_2).cos().max(0.0),
            FaderCurve::Cut => if away < 0.5 || (away == 0.5 && self == CrossGroup::A) { 1.0 } else { 0.0 },
        }
    }
}

/// How the crossfader shares out A and B.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaderCurve {
    /// Both full at the centre; each fades over its far half.
    #[default]
    FullInMiddle,
    /// Each fades the whole way across (both at half in the middle: a dip).
    Straight,
    /// Equal power: no dip in the middle (cos / sin).
    Smooth,
    /// A hard switch at the centre.
    Cut,
}

impl FaderCurve {
    pub const ALL: [FaderCurve; 4] = [FaderCurve::FullInMiddle, FaderCurve::Smooth, FaderCurve::Straight, FaderCurve::Cut];

    pub fn name(self) -> &'static str {
        match self {
            FaderCurve::FullInMiddle => "Both full in the middle",
            FaderCurve::Smooth => "Smooth (no dip)",
            FaderCurve::Straight => "Straight (dip in the middle)",
            FaderCurve::Cut => "Cut at the middle",
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
    /// A random frame (between the in and out points) each frame time; speed sets how often it jumps.
    Random,
}

impl PlayDirection {
    pub const ALL: [PlayDirection; 4] = [PlayDirection::Forward, PlayDirection::Reverse, PlayDirection::PingPong, PlayDirection::Random];

    pub fn name(self) -> &'static str {
        match self {
            PlayDirection::Forward => "Forward",
            PlayDirection::Reverse => "Reverse",
            PlayDirection::PingPong => "Ping-pong",
            PlayDirection::Random => "Random frames",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    #[default]
    Crossfade,
    DipToBlack,
    Wipe,
    /// Blend transitions: the new source comes in with that blend mode over the old, then the old fades away.
    Add,
    Screen,
    Subtract,
    Multiply,
    Lighten,
    Darken,
    Difference,
    Overlay,
    /// Both pictures pass through an effect (the layer's `transition_effect`) that peaks half-way, under a crossfade.
    Effect,
    /// A shader transition (an ISF transition: cube, doorway, morph…), named by the layer's `transition_effect`.
    Shader,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 13] = [
        TransitionKind::Crossfade,
        TransitionKind::DipToBlack,
        TransitionKind::Wipe,
        TransitionKind::Add,
        TransitionKind::Screen,
        TransitionKind::Subtract,
        TransitionKind::Multiply,
        TransitionKind::Lighten,
        TransitionKind::Darken,
        TransitionKind::Difference,
        TransitionKind::Overlay,
        TransitionKind::Effect,
        TransitionKind::Shader,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TransitionKind::Crossfade => "Crossfade",
            TransitionKind::DipToBlack => "Dip to black",
            TransitionKind::Wipe => "Wipe",
            TransitionKind::Add => "Add",
            TransitionKind::Screen => "Screen",
            TransitionKind::Subtract => "Subtract",
            TransitionKind::Multiply => "Multiply",
            TransitionKind::Lighten => "Lighten",
            TransitionKind::Darken => "Darken",
            TransitionKind::Difference => "Difference",
            TransitionKind::Overlay => "Overlay",
            TransitionKind::Effect => "Through an effect",
            TransitionKind::Shader => "Shader transition",
        }
    }

    /// The blend mode a blend transition passes through.
    pub fn blend(self) -> Option<BlendMode> {
        Some(match self {
            TransitionKind::Add => BlendMode::Add,
            TransitionKind::Screen => BlendMode::Screen,
            TransitionKind::Subtract => BlendMode::Subtract,
            TransitionKind::Multiply => BlendMode::Multiply,
            TransitionKind::Lighten => BlendMode::Lighten,
            TransitionKind::Darken => BlendMode::Darken,
            TransitionKind::Difference => BlendMode::Difference,
            TransitionKind::Overlay => BlendMode::Overlay,
            _ => return None,
        })
    }

    /// `fs_crossfade` mode: 0 crossfade, 1 dip, 2 wipe, 3 blend (mode in p[1].x).
    pub(crate) fn shader_index(self) -> u32 {
        // Effect and Shader transitions crossfade underneath (Shader falls back to it when its shader isn't there).
        if matches!(self, TransitionKind::Effect | TransitionKind::Shader) {
            0
        } else if self.blend().is_some() {
            3
        } else {
            self.index() as u32
        }
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
    /// Effect id for `TransitionKind::Effect`, transition id for `Shader` (empty / not found = plain crossfade).
    pub effect: String,
    /// The shader transition's settings (see `LayerState::transition_values`).
    pub values: Vec<f32>,
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

/// How often the engine draws a frame (an app preference, not part of the project). Projectors and most
/// screens are 60 Hz; a 120 Hz laptop display would otherwise make the engine draw twice as many frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderRate {
    /// Every display refresh (120 fps on a ProMotion Mac).
    Display,
    #[default]
    Fps60,
    Fps30,
}

impl RenderRate {
    pub const ALL: [RenderRate; 3] = [RenderRate::Display, RenderRate::Fps60, RenderRate::Fps30];

    pub fn name(self) -> &'static str {
        match self {
            RenderRate::Display => "Display rate",
            RenderRate::Fps60 => "60 fps",
            RenderRate::Fps30 => "30 fps",
        }
    }

    /// Frames per second to pace to; None = every display refresh.
    pub fn fps(self) -> Option<u32> {
        match self {
            RenderRate::Display => None,
            RenderRate::Fps60 => Some(60),
            RenderRate::Fps30 => Some(30),
        }
    }
}

/// How clicking/triggering media puts it on a layer: cut or fade, now or on the beat/bar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchSettings {
    pub fade: bool,
    pub fade_secs: f32,
    pub timing: LaunchTiming,
    /// Fade style new layers start with; set from the launch bar it goes to every layer (`Composition::set_launch_style`).
    pub style: TransitionKind,
    /// Scan every movie that lands in the grid for its visual beat and lock it to the tempo (app: Match the beat).
    pub auto_beat_match: bool,
    /// Effect the "Through an effect" fade style uses (effect id); set from the launch bar it goes to every layer.
    pub transition_effect: String,
    /// Keep the next movie clips open and paused on their first frame, so they start the moment they're triggered.
    pub preload: bool,
}

impl Default for LaunchSettings {
    fn default() -> Self {
        Self { fade: true, fade_secs: 0.5, timing: LaunchTiming::Now, style: TransitionKind::Crossfade, auto_beat_match: true, transition_effect: "zoom_blur".into(), preload: true }
    }
}

#[cfg(test)]
mod crossfader_tests {
    use super::{CrossGroup, FaderCurve};

    #[test]
    fn both_sides_full_at_the_centre_and_alone_at_the_ends() {
        let c = FaderCurve::FullInMiddle;
        assert_eq!((CrossGroup::A.gain(0.0, c), CrossGroup::B.gain(0.0, c)), (1.0, 0.0));
        assert_eq!((CrossGroup::A.gain(0.5, c), CrossGroup::B.gain(0.5, c)), (1.0, 1.0));
        assert_eq!((CrossGroup::A.gain(1.0, c), CrossGroup::B.gain(1.0, c)), (0.0, 1.0));
        assert_eq!(CrossGroup::None.gain(1.0, c), 1.0);
        // Every curve: full A at the left, full B at the right.
        for c in FaderCurve::ALL {
            assert_eq!((CrossGroup::A.gain(0.0, c), CrossGroup::B.gain(0.0, c)), (1.0, 0.0), "{c:?}");
            assert!(CrossGroup::A.gain(1.0, c) < 1e-6 && CrossGroup::B.gain(1.0, c) == 1.0, "{c:?}");
        }
        assert_eq!(CrossGroup::A.gain(0.5, FaderCurve::Straight), 0.5);
        let (a, b) = (CrossGroup::A.gain(0.5, FaderCurve::Smooth), CrossGroup::B.gain(0.5, FaderCurve::Smooth));
        assert!((a * a + b * b - 1.0).abs() < 1e-5, "equal power");
        assert_eq!((CrossGroup::A.gain(0.4, FaderCurve::Cut), CrossGroup::B.gain(0.4, FaderCurve::Cut)), (1.0, 0.0));
        assert_eq!((CrossGroup::A.gain(0.6, FaderCurve::Cut), CrossGroup::B.gain(0.6, FaderCurve::Cut)), (0.0, 1.0));
    }
}

#[cfg(test)]
mod transition_tests {
    use super::{BlendMode, TransitionKind};

    #[test]
    fn blend_transitions_use_their_blend_mode_and_old_ones_keep_their_shader_index() {
        assert_eq!(TransitionKind::Wipe.shader_index(), 2);
        assert_eq!(TransitionKind::Crossfade.blend(), None);
        for k in TransitionKind::ALL.iter().filter(|k| k.blend().is_some()) {
            assert_eq!(k.shader_index(), 3);
            assert_eq!(k.blend().unwrap().name(), k.name());
        }
        assert_eq!(TransitionKind::Subtract.blend(), Some(BlendMode::Subtract));
    }
}

/// Auto show: the show runs itself. Every few bars the next column with clips fires (on the launch quantise),
/// and a jump in the music's level (a drop) fires Surprise. Saved with the project.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoShow {
    pub enabled: bool,
    /// Bars between column changes.
    pub bars: u32,
    /// Step through the columns that have clips.
    pub columns: bool,
    /// Surprise when the level jumps well above its recent average.
    pub surprise_on_drops: bool,
    /// Every layer reacts to the music (audio-driven modulation on generators and effects), re-applied after
    /// each Surprise.
    pub react: bool,
}

impl Default for AutoShow {
    fn default() -> Self {
        Self { enabled: false, bars: 8, columns: true, surprise_on_drops: true, react: true }
    }
}
