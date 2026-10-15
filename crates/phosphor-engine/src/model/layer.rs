//! Layers, their effects, and modulation (LFOs, audio, beat…).

use serde::{Deserialize, Serialize};

use super::*;
use crate::shaders;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectState {
    pub id: u64,
    pub effect: String,
    pub enabled: bool,
    pub values: Vec<f32>,
    /// Dry/wet: 0 = input untouched, 1 = fully processed.
    #[serde(default = "full_mix")]
    pub mix: f32,
    #[serde(default)]
    pub mods: Vec<ParamMod>,
}

fn full_mix() -> f32 {
    1.0
}

/// A live signal (0..1) that can drive any parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModSource {
    AudioLevel,
    AudioLow,
    AudioMid,
    AudioHigh,
    LfoSine,
    LfoTriangle,
    LfoSaw,
    LfoSquare,
    /// Sawtooth that restarts on every beat of the BPM clock.
    Beat,
    /// A new random value every cycle (sample and hold).
    Random,
    /// An 8-step sequencer.
    Steps,
    /// A pulse on every beat the audio beat detector hears.
    AudioBeat,
}

impl ModSource {
    pub const ALL: [ModSource; 12] = [
        ModSource::AudioLevel,
        ModSource::AudioLow,
        ModSource::AudioMid,
        ModSource::AudioHigh,
        ModSource::AudioBeat,
        ModSource::LfoSine,
        ModSource::LfoTriangle,
        ModSource::LfoSaw,
        ModSource::LfoSquare,
        ModSource::Beat,
        ModSource::Random,
        ModSource::Steps,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ModSource::AudioLevel => "Audio level",
            ModSource::AudioLow => "Audio low",
            ModSource::AudioMid => "Audio mid",
            ModSource::AudioHigh => "Audio high",
            ModSource::LfoSine => "LFO sine",
            ModSource::LfoTriangle => "LFO triangle",
            ModSource::LfoSaw => "Ramp (saw)",
            ModSource::LfoSquare => "LFO square",
            ModSource::Beat => "Beat (BPM)",
            ModSource::Random => "Random",
            ModSource::Steps => "Step sequencer",
            ModSource::AudioBeat => "Audio beat (kick)",
        }
    }

    pub fn is_lfo(self) -> bool {
        matches!(
            self,
            ModSource::LfoSine | ModSource::LfoTriangle | ModSource::LfoSaw | ModSource::LfoSquare | ModSource::Random | ModSource::Steps
        )
    }

    pub fn is_audio(self) -> bool {
        matches!(self, ModSource::AudioLevel | ModSource::AudioLow | ModSource::AudioMid | ModSource::AudioHigh | ModSource::AudioBeat)
    }
}

/// How a signal moves a parameter: `amount` is a fraction of the parameter's range (−1..1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Modulation {
    pub source: ModSource,
    pub amount: f32,
    /// Cycles per second, or beats per cycle when `sync` is on (ignored for audio sources).
    pub rate: f32,
    /// Lock the cycle to the BPM clock.
    pub sync: bool,
    /// Range mode: the signal sweeps the parameter between `min` and `max` (fractions of its
    /// range) and the slider is ignored. Otherwise the signal is added on top of the slider.
    pub range: bool,
    pub min: f32,
    pub max: f32,
    pub invert: bool,
    pub steps: [f32; 8],
}

impl Default for Modulation {
    fn default() -> Self {
        Self {
            source: ModSource::AudioLevel,
            amount: 0.5,
            rate: 1.0,
            sync: false,
            range: false,
            min: 0.0,
            max: 1.0,
            invert: false,
            steps: [1.0, 0.0, 0.5, 0.0, 1.0, 0.25, 0.75, 0.0],
        }
    }
}

/// A modulation attached to parameter `index` of an effect or generator.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamMod {
    pub index: usize,
    pub m: Modulation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerState {
    pub id: u64,
    pub name: String,
    pub source: SourceSpec,
    pub enabled: bool,
    /// When any layer is soloed, only soloed layers reach the mix.
    pub solo: bool,
    /// Skip this layer's effect chain (quick A/B without touching the chain).
    pub fx_bypass: bool,
    /// Blended into the mix. Off = only mapping surfaces that pick this layer show it.
    pub in_mix: bool,
    pub opacity: f32,
    pub blend: BlendMode,
    pub fit: FitMode,
    pub generator_values: Vec<f32>,
    pub generator_mods: Vec<ParamMod>,
    pub opacity_mod: Option<Modulation>,
    /// Movie sound volume (1 = as recorded).
    pub volume: f32,
    /// Sound fades with the layer's opacity (and its group's).
    pub volume_follows_opacity: bool,
    pub effects: Vec<EffectState>,
    pub playing: bool,
    pub rate: f32,
    pub looping: bool,
    pub direction: PlayDirection,
    /// Crossfader side (Mixer).
    pub group: CrossGroup,
    /// The layer group it belongs to (`Composition::groups`); members sit next to each other in the stack.
    pub parent: Option<u64>,
    /// Loop region as fractions of the movie (0..1), so it survives switching clips and maps like any slider.
    pub loop_in: f32,
    pub loop_out: f32,
    /// Beat loop: when > 0 the loop is this many beats long from `loop_in` (overrides `loop_out`).
    pub loop_beats: f32,
    /// Clip slots, by column.
    pub clips: Vec<Clip>,
    pub transition_secs: f32,
    pub transition_kind: TransitionKind,
    #[serde(skip)]
    pub transition: Option<Transition>,
    #[serde(skip)]
    pub active_clip: Option<u64>,
}

impl Default for LayerState {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Layer".into(),
            source: SourceSpec::None,
            enabled: true,
            solo: false,
            fx_bypass: false,
            in_mix: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            fit: FitMode::Fill,
            generator_values: Vec::new(),
            generator_mods: Vec::new(),
            opacity_mod: None,
            volume: 1.0,
            volume_follows_opacity: true,
            effects: Vec::new(),
            playing: true,
            rate: 1.0,
            looping: true,
            direction: PlayDirection::Forward,
            group: CrossGroup::None,
            parent: None,
            loop_in: 0.0,
            loop_out: 1.0,
            loop_beats: 0.0,
            clips: Vec::new(),
            transition_secs: 0.5,
            transition_kind: TransitionKind::Crossfade,
            transition: None,
            active_clip: None,
        }
    }
}

impl LayerState {
    /// The loop region in seconds for a movie of `duration` at `bpm`, or None for the whole movie.
    pub fn loop_range(&self, duration: f64, bpm: f32) -> Option<(f64, f64)> {
        let lo = (self.loop_in.clamp(0.0, 1.0) as f64) * duration;
        let hi = if self.loop_beats > 0.0 {
            lo + self.loop_beats as f64 * 60.0 / bpm.max(1.0) as f64
        } else {
            self.loop_out.clamp(0.0, 1.0) as f64 * duration
        }
        .min(duration);
        if lo <= 1e-6 && hi >= duration - 1e-6 {
            return None;
        }
        // At least a frame or two long.
        Some((lo.min(duration - 0.05).max(0.0), hi.max(lo + 0.05)))
    }

    /// Starts a `secs`-long transition from the current source (0 = cut).
    pub fn start_transition(&mut self, now: f32, secs: f32) {
        if secs > 0.01 && self.source != SourceSpec::None {
            self.transition = Some(Transition {
                from: self.source.clone(),
                from_values: self.generator_values.clone(),
                start: now,
                duration: secs,
                kind: self.transition_kind,
            });
        } else {
            self.transition = None;
        }
    }

    /// Puts clip `slot` on this layer with a `secs` transition. Returns false for an empty slot.
    pub fn trigger_clip(&mut self, slot: usize, now: f32, secs: f32) -> bool {
        let Some(clip) = self.clips.get(slot).filter(|c| !c.is_empty()).cloned() else { return false };
        if self.active_clip == Some(clip.id) && self.source == clip.source {
            return true;
        }
        self.start_transition(now, secs);
        self.source = clip.source;
        self.generator_values = clip.generator_values;
        self.generator_mods.clear();
        if !clip.name.is_empty() {
            self.name = clip.name;
        }
        self.active_clip = Some(clip.id);
        self.playing = true;
        true
    }

    pub fn clip_slot_mut(&mut self, slot: usize) -> &mut Clip {
        if self.clips.len() <= slot {
            self.clips.resize_with(slot + 1, Clip::default);
        }
        &mut self.clips[slot]
    }

    pub fn expire_transition(&mut self, now: f32) {
        if self.transition.as_ref().is_some_and(|t| t.progress(now) >= 1.0) {
            self.transition = None;
        }
    }

    pub fn set_generator(&mut self, id: &str) {
        if let Some(desc) = shaders::generator(id) {
            self.source = SourceSpec::Generator(id.to_string());
            self.generator_values = desc.defaults();
            self.generator_mods.clear();
        }
    }
}
