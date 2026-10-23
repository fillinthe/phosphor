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
    /// An Arduino board's pin (Firmata): an analog input (A0…, a knob or sensor, 0..1) or a digital pin (a button).
    Arduino { pin: u8, analog: bool },
    /// An Elgato Stream Deck key (from 0, left to right, top to bottom; the Stream Deck +'s dial presses from 100).
    StreamDeck { key: u8 },
    /// A Stream Deck + dial turned (its own value, 0..1).
    StreamDeckDial { dial: u8 },
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
            ControlSource::Arduino { pin, analog: true } => format!("Arduino A{pin}"),
            ControlSource::Arduino { pin, analog: false } => format!("Arduino pin {pin}"),
            ControlSource::StreamDeck { key } if *key >= 100 => format!("Stream Deck dial {} press", key - 99),
            ControlSource::StreamDeck { key } => format!("Stream Deck key {}", key + 1),
            ControlSource::StreamDeckDial { dial } => format!("Stream Deck dial {}", dial + 1),
        }
    }

    /// Press / release (pads, keys, gamepad buttons) rather than a value that moves (knobs, sticks, triggers).
    pub fn is_momentary(&self) -> bool {
        match self {
            ControlSource::MidiNote { .. } | ControlSource::Key { .. } | ControlSource::Arduino { analog: false, .. } | ControlSource::StreamDeck { .. } => true,
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
    /// Jump a movie back or forward by its jump step (a press).
    LayerJump { layer: u64, forward: bool },
    /// A widget on the Control Surface (its main value).
    Surface(u64),
    /// An XY pad's vertical value.
    SurfaceY(u64),
    /// One bar of a multi-slider (counted from 0).
    SurfaceBar { widget: u64, bar: u8 },
    /// A shared modulator's hand-set value (its "Hand / MIDI" source): one knob feeding a chain.
    Modulator(u64),
    /// App-wide controls (tempo, launch settings, audio…).
    App(AppControl),
    /// A movie layer's sound volume (0..1 = silent..2×).
    LayerVolume(u64),
    /// A text layer's words, from an OSC message that carries text (numbers do nothing here).
    LayerText(u64),
    /// Picks one of a layer's clips by value (0..1 across its clips; a knob or fader), and launches it.
    LayerClipPick(u64),
    /// A text layer in parts: the next / previous part (a press).
    LayerTextStep { layer: u64, forward: bool },
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
    /// A light (LED / DMX fixture): `param` indexes `Fixture::PARAMS` (pan, tilt, dimmer, motion size, brightness).
    Fixture { fixture: u64, param: u8 },
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
    /// Every output holds its picture (on) / goes live again (off).
    FreezeOutputs,
    /// Cue list: a random cue (not the one on now).
    CueRandom,
    /// LED / DMX master level.
    DmxMaster,
    /// Every movie's speed (0..1 = a quarter to four times, the middle = as set).
    MasterSpeed,
}

impl AppControl {
    pub const ALL: [AppControl; 33] = [
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
        AppControl::FreezeOutputs,
        AppControl::CueRandom,
        AppControl::DmxMaster,
        AppControl::MasterSpeed,
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
            AppControl::FreezeOutputs => "Freeze outputs (hold the picture)",
            AppControl::CueRandom => "Cue list: a random cue",
            AppControl::DmxMaster => "LED / DMX master",
            AppControl::MasterSpeed => "Master speed (every movie)",
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
            AppControl::FreezeOutputs => "outputs/freeze",
            AppControl::CueRandom => "cue/random",
            AppControl::DmxMaster => "dmx/master",
            AppControl::MasterSpeed => "speed",
        }
    }

    /// On/off controls: pads and keys flip them on each press.
    pub fn is_toggle(self) -> bool {
        matches!(self, AppControl::FollowAudio | AppControl::LaunchFade | AppControl::MasterFxBypass | AppControl::Syphon | AppControl::Ndi | AppControl::Dmx | AppControl::Link | AppControl::MuteSound | AppControl::AutoLevel | AppControl::WebControl | AppControl::ProjectorsShutter | AppControl::FreezeOutputs)
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
    /// Only while its control is on screen: one knob can drive different things on different pages / tabs.
    #[serde(default)]
    pub only_shown: bool,
}

impl ControlMap {
    pub fn new(source: ControlSource, target: ControlTarget) -> Self {
        Self { source, target, range: ValueRange::default(), enabled: true, fx: Vec::new(), only_shown: false }
    }

    /// The controller value (0..1) after the instant effects (curve, steps, formula); smoothing is the hub's.
    pub fn shape(&self, v: f32) -> f32 {
        self.fx.iter().fold(v.clamp(0.0, 1.0), |v, fx| fx.apply(v))
    }

    /// Seconds this mapping glides over (0 = it jumps).
    pub fn smoothing(&self) -> f32 {
        self.fx.iter().map(|f| if let ValueFx::Smooth(s) = f { *s } else { 0.0 }).fold(0.0, f32::max)
    }

    /// Seconds a drop takes (rises land at once): peak hold / fall (0 = drops land at once too).
    pub fn fall(&self) -> f32 {
        self.fx.iter().map(|f| if let ValueFx::Fall(s) = f { *s } else { 0.0 }).fold(0.0, f32::max)
    }

    /// Endless knob: the value per click (None = the controller sends positions).
    pub fn endless(&self) -> Option<f32> {
        self.fx.iter().find_map(|f| if let ValueFx::Endless(s) = f { Some(*s) } else { None })
    }

    /// Changes smaller than this are ignored (0 = none).
    pub fn dead_band(&self) -> f32 {
        self.fx.iter().map(|f| if let ValueFx::DeadBand(d) = f { *d } else { 0.0 }).fold(0.0, f32::max)
    }

    /// Seconds each move waits before it lands (0 = straight away).
    pub fn delay(&self) -> f32 {
        self.fx.iter().map(|f| if let ValueFx::Delay(s) = f { *s } else { 0.0 }).sum()
    }
}

/// Where a control's value goes when it changes (besides back to the controller that moves it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SendTo {
    /// An OSC float (0..1) to the Control Surface's OSC-out host and port.
    Osc { address: String },
    /// A MIDI CC (0–127) on every MIDI output.
    MidiCc { channel: u8, cc: u8 },
    /// A MIDI note: on (127) above half, off below.
    MidiNote { channel: u8, note: u8 },
    /// An OSC float to another app (found with OSC Query): 0..1 scaled to its `min`..`max`.
    OscTo { host: String, port: u16, address: String, min: f32, max: f32 },
    /// An Arduino board's PWM pin (an LED's brightness, a servo…): 0..1 → 0..255.
    Arduino { pin: u8 },
}

impl SendTo {
    pub fn label(&self) -> String {
        match self {
            SendTo::OscTo { host, port, address, .. } => format!("OSC {host}:{port}{address}"),
            SendTo::Osc { address } => format!("OSC {address}"),
            SendTo::MidiCc { channel, cc } => format!("MIDI ch {} CC {cc}", channel + 1),
            SendTo::MidiNote { channel, note } => format!("MIDI ch {} note {note}", channel + 1),
            SendTo::Arduino { pin } => format!("Arduino pin {pin}"),
        }
    }
}

/// A control that sends its value out (VDMX's senders).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlSend {
    pub target: ControlTarget,
    pub to: SendTo,
}

impl crate::Composition {
    /// A layer's clip row on MIDI notes: slot 1 = `first`, slot 2 = `first + 1`… on `channel` (VDMX's detect range),
    /// replacing the row's note mappings.
    pub fn map_clip_row(&mut self, layer: u64, channel: u8, first: u8) {
        let row = |t: &ControlTarget| matches!(t, ControlTarget::TriggerClip { layer: l, .. } if *l == layer);
        self.control_maps.retain(|m| !(row(&m.target) && matches!(m.source, ControlSource::MidiNote { .. })));
        for slot in 0..self.clip_columns.max(1) {
            let Some(note) = (first as usize).checked_add(slot).filter(|n| *n <= 127) else { break };
            self.control_maps.push(ControlMap::new(ControlSource::MidiNote { channel: channel & 15, note: note as u8 }, ControlTarget::TriggerClip { layer, slot }));
        }
    }

    /// Moves a clip row's note mappings by `by` notes (a bank up / down on a grid controller).
    pub fn shift_clip_row(&mut self, layer: u64, by: i32) {
        for m in &mut self.control_maps {
            if let (ControlTarget::TriggerClip { layer: l, .. }, ControlSource::MidiNote { note, .. }) = (&m.target, &mut m.source)
                && *l == layer
            {
                *note = (*note as i32 + by).clamp(0, 127) as u8;
            }
        }
    }

    /// Copies of `maps` (input, range, value effects) aimed at `target` too; one already the same isn't added twice.
    pub fn paste_mappings(&mut self, maps: &[ControlMap], target: &ControlTarget) {
        for m in maps {
            let copy = ControlMap { target: target.clone(), ..m.clone() };
            if !self.control_maps.contains(&copy) {
                self.control_maps.push(copy);
            }
        }
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
    /// On (1) at or above this, off (0) below: a fader as a switch.
    Threshold(f32),
    /// Kept between these two (the rest of the fader does nothing).
    Clamp(f32, f32),
    /// The range repeats this many times along the fader (0→1, 0→1…).
    Repeat(u32),
    /// Up to this much random jitter on each value.
    Randomize(f32),
    /// Rises land at once; drops take this many seconds (peak hold, like a VU meter).
    Fall(f32),
    /// A drawn curve: the output at evenly spaced points from 0 to 1, straight lines between.
    Lookup(Vec<f32>),
    /// Endless knob (relative MIDI): each message turns the value up or down by its count × this (0..1 per click);
    /// 1–63 = up, 65–127 = down (two's complement, the common encoder setting).
    Endless(f32),
    /// Ignore changes smaller than this (a jittery fader or sensor).
    DeadBand(f32),
}

impl ValueFx {
    pub fn name(&self) -> &'static str {
        match self {
            ValueFx::Smooth(_) => "Smooth",
            ValueFx::Steps(_) => "Steps",
            ValueFx::Curve(_) => "Curve",
            ValueFx::Formula(_) => "Formula",
            ValueFx::Delay(_) => "Delay",
            ValueFx::Threshold(_) => "Threshold",
            ValueFx::Clamp(..) => "Limit",
            ValueFx::Repeat(_) => "Repeat",
            ValueFx::Randomize(_) => "Jitter",
            ValueFx::Fall(_) => "Fall",
            ValueFx::Lookup(_) => "Drawn curve",
            ValueFx::Endless(_) => "Endless knob",
            ValueFx::DeadBand(_) => "Dead band",
        }
    }

    /// The instant ones; Smooth, Delay and Fall pass through here (they work over time).
    pub fn apply(&self, v: f32) -> f32 {
        match self {
            ValueFx::Smooth(_) | ValueFx::Delay(_) | ValueFx::Fall(_) | ValueFx::Endless(_) | ValueFx::DeadBand(_) => v,
            ValueFx::Threshold(t) => if v >= *t { 1.0 } else { 0.0 },
            ValueFx::Clamp(lo, hi) => v.clamp(lo.min(*hi), hi.max(*lo)),
            ValueFx::Repeat(n) => {
                let x = v * (*n).max(1) as f32;
                // The top of each repeat stays 1 (not back to 0), so a full fader still reads full.
                if v >= 1.0 { 1.0 } else { x.fract() }
            }
            ValueFx::Randomize(amount) => (v + (jitter() * 2.0 - 1.0) * amount.clamp(0.0, 1.0)).clamp(0.0, 1.0),
            ValueFx::Lookup(points) => lookup(points, v),
            ValueFx::Steps(n) => {
                let n = (*n).max(2) as f32 - 1.0;
                (v * n).round() / n
            }
            ValueFx::Curve(k) => v.clamp(0.0, 1.0).powf(k.max(0.05)),
            ValueFx::Formula(f) => crate::expr::eval(f, &|name| matches!(name, "$VAL" | "VAL").then_some(v as f64)).map_or(v, |r| (r as f32).clamp(0.0, 1.0)),
        }
    }
}

/// An endless knob's message (0..1 = MIDI 0..127) as clicks: 1–63 up, 65–127 down (two's complement), 0 / 64 none.
pub fn encoder_clicks(v: f32) -> i32 {
    let raw = (v.clamp(0.0, 1.0) * 127.0).round() as i32;
    if raw < 64 { raw } else { raw - 128 }
}

/// A drawn curve's value at `x` (0..1): straight lines between evenly spaced points (none = unchanged).
fn lookup(points: &[f32], x: f32) -> f32 {
    match points.len() {
        0 => x,
        1 => points[0].clamp(0.0, 1.0),
        n => {
            let at = x.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (at as usize).min(n - 2);
            let t = at - i as f32;
            (points[i] + (points[i + 1] - points[i]) * t).clamp(0.0, 1.0)
        }
    }
}

/// 0..1, different every call (Jitter): a shared xorshift, so values need no random crate.
fn jitter() -> f32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0x2545_F491_4F6C_DD1D);
    let mut x = STATE.load(Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    STATE.store(x, Ordering::Relaxed);
    (x >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod value_fx_tests {
    use super::*;

    #[test]
    fn clip_rows_map_to_runs_of_notes() {
        let mut c = crate::Composition { clip_columns: 4, ..Default::default() };
        c.map_clip_row(7, 0, 36);
        c.map_clip_row(7, 0, 40);
        let notes: Vec<u8> = c.control_maps.iter().filter_map(|m| if let ControlSource::MidiNote { note, .. } = m.source { Some(note) } else { None }).collect();
        assert_eq!(notes, vec![40, 41, 42, 43], "replaced, not added");
        assert_eq!(c.control_maps[3].target, ControlTarget::TriggerClip { layer: 7, slot: 3 });
        c.shift_clip_row(7, -8);
        assert!(matches!(c.control_maps[0].source, ControlSource::MidiNote { note: 32, .. }));
    }

    #[test]
    fn mappings_paste_onto_another_control() {
        let mut c = crate::Composition::default();
        let mut m = ControlMap::new(ControlSource::MidiCc { channel: 0, cc: 7 }, ControlTarget::MasterFade);
        m.fx.push(ValueFx::Smooth(0.2));
        c.control_maps.push(m.clone());
        c.paste_mappings(&[m.clone()], &ControlTarget::LayerOpacity(3));
        c.paste_mappings(&[m], &ControlTarget::LayerOpacity(3));
        assert_eq!(c.control_maps.len(), 2, "pasted once");
        assert_eq!(c.control_maps[1].target, ControlTarget::LayerOpacity(3));
        assert_eq!(c.control_maps[1].fx, vec![ValueFx::Smooth(0.2)]);
    }

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

    #[test]
    fn switches_limits_repeats_jitter_and_drawn_curves() {
        let mut m = ControlMap::new(ControlSource::MidiCc { channel: 0, cc: 1 }, ControlTarget::MasterFade);
        m.fx = vec![ValueFx::Threshold(0.6)];
        assert_eq!((m.shape(0.59), m.shape(0.6)), (0.0, 1.0));
        m.fx = vec![ValueFx::Clamp(0.2, 0.7)];
        assert_eq!((m.shape(0.0), m.shape(0.5), m.shape(1.0)), (0.2, 0.5, 0.7));
        m.fx = vec![ValueFx::Repeat(4)];
        assert!((m.shape(0.3) - 0.2).abs() < 1e-6 && m.shape(1.0) == 1.0);
        m.fx = vec![ValueFx::Randomize(0.1)];
        let vals: Vec<f32> = (0..50).map(|_| m.shape(0.5)).collect();
        assert!(vals.iter().all(|v| (0.4..=0.6).contains(v)), "within the amount");
        assert!(vals.iter().any(|v| (v - 0.5).abs() > 0.01), "and it does move");
        // A V shape: 1 at the ends, 0 in the middle.
        m.fx = vec![ValueFx::Lookup(vec![1.0, 0.0, 1.0])];
        assert_eq!((m.shape(0.0), m.shape(0.25), m.shape(0.5), m.shape(1.0)), (1.0, 0.5, 0.0, 1.0));
        m.fx = vec![ValueFx::Fall(0.8)];
        assert_eq!((m.shape(0.3), m.fall()), (0.3, 0.8));
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
