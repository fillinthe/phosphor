//! Controllers: what a MIDI/OSC/key input is, what it drives, ranges, mappings, app-wide controls.

use serde::{Deserialize, Serialize};

/// Where an external controller (MIDI, OSC) message comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlSource {
    MidiCc { channel: u8, cc: u8 },
    MidiNote { channel: u8, note: u8 },
    Osc { address: String },
    Key { key: String },
}

impl ControlSource {
    pub fn label(&self) -> String {
        match self {
            ControlSource::MidiCc { channel, cc } => format!("MIDI ch{} CC{cc}", channel + 1),
            ControlSource::MidiNote { channel, note } => format!("MIDI ch{} note {note}", channel + 1),
            ControlSource::Osc { address } => format!("OSC {address}"),
            ControlSource::Key { key } => format!("Key {key}"),
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
    /// App-wide controls (tempo, launch settings, audio…).
    App(AppControl),
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
}

impl AppControl {
    pub const ALL: [AppControl; 15] = [
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
            AppControl::Crossfader => "Crossfader (A ↔ B)",
            AppControl::SurfacePage => "Control Surface page",
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
        }
    }

    /// On/off controls: pads and keys flip them on each press.
    pub fn is_toggle(self) -> bool {
        matches!(self, AppControl::FollowAudio | AppControl::LaunchFade | AppControl::MasterFxBypass | AppControl::Syphon | AppControl::Ndi)
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
}

impl ControlMap {
    pub fn new(source: ControlSource, target: ControlTarget) -> Self {
        Self { source, target, range: ValueRange::default(), enabled: true }
    }
}
