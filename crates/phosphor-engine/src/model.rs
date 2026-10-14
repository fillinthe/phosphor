//! The project/document model. Plain serde data so the same JSON round-trips between
//! native, browser, and (later) cloud-synced accounts.

use serde::{Deserialize, Serialize};

use crate::mapping::Mapping;
use crate::shaders;

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
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0) as u32
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

/// What feeds a layer. Media is referenced, never embedded: the app resolves it into
/// GPU textures via `Engine::upload_rgba` / `Engine::upload_nv12`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum SourceSpec {
    #[default]
    None,
    Generator(String),
    Image { name: String, path: Option<String> },
    Movie { name: String, path: Option<String> },
}

impl SourceSpec {
    pub fn label(&self) -> String {
        match self {
            SourceSpec::None => "No source".into(),
            SourceSpec::Generator(id) => shaders::generator(id).map_or(id.clone(), |g| g.name.to_string()),
            SourceSpec::Image { name, .. } | SourceSpec::Movie { name, .. } => name.clone(),
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
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0) as u32
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

/// A clip slot: a source ready to be triggered onto its layer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    pub id: u64,
    pub name: String,
    pub source: SourceSpec,
    pub generator_values: Vec<f32>,
}

impl Clip {
    pub fn is_empty(&self) -> bool {
        self.source == SourceSpec::None
    }
}

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
    /// App-wide controls (tempo, launch settings, audio…).
    App(AppControl),
}

/// NDI network video output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NdiSettings {
    pub enabled: bool,
    /// Shown to receivers as "COMPUTER (name)".
    pub name: String,
    pub fps: u32,
}

impl Default for NdiSettings {
    fn default() -> Self {
        Self { enabled: false, name: "Phosphor".into(), fps: 60 }
    }
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
    MasterFxBypass,
    AudioGain,
    KickThreshold,
    Syphon,
    Ndi,
}

impl AppControl {
    pub const ALL: [AppControl; 12] = [
        AppControl::Tempo,
        AppControl::TapTempo,
        AppControl::Resync,
        AppControl::FollowAudio,
        AppControl::LaunchFade,
        AppControl::FadeTime,
        AppControl::LaunchTiming,
        AppControl::MasterFxBypass,
        AppControl::AudioGain,
        AppControl::KickThreshold,
        AppControl::Syphon,
        AppControl::Ndi,
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
            AppControl::MasterFxBypass => "Master FX bypass",
            AppControl::AudioGain => "Audio gain",
            AppControl::KickThreshold => "Kick threshold",
            AppControl::Syphon => "Syphon on/off",
            AppControl::Ndi => "NDI on/off",
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
            AppControl::MasterFxBypass => "fxbypass",
            AppControl::AudioGain => "audio/gain",
            AppControl::KickThreshold => "audio/kick",
            AppControl::Syphon => "syphon",
            AppControl::Ndi => "ndi",
        }
    }

    /// On/off controls: pads and keys flip them on each press.
    pub fn is_toggle(self) -> bool {
        matches!(self, AppControl::FollowAudio | AppControl::LaunchFade | AppControl::MasterFxBypass | AppControl::Syphon | AppControl::Ndi)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlMap {
    pub source: ControlSource,
    pub target: ControlTarget,
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
    pub opacity: f32,
    pub blend: BlendMode,
    pub fit: FitMode,
    pub generator_values: Vec<f32>,
    pub generator_mods: Vec<ParamMod>,
    pub opacity_mod: Option<Modulation>,
    pub effects: Vec<EffectState>,
    pub playing: bool,
    pub rate: f32,
    pub looping: bool,
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
            opacity: 1.0,
            blend: BlendMode::Normal,
            fit: FitMode::Fill,
            generator_values: Vec::new(),
            generator_mods: Vec::new(),
            opacity_mod: None,
            effects: Vec::new(),
            playing: true,
            rate: 1.0,
            looping: true,
            clips: Vec::new(),
            transition_secs: 0.5,
            transition_kind: TransitionKind::Crossfade,
            transition: None,
            active_clip: None,
        }
    }
}

impl LayerState {
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

/// One physical output (projector, screen, LED wall): its own size and mapping surfaces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    /// 0 × 0 follows the canvas size.
    pub width: u32,
    pub height: u32,
    pub mapping: Mapping,
    /// Also publish this output (after mapping) as its own Syphon source.
    pub syphon: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self { id: 0, name: "Output".into(), enabled: true, width: 0, height: 0, mapping: Mapping::default(), syphon: false }
    }
}

impl OutputConfig {
    pub fn size(&self, canvas: (u32, u32)) -> (u32, u32) {
        if self.width == 0 || self.height == 0 { canvas } else { (self.width, self.height) }
    }
}

/// A file in the media bin. Paths are stored as given; thumbnails are rebuilt on load.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaItem {
    pub name: String,
    pub path: String,
    /// Watched folder this came from (empty for files added one by one).
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub favorite: bool,
    /// Your collections this item belongs to.
    #[serde(default)]
    pub collections: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Composition {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub master_fade: f32,
    /// Index 0 is the bottom of the stack.
    pub layers: Vec<LayerState>,
    pub next_id: u64,
    /// Legacy single-output mapping; moved into `outputs[0]` on load.
    pub mapping: Mapping,
    pub outputs: Vec<OutputConfig>,
    /// Effects applied to the whole mix, after every layer has been blended.
    pub master_effects: Vec<EffectState>,
    /// MIDI/OSC bindings.
    pub control_maps: Vec<ControlMap>,
    pub master_fx_bypass: bool,
    pub media_bin: Vec<MediaItem>,
    /// Columns shown in the Clips grid.
    pub clip_columns: usize,
    pub master_fade_mod: Option<Modulation>,
    pub launch: LaunchSettings,
    /// Publish the composition as a Syphon source named "Phosphor" (macOS).
    pub syphon: bool,
    /// Send the composition over the network as NDI.
    pub ndi: NdiSettings,
    /// Names of your media collections.
    pub media_collections: Vec<String>,
    pub bpm: f32,
}

impl Default for Composition {
    fn default() -> Self {
        Self { version: 1, width: 1920, height: 1080, master_fade: 1.0, layers: Vec::new(), next_id: 1, mapping: Mapping::default(), outputs: Vec::new(), master_effects: Vec::new(), control_maps: Vec::new(), master_fx_bypass: false, media_bin: Vec::new(), clip_columns: 8, bpm: 120.0, master_fade_mod: None, launch: LaunchSettings::default(), syphon: true, ndi: NdiSettings::default(), media_collections: Vec::new() }
    }
}

impl Composition {
    /// A small starter project so a fresh launch shows something moving.
    pub fn starter() -> Self {
        let mut comp = Self::default();
        let base = comp.add_layer();
        base.set_generator("plasma");
        let top = comp.add_layer();
        top.set_generator("rings");
        top.blend = BlendMode::Screen;
        top.opacity = 0.6;
        comp
    }

    pub fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Adds a layer on top of the stack.
    /// Rewrites every media file path (layer sources, clip slots, the media bin), e.g. to make a
    /// project portable or to resolve it against a new folder.
    pub fn map_paths(&mut self, mut f: impl FnMut(&str) -> String) {
        let spec = |s: &mut SourceSpec, f: &mut dyn FnMut(&str) -> String| {
            if let SourceSpec::Image { path: Some(p), .. } | SourceSpec::Movie { path: Some(p), .. } = s {
                *p = f(p);
            }
        };
        for l in &mut self.layers {
            spec(&mut l.source, &mut f);
            for c in &mut l.clips {
                spec(&mut c.source, &mut f);
            }
        }
        for m in &mut self.media_bin {
            m.path = f(&m.path);
        }
    }

    /// Effect and generator ids in use (for packing shaders with a show).
    pub fn used_shader_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.master_effects.iter().map(|e| e.effect.clone()).collect();
        for l in &self.layers {
            ids.extend(l.effects.iter().map(|e| e.effect.clone()));
            for s in std::iter::once(&l.source).chain(l.clips.iter().map(|c| &c.source)) {
                if let SourceSpec::Generator(g) = s {
                    ids.push(g.clone());
                }
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn add_layer(&mut self) -> &mut LayerState {
        let id = self.new_id();
        let name = format!("Layer {}", self.layers.len() + 1);
        self.layers.push(LayerState { id, name, ..Default::default() });
        self.layers.last_mut().unwrap()
    }

    pub fn layer_mut(&mut self, id: u64) -> Option<&mut LayerState> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    pub fn new_effect(&mut self, effect_id: &str) -> Option<EffectState> {
        let desc = shaders::effect(effect_id)?;
        Some(EffectState { id: self.new_id(), effect: effect_id.to_string(), enabled: true, values: desc.defaults(), mix: 1.0, mods: Vec::new() })
    }

    /// Makes sure there is at least one output (older projects kept a single mapping instead).
    pub fn ensure_outputs(&mut self) {
        if self.outputs.is_empty() {
            let id = self.new_id();
            let mapping = std::mem::take(&mut self.mapping);
            self.outputs.push(OutputConfig { id, name: "Output 1".into(), mapping, ..Default::default() });
        }
    }

    pub fn add_output(&mut self) -> u64 {
        let id = self.new_id();
        let name = format!("Output {}", self.outputs.len() + 1);
        self.outputs.push(OutputConfig { id, name, ..Default::default() });
        id
    }

    pub fn output_mut(&mut self, id: u64) -> Option<&mut OutputConfig> {
        self.outputs.iter_mut().find(|o| o.id == id)
    }

    /// Triggers column `col` on every layer that has a clip there.
    pub fn trigger_column(&mut self, col: usize, now: f32) {
        let secs = self.launch_secs();
        for l in &mut self.layers {
            l.trigger_clip(col, now, secs);
        }
    }

    /// Fade time for launches (0 = cut).
    pub fn launch_secs(&self) -> f32 {
        if self.launch.fade { self.launch.fade_secs } else { 0.0 }
    }

    pub fn trigger_clip(&mut self, layer: u64, slot: usize, now: f32) {
        let secs = self.launch_secs();
        if let Some(l) = self.layer_mut(layer) {
            l.trigger_clip(slot, now, secs);
        }
    }

    /// Puts a new source on a layer using the launch fade.
    pub fn set_layer_source(&mut self, layer: u64, source: SourceSpec, values: Vec<f32>, name: String, now: f32) {
        let secs = self.launch_secs();
        if let Some(l) = self.layer_mut(layer) {
            l.start_transition(now, secs);
            l.source = source;
            l.generator_values = values;
            l.generator_mods.clear();
            l.active_clip = None;
            if !name.is_empty() {
                l.name = name;
            }
            l.playing = true;
        }
    }

    pub fn expire_transitions(&mut self, now: f32) {
        for l in &mut self.layers {
            l.expire_transition(now);
        }
    }

    /// Adds a file to the media bin (no duplicates). Returns true if it was new.
    pub fn add_media(&mut self, name: &str, path: &str) -> bool {
        if self.media_bin.iter().any(|m| m.path == path) {
            return false;
        }
        self.media_bin.push(MediaItem { name: name.to_string(), path: path.to_string(), ..Default::default() });
        true
    }

    /// Adds a file from a watched folder (no duplicates). Returns true if it was new.
    pub fn add_folder_media(&mut self, name: &str, path: &str, folder: &str) -> bool {
        if let Some(m) = self.media_bin.iter_mut().find(|m| m.path == path) {
            m.folder = folder.to_string();
            return false;
        }
        self.media_bin.push(MediaItem { name: name.to_string(), path: path.to_string(), folder: folder.to_string(), ..Default::default() });
        true
    }

    /// Makes sure every file used by a layer is in the bin (covers Open…, drops and loaded projects).
    pub fn sync_media_bin(&mut self) {
        let used: Vec<(String, String)> = self
            .layers
            .iter()
            .filter_map(|l| match &l.source {
                SourceSpec::Image { name, path: Some(p) } | SourceSpec::Movie { name, path: Some(p) } => Some((name.clone(), p.clone())),
                _ => None,
            })
            .collect();
        for (name, path) in used {
            self.add_media(&name, &path);
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("composition serializes")
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
