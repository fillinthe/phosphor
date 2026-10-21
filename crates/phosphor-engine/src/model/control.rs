//! Controllers: what a MIDI/OSC/key input is, what it drives, ranges, mappings, app-wide controls.

use serde::{Deserialize, Serialize};

/// Where an external controller (MIDI, OSC) message comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlSource {
    MidiCc { channel: u8, cc: u8 },
    MidiNote { channel: u8, note: u8 },
    Osc { address: String },
    Key { key: String },
    /// A game controller's button, stick or trigger (gilrs name: "South", "LeftStickX", "RightTrigger2"…).
    Gamepad { input: String },
    /// A DMX channel (1–512) of a universe coming in over Art-Net or sACN (a lighting desk).
    Dmx { universe: u16, channel: u16 },
}

impl ControlSource {
    pub fn label(&self) -> String {
        match self {
            ControlSource::MidiCc { channel, cc } => format!("MIDI ch{} CC{cc}", channel + 1),
            ControlSource::MidiNote { channel, note } => format!("MIDI ch{} note {note}", channel + 1),
            ControlSource::Osc { address } => format!("OSC {address}"),
            ControlSource::Key { key } => format!("Key {key}"),
            ControlSource::Gamepad { input } => format!("Gamepad {input}"),
            ControlSource::Dmx { universe, channel } => format!("DMX u{universe} ch{channel}"),
        }
    }

    /// Press / release (pads, keys, gamepad buttons) rather than a value that moves (knobs, sticks, triggers).
    pub fn is_momentary(&self) -> bool {
        match self {
            ControlSource::MidiNote { .. } | ControlSource::Key { .. } => true,
            ControlSource::Gamepad { input } => !(input.contains("Stick") || input.ends_with('Z') || input.ends_with("Trigger2") || input.starts_with("DPadX") || input.starts_with("DPadY")),
            _ => false,
        }
    }
}

/// What a controller drives. Values arrive normalised 0..1.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlTarget {
    MasterFade,
    LayerOpacity(u64),
    LayerEnabled(u64),
    GeneratorParam { layer: u64, index: usize },
    EffectParam { effect: u64, index: usize },
    EffectMix(u64),
    EffectEnabled(u64),
    TriggerClip { layer: u64, slot: usize },
    TriggerColumn(usize),
    LayerSpeed(u64),
    LayerFxBypass(u64),
    /// Movie loop in / out point (0..1 of the movie).
    LayerLoopIn(u64),
    LayerLoopOut(u64),
    /// Forward / reverse / ping-pong (0..1 picks one of three).
    LayerDirection(u64),
    /// Jump a movie to a point (0..1 of its length).
    LayerScrub(u64),
    /// A widget on the Control Surface (its main value).
    Surface(u64),
    /// An XY pad's vertical value.
    SurfaceY(u64),
    /// A shared modulator's hand-set value (its "Hand / MIDI" source): one knob feeding a chain.
    Modulator(u64),
    /// App-wide controls (tempo, launch settings, audio…).
    App(AppControl),
    /// A movie layer's sound volume (0..1 = silent..2×).
    LayerVolume(u64),
    /// Layer transform: position (0..1 = left/bottom..right/top), scale (0..1 = 0.05×..4×, log), rotation (0..1 = -180..180°).
    LayerX(u64),
    LayerY(u64),
    LayerScale(u64),
    LayerRotation(u64),
    /// One coordinate of a corner-pinned layer's corner: `point` 0–7 = (top-left x, y, top-right x, y, bottom-right
    /// x, y, bottom-left x, y), 0..1 across the canvas.
    LayerCorner { layer: u64, point: u8 },
    /// A mapping slice's opacity and on/off (MadMapper-style surface control; ids are unique across outputs).
    SliceOpacity(u64),
    SliceEnabled(u64),
}

/// App-wide mappable controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AppControl {
    Tempo,
    TapTempo,
    Resync,
    FollowAudio,
    LaunchFade,
    FadeTime,
    LaunchTiming,
    LaunchStyle,
    MasterFxBypass,
    AudioGain,
    KickThreshold,
    Syphon,
    Ndi,
    Crossfader,
    /// Which Control Surface page shows (0..1 across the pages).
    SurfacePage,
    /// Movie sound, all layers (0..1 = silent..2×).
    MasterVolume,
    /// LED / DMX output on/off.
    Dmx,
    /// Cue list: next cue / previous cue.
    CueGo,
    CueBack,
    /// Ableton Link on/off.
    Link,
    /// A random look on every layer (`Composition::surprise`).
    Surprise,
    /// New values for the selected layer's generator and effects.
    Shuffle,
    /// Mute every sound Phosphor sends out.
    MuteSound,
    /// Even out the movies' loudness.
    AutoLevel,
    /// Phones / web may control the visuals (off = you take over).
    WebControl,
    /// Every output's projector (PJLink): switch on / switch off / shutter closed or open.
    ProjectorsOn,
    ProjectorsOff,
    ProjectorsShutter,
    /// The crossfader glides to the other side (A ↔ B) over the auto-fade time.
    AutoFade,
}

impl AppControl {
    pub const ALL: [AppControl; 29] = [
        AppControl::Tempo,
        AppControl::TapTempo,
        AppControl::Resync,
        AppControl::FollowAudio,
        AppControl::LaunchFade,
        AppControl::FadeTime,
        AppControl::LaunchTiming,
        AppControl::LaunchStyle,
        AppControl::MasterFxBypass,
        AppControl::AudioGain,
        AppControl::KickThreshold,
        AppControl::Syphon,
        AppControl::Ndi,
        AppControl::Crossfader,
        AppControl::SurfacePage,
        AppControl::MasterVolume,
        AppControl::Dmx,
        AppControl::CueGo,
        AppControl::CueBack,
        AppControl::Link,
        AppControl::Surprise,
        AppControl::Shuffle,
        AppControl::MuteSound,
        AppControl::AutoLevel,
        AppControl::WebControl,
        AppControl::ProjectorsOn,
        AppControl::ProjectorsOff,
        AppControl::ProjectorsShutter,
        AppControl::AutoFade,
    ];

    pub fn name(self) -> &'static str {
        match self {
            AppControl::Tempo => "Tempo (BPM)",
            AppControl::TapTempo => "Tap tempo",
            AppControl::Resync => "Resync beat",
            AppControl::FollowAudio => "Follow audio on/off",
            AppControl::LaunchFade => "Launch: fade on/off",
            AppControl::FadeTime => "Launch: fade time",
            AppControl::LaunchTiming => "Launch: now / beat / bar",
            AppControl::LaunchStyle => "Launch: fade style (all layers)",
            AppControl::MasterFxBypass => "Master FX bypass",
            AppControl::AudioGain => "Audio gain",
            AppControl::KickThreshold => "Kick threshold",
            AppControl::Syphon => "Syphon on/off",
            AppControl::Ndi => "NDI on/off",
            AppControl::Crossfader => "Crossfader (A to B)",
            AppControl::SurfacePage => "Control Surface page",
            AppControl::MasterVolume => "Master volume (movie sound)",
            AppControl::Dmx => "LED / DMX on/off",
            AppControl::CueGo => "Cue list: GO (next cue)",
            AppControl::CueBack => "Cue list: back",
            AppControl::Link => "Ableton Link on/off",
            AppControl::Surprise => "Surprise me (random look, all layers)",
            AppControl::Shuffle => "Shuffle the selected layer",
            AppControl::MuteSound => "Mute all sound",
            AppControl::AutoLevel => "Even out volume levels on/off",
            AppControl::WebControl => "Web control on/off (take over)",
            AppControl::ProjectorsOn => "Projectors: switch on (PJLink)",
            AppControl::ProjectorsOff => "Projectors: switch off (PJLink)",
            AppControl::ProjectorsShutter => "Projectors: shutter closed/open (PJLink)",
            AppControl::AutoFade => "Crossfader: auto-fade to the other side",
        }
    }

    /// The built-in OSC path segment, e.g. /phosphor/tempo.
    pub fn osc(self) -> &'static str {
        match self {
            AppControl::Tempo => "tempo",
            AppControl::TapTempo => "tap",
            AppControl::Resync => "resync",
            AppControl::FollowAudio => "follow",
            AppControl::LaunchFade => "launch/fade",
            AppControl::FadeTime => "launch/time",
            AppControl::LaunchTiming => "launch/timing",
            AppControl::LaunchStyle => "launch/style",
            AppControl::MasterFxBypass => "fxbypass",
            AppControl::AudioGain => "audio/gain",
            AppControl::KickThreshold => "audio/kick",
            AppControl::Syphon => "syphon",
            AppControl::Ndi => "ndi",
            AppControl::Crossfader => "crossfader",
            AppControl::SurfacePage => "surface/page",
            AppControl::MasterVolume => "volume",
            AppControl::Dmx => "dmx",
            AppControl::CueGo => "cue/go",
            AppControl::CueBack => "cue/back",
            AppControl::Link => "link",
            AppControl::Surprise => "surprise",
            AppControl::Shuffle => "shuffle",
            AppControl::MuteSound => "mute",
            AppControl::AutoLevel => "autolevel",
            AppControl::WebControl => "webcontrol",
            AppControl::ProjectorsOn => "projectors/on",
            AppControl::ProjectorsOff => "projectors/off",
            AppControl::ProjectorsShutter => "projectors/shutter",
            AppControl::AutoFade => "crossfader/auto",
        }
    }

    /// On/off controls: pads and keys flip them on each press.
    pub fn is_toggle(self) -> bool {
        matches!(self, AppControl::FollowAudio | AppControl::LaunchFade | AppControl::MasterFxBypass | AppControl::Syphon | AppControl::Ndi | AppControl::Dmx | AppControl::Link | AppControl::MuteSound | AppControl::AutoLevel | AppControl::WebControl | AppControl::ProjectorsShutter)
    }
}

/// How a 0..1 control value lands on its target: rescaled into `min..max`, optionally flipped.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ValueRange {
    pub min: f32,
    pub max: f32,
    pub invert: bool,
}

impl Default for ValueRange {
    fn default() -> Self {
        Self { min: 0.0, max: 1.0, invert: false }
    }
}

impl ValueRange {
    pub fn apply(&self, v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        let v = if self.invert { 1.0 - v } else { v };
        self.min + v * (self.max - self.min)
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The controller value (0..1) that `apply` would turn into `target` (controller feedback).
    pub fn unapply(&self, target: f32) -> f32 {
        let span = self.max - self.min;
        let v = if span.abs() < 1e-6 { 0.0 } else { ((target - self.min) / span).clamp(0.0, 1.0) };
        if self.invert { 1.0 - v } else { v }
    }
}

/// One thing a control drives, with its range. Used by controller mappings and surface widgets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub target: ControlTarget,
    #[serde(default)]
    pub range: ValueRange,
}

fn yes() -> bool {
    true
}

/// A controller input (MIDI, OSC, key) driving a target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlMap {
    pub source: ControlSource,
    pub target: ControlTarget,
    #[serde(default)]
    pub range: ValueRange,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// What happens to the controller's value on the way (VDMX's data-source effects): curve, steps, formula,
    /// smoothing. Applied in order, before the range.
    #[serde(default)]
    pub fx: Vec<ValueFx>,
}

impl ControlMap {
    pub fn new(source: ControlSource, target: ControlTarget) -> Self {
        Self { source, target, range: ValueRange::default(), enabled: true, fx: Vec::new() }
    }

    /// The controller value (0..1) after the instant effects (curve, steps, formula); smoothing is the hub's.
    pub fn shape(&self, v: f32) -> f32 {
        self.fx.iter().fold(v.clamp(0.0, 1.0), |v, fx| fx.apply(v))
    }

    /// Seconds this mapping glides over (0 = it jumps).
    pub fn smoothing(&self) -> f32 {
        self.fx.iter().map(|f| if let ValueFx::Smooth(s) = f { *s } else { 0.0 }).fold(0.0, f32::max)
    }

    /// Seconds each move waits before it lands (0 = straight away).
    pub fn delay(&self) -> f32 {
        self.fx.iter().map(|f| if let ValueFx::Delay(s) = f { *s } else { 0.0 }).sum()
    }
}

/// One step a controller's value goes through (0..1 in, 0..1 out).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ValueFx {
    /// Glide to each new value over this many seconds.
    Smooth(f32),
    /// Snap to this many steps (2 = off/on).
    Steps(u32),
    /// Ease: above 1 slow then fast, below 1 fast then slow (value ^ this).
    Curve(f32),
    /// A formula of `$VAL` (0..1), e.g. `1 - $VAL`, `$VAL * $VAL`, `sin($VAL * 3.14)`; the result is kept in 0..1.
    Formula(String),
    /// Each move lands this many seconds later (an echo of the hand).
    Delay(f32),
}

impl ValueFx {
    pub fn name(&self) -> &'static str {
        match self {
            ValueFx::Smooth(_) => "Smooth",
            ValueFx::Steps(_) => "Steps",
            ValueFx::Curve(_) => "Curve",
            ValueFx::Formula(_) => "Formula",
            ValueFx::Delay(_) => "Delay",
        }
    }

    /// The instant ones; Smooth and Delay pass through here (they work over time).
    pub fn apply(&self, v: f32) -> f32 {
        match self {
            ValueFx::Smooth(_) | ValueFx::Delay(_) => v,
            ValueFx::Steps(n) => {
                let n = (*n).max(2) as f32 - 1.0;
                (v * n).round() / n
            }
            ValueFx::Curve(k) => v.clamp(0.0, 1.0).powf(k.max(0.05)),
            ValueFx::Formula(f) => crate::expr::eval(f, &|name| matches!(name, "$VAL" | "VAL").then_some(v as f64)).map_or(v, |r| (r as f32).clamp(0.0, 1.0)),
        }
    }
}

#[cfg(test)]
mod value_fx_tests {
    use super::*;

    #[test]
    fn value_effects_shape_a_controller() {
        let mut m = ControlMap::new(ControlSource::MidiCc { channel: 0, cc: 1 }, ControlTarget::MasterFade);
        assert_eq!(m.shape(0.3), 0.3);
        m.fx.push(ValueFx::Steps(5));
        assert!((m.shape(0.3) - 0.25).abs() < 1e-6, "5 steps: 0, .25, .5, .75, 1");
        m.fx = vec![ValueFx::Curve(2.0)];
        assert!((m.shape(0.5) - 0.25).abs() < 1e-6);
        m.fx = vec![ValueFx::Formula("1 - $VAL".into())];
        assert!((m.shape(0.2) - 0.8).abs() < 1e-6);
        m.fx = vec![ValueFx::Formula("$VAL * 3".into())];
        assert_eq!(m.shape(0.5), 1.0, "kept in 0..1");
        m.fx = vec![ValueFx::Formula("nonsense(".into())];
        assert_eq!(m.shape(0.4), 0.4, "a broken formula passes the value through");
        m.fx = vec![ValueFx::Smooth(0.5), ValueFx::Curve(1.0)];
        assert_eq!(m.smoothing(), 0.5);
    }
}

#[cfg(test)]
mod gamepad_tests {
    use super::ControlSource;

    #[test]
    fn gamepad_buttons_press_sticks_move() {
        let g = |s: &str| ControlSource::Gamepad { input: s.into() };
        assert!(g("South").is_momentary() && g("LeftTrigger").is_momentary());
        assert!(!g("LeftStickX").is_momentary() && !g("RightTrigger2").is_momentary() && !g("LeftZ").is_momentary());
        assert_eq!(g("South").label(), "Gamepad South");
    }
}
