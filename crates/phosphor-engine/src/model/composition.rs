//! The whole project (`Composition`) and the media bin.

use serde::{Deserialize, Serialize};

use super::*;
use crate::mapping::Mapping;
use crate::shaders;

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
    /// Layer groups (members point at them with `LayerState::parent`).
    pub groups: Vec<LayerGroup>,
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
    /// Your own buttons, sliders and pads.
    pub surface: ControlSurface,
    /// Audience play (QR code, timed turns).
    #[serde(default)]
    pub play: AudiencePlay,
    /// The show runs itself: columns on the bar, Surprise on drops.
    #[serde(default)]
    pub auto_show: AutoShow,
    /// Names of your media collections.
    pub media_collections: Vec<String>,
    /// Mixer crossfader: 0 = only A layers, 0.5 = both, 1 = only B (layers without a side ignore it).
    pub crossfader: f32,
    pub bpm: f32,
    /// Send outputs to their saved screens when this project opens (venue setups).
    pub outputs_on_open: bool,
    /// Master volume for movie sound.
    pub volume: f32,
    /// LED / DMX pixel mapping (Art-Net, sACN).
    pub dmx: DmxSettings,
    /// Show control: cues to step through or fire on timecode.
    pub cues: CueList,
}

impl Default for Composition {
    fn default() -> Self {
        Self { version: 1, width: 1920, height: 1080, master_fade: 1.0, layers: Vec::new(), groups: Vec::new(), next_id: 1, mapping: Mapping::default(), outputs: Vec::new(), master_effects: Vec::new(), control_maps: Vec::new(), master_fx_bypass: false, media_bin: Vec::new(), clip_columns: 8, bpm: 120.0, master_fade_mod: None, launch: LaunchSettings::default(), syphon: true, ndi: NdiSettings::default(), surface: ControlSurface::default(), play: AudiencePlay::default(), auto_show: AutoShow::default(), media_collections: Vec::new(), crossfader: 0.5, outputs_on_open: false, volume: 1.0, dmx: DmxSettings::default(), cues: CueList::default() }
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

    /// Every source in the project: each layer's current source and its clip slots.
    pub fn sources(&self) -> impl Iterator<Item = &SourceSpec> {
        self.layers.iter().flat_map(|l| std::iter::once(&l.source).chain(l.clips.iter().map(|c| &c.source)))
    }

    /// Every media file the project uses (sources, clips, media bin), sorted, no repeats.
    pub fn media_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .sources()
            .filter_map(|s| match s {
                SourceSpec::Image { path: Some(p), .. } | SourceSpec::Movie { path: Some(p), .. } => Some(p.clone()),
                _ => None,
            })
            .chain(self.media_bin.iter().map(|m| m.path.clone()))
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// Effect and generator ids in use (for packing shaders with a show).
    pub fn used_shader_ids(&self) -> Vec<String> {
        let effects = self.all_effects().map(|e| e.effect.clone());
        let generators = self.sources().filter_map(|s| if let SourceSpec::Generator(g) = s { Some(g.clone()) } else { None });
        let mut ids: Vec<String> = effects.chain(generators).collect();
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn add_layer(&mut self) -> &mut LayerState {
        let id = self.new_id();
        let name = format!("Layer {}", self.layers.len() + 1);
        let transition_kind = self.launch.style;
        self.layers.push(LayerState { id, name, transition_kind, ..Default::default() });
        self.layers.last_mut().unwrap()
    }

    /// How loud a layer's movie sound plays: its volume × the master volume; silent while the layer or its
    /// group is off or another layer is soloed; follows opacity (layer, group, crossfader) unless turned off.
    pub fn sound_gain(&self, layer: &LayerState) -> f32 {
        let group = layer.parent.and_then(|g| self.group(g));
        let soloed_out = !layer.solo && self.layers.iter().any(|l| l.enabled && l.solo);
        if !layer.enabled || soloed_out || group.is_some_and(|g| !g.enabled) {
            return 0.0;
        }
        let mut g = layer.volume * self.volume;
        if layer.volume_follows_opacity {
            g *= layer.opacity * layer.group.gain(self.crossfader) * group.map_or(1.0, |g| g.opacity);
        }
        g.max(0.0)
    }

    /// Layers and groups whose finished picture another layer uses as its source.
    pub fn feed_sources(&self) -> impl Iterator<Item = u64> + '_ {
        self.layers.iter().filter_map(|l| match &l.source {
            SourceSpec::Feed { id, .. } if *id != crate::MASTER_ID => Some(*id),
            _ => None,
        })
    }

    /// One fade style for every layer (and new ones).
    pub fn set_launch_style(&mut self, style: TransitionKind) {
        self.launch.style = style;
        for l in &mut self.layers {
            l.transition_kind = style;
        }
    }

    /// The effect every layer's "Through an effect" fade uses.
    pub fn set_transition_effect(&mut self, effect: &str) {
        self.launch.transition_effect = effect.to_string();
        for l in &mut self.layers {
            l.transition_effect = effect.to_string();
        }
    }

    pub fn layer_mut(&mut self, id: u64) -> Option<&mut LayerState> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// Effects from elsewhere (clipboard, a saved chain) with fresh ids for this project.
    pub fn adopt_effects(&mut self, mut effects: Vec<EffectState>) -> Vec<EffectState> {
        for fx in &mut effects {
            fx.id = self.new_id();
        }
        effects
    }

    /// A layer from elsewhere (a copy, a saved layer) with fresh ids for it, its effects and clips.
    pub fn adopt_layer(&mut self, mut layer: LayerState) -> LayerState {
        layer.id = self.new_id();
        layer.effects = self.adopt_effects(std::mem::take(&mut layer.effects));
        for c in &mut layer.clips {
            c.id = self.new_id();
            if let Some(own) = &mut c.own {
                own.effects = self.adopt_effects(std::mem::take(&mut own.effects));
            }
        }
        layer.active_clip = None;
        layer.transition = None;
        layer
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

    /// A venue: every output with its mapping (surfaces, mesh, masks), size and colour, as JSON to save by name.
    pub fn venue_json(&self) -> String {
        serde_json::to_string_pretty(&self.outputs).unwrap_or_default()
    }

    /// Recalls a saved venue: replaces the outputs. Outputs keep this project's ids in order (open projector
    /// windows stay where they are); extra outputs, surfaces and masks get fresh ids so nothing clashes.
    pub fn apply_venue(&mut self, json: &str) -> Result<(), String> {
        let mut outputs: Vec<OutputConfig> = serde_json::from_str(json).map_err(|e| format!("not a venue ({e})"))?;
        if outputs.is_empty() {
            return Err("that venue has no outputs".into());
        }
        let keep: Vec<u64> = self.outputs.iter().map(|o| o.id).collect();
        for (i, o) in outputs.iter_mut().enumerate() {
            o.id = match keep.get(i) {
                Some(id) => *id,
                None => self.new_id(),
            };
            for s in &mut o.mapping.surfaces {
                s.id = self.new_id();
            }
            for m in &mut o.mapping.masks {
                m.id = self.new_id();
            }
        }
        self.outputs = outputs;
        Ok(())
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

    /// A clip button went down (its trigger mode decides: play, toggle off, flash…).
    pub fn press_clip(&mut self, layer: u64, slot: usize, now: f32) {
        let secs = self.launch_secs();
        if let Some(l) = self.layer_mut(layer) {
            l.press_clip(slot, now, secs);
        }
    }

    pub fn release_clip(&mut self, layer: u64, slot: usize, now: f32) {
        let secs = self.launch_secs();
        if let Some(l) = self.layer_mut(layer) {
            l.release_clip(slot, now, secs);
        }
    }

    /// Puts sources into a layer's clip slots from `start` on (replacing what's there), adding columns when the row
    /// runs out. Returns how many clips were placed.
    pub fn place_clips(&mut self, layer: u64, start: usize, items: Vec<(String, SourceSpec)>) -> usize {
        let items: Vec<_> = items.into_iter().filter(|(_, s)| *s != SourceSpec::None).collect();
        let ids: Vec<u64> = items.iter().map(|_| self.new_id()).collect();
        let Some(l) = self.layer_mut(layer) else { return 0 };
        let n = items.len();
        for (i, ((name, source), id)) in items.into_iter().zip(ids).enumerate() {
            *l.clip_slot_mut(start + i) = Clip { id, name, source, ..Default::default() };
        }
        self.clip_columns = self.clip_columns.max(start + n);
        n
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
            l.keep_clip_settings();
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
        let mut comp: Self = serde_json::from_str(json)?;
        comp.normalize_groups();
        Ok(comp)
    }
}

#[cfg(test)]
mod venue_tests {
    use super::*;

    #[test]
    fn a_venue_comes_back_with_fresh_ids() {
        let mut a = Composition::default();
        a.ensure_outputs();
        let sid = a.new_id();
        a.outputs[0].mapping.add_surface(sid);
        let mid = a.new_id();
        a.outputs[0].mapping.add_mask(mid).feather = 0.05;
        let json = a.venue_json();
        let mut b = Composition::default();
        for _ in 0..10 {
            b.new_id();
        }
        b.apply_venue(&json).unwrap();
        assert_eq!(b.outputs.len(), 1);
        assert_eq!(b.outputs[0].mapping.masks[0].feather, 0.05);
        let ids = [b.outputs[0].id, b.outputs[0].mapping.surfaces[0].id, b.outputs[0].mapping.masks[0].id];
        assert!(ids.iter().all(|i| *i >= 10), "renumbered past b's ids: {ids:?}");
        // An existing output keeps its id (its projector window stays open).
        let mut c = Composition::default();
        c.ensure_outputs();
        let before = c.outputs[0].id;
        c.apply_venue(&json).unwrap();
        assert_eq!(c.outputs[0].id, before);
        assert!(b.apply_venue("{}").is_err());
    }
}

#[cfg(test)]
mod adopt_tests {
    use super::*;

    #[test]
    fn adopted_layers_get_fresh_ids_everywhere() {
        let mut comp = Composition::starter();
        let mut layer = comp.layers[0].clone();
        layer.effects = vec![comp.new_effect("invert").unwrap(), comp.new_effect("blur").unwrap()];
        layer.clips = vec![Clip { id: 7, ..Default::default() }];
        let before: Vec<u64> = comp.layers.iter().map(|l| l.id).collect();
        let copy = comp.adopt_layer(layer.clone());
        assert!(!before.contains(&copy.id));
        assert!(copy.effects.iter().zip(&layer.effects).all(|(a, b)| a.id != b.id));
        assert_ne!(copy.clips[0].id, 7);
        assert_eq!(copy.effects.len(), 2);
    }

    #[test]
    fn place_clips_fills_a_row_and_grows_the_grid() {
        let mut comp = Composition::default();
        let id = comp.add_layer().id;
        let items: Vec<_> = (0..10).map(|i| (format!("m{i}"), SourceSpec::Movie { name: format!("m{i}.mov"), path: Some(format!("/m{i}.mov")) })).collect();
        assert_eq!(comp.place_clips(id, 3, items), 10);
        let l = &comp.layers[0];
        assert!(l.clips[..3].iter().all(|c| c.is_empty()));
        assert_eq!(l.clips[12].name, "m9");
        assert_eq!(comp.clip_columns, 13);
        assert_ne!(l.clips[3].id, l.clips[4].id);
    }
}
