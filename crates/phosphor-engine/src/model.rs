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
}

impl ModSource {
    pub const ALL: [ModSource; 8] = [
        ModSource::AudioLevel,
        ModSource::AudioLow,
        ModSource::AudioMid,
        ModSource::AudioHigh,
        ModSource::LfoSine,
        ModSource::LfoTriangle,
        ModSource::LfoSaw,
        ModSource::LfoSquare,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ModSource::AudioLevel => "Audio level",
            ModSource::AudioLow => "Audio low",
            ModSource::AudioMid => "Audio mid",
            ModSource::AudioHigh => "Audio high",
            ModSource::LfoSine => "LFO sine",
            ModSource::LfoTriangle => "LFO triangle",
            ModSource::LfoSaw => "LFO saw",
            ModSource::LfoSquare => "LFO square",
        }
    }

    pub fn is_lfo(self) -> bool {
        matches!(self, ModSource::LfoSine | ModSource::LfoTriangle | ModSource::LfoSaw | ModSource::LfoSquare)
    }
}

/// How a signal moves a parameter: `amount` is a fraction of the parameter's range (−1..1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Modulation {
    pub source: ModSource,
    pub amount: f32,
    /// LFO speed in Hz (ignored for audio sources).
    pub rate: f32,
}

impl Default for Modulation {
    fn default() -> Self {
        Self { source: ModSource::AudioLevel, amount: 0.5, rate: 1.0 }
    }
}

/// Where an external controller (MIDI, OSC) message comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlSource {
    MidiCc { channel: u8, cc: u8 },
    MidiNote { channel: u8, note: u8 },
    Osc { address: String },
}

impl ControlSource {
    pub fn label(&self) -> String {
        match self {
            ControlSource::MidiCc { channel, cc } => format!("MIDI ch{} CC{cc}", channel + 1),
            ControlSource::MidiNote { channel, note } => format!("MIDI ch{} note {note}", channel + 1),
            ControlSource::Osc { address } => format!("OSC {address}"),
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
}

impl Default for LayerState {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Layer".into(),
            source: SourceSpec::None,
            enabled: true,
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
        }
    }
}

impl LayerState {
    pub fn set_generator(&mut self, id: &str) {
        if let Some(desc) = shaders::generator(id) {
            self.source = SourceSpec::Generator(id.to_string());
            self.generator_values = desc.defaults();
            self.generator_mods.clear();
        }
    }
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
    pub mapping: Mapping,
    /// Effects applied to the whole mix, after every layer has been blended.
    pub master_effects: Vec<EffectState>,
    /// MIDI/OSC bindings.
    pub control_maps: Vec<ControlMap>,
}

impl Default for Composition {
    fn default() -> Self {
        Self { version: 1, width: 1920, height: 1080, master_fade: 1.0, layers: Vec::new(), next_id: 1, mapping: Mapping::default(), master_effects: Vec::new(), control_maps: Vec::new() }
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

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("composition serializes")
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
