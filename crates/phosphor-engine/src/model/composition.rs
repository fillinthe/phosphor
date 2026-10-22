//! The whole project (`Composition`) and the media bin.

use serde::{Deserialize, Serialize};

use super::*;
use crate::mapping::Mapping;
use crate::shaders;

/// A file in the media bin. Paths are stored as given; thumbnails are rebuilt on load.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
    /// How it last played (speed, direction, loop, effects): it comes back with these when you use it again.
    #[serde(default)]
    pub settings: Option<crate::ClipSettings>,
    /// Last played without sound.
    #[serde(default)]
    pub muted: bool,
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
    /// Comps: canvases of their own, mixed into the master in order. Empty = one ("Main") holding every layer.
    pub comps: Vec<Comp>,
    /// The comp whose layers the Clips / Layers / Mixer panels show (None = the first).
    pub open_comp: Option<u64>,
    pub next_id: u64,
    /// Legacy single-output mapping; moved into `outputs[0]` on load.
    pub mapping: Mapping,
    pub outputs: Vec<OutputConfig>,
    /// Effects applied to the whole mix, after every layer has been blended.
    pub master_effects: Vec<EffectState>,
    /// MIDI/OSC bindings.
    pub control_maps: Vec<ControlMap>,
    /// Controls that send their value out (MIDI / OSC) when it changes.
    pub control_sends: Vec<ControlSend>,
    /// Locked controls and slider snap points.
    pub control_prefs: ControlPrefs,
    /// Video tracking (faces, hands, body) as modulators.
    #[serde(default)]
    pub tracking: Tracking,
    pub master_fx_bypass: bool,
    pub media_bin: Vec<MediaItem>,
    /// Columns shown in the Clips grid.
    pub clip_columns: usize,
    pub master_fade_mod: Option<Modulation>,
    pub launch: LaunchSettings,
    /// Publish the composition as a Syphon source named "Phosphor" (macOS; costs nothing while nobody receives).
    pub syphon: bool,
    /// Publish the composition as the Spout sender "Phosphor" (Windows). Separate from `syphon` and off by
    /// default: Spout can't tell whether anyone receives, so it copies every frame while on.
    pub spout: bool,
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
    /// Shared modulators any control can follow (and chain into each other).
    #[serde(default)]
    pub modulators: Vec<Modulator>,
    /// Names of your media collections.
    pub media_collections: Vec<String>,
    /// Folders whose files belong to a collection (see `model::collections`).
    pub collection_folders: Vec<CollectionFolder>,
    /// Mixer crossfader: 0 = only A layers, 0.5 = both, 1 = only B (layers without a side ignore it).
    pub crossfader: f32,
    /// How the crossfader shares out A and B.
    pub fader_curve: FaderCurve,
    /// Every movie plays this much faster or slower (1 = as set on each layer).
    pub master_speed: f32,
    /// Notes kept with the project (the Notes panel).
    pub notes: String,
    /// Auto-fade: seconds the crossfader takes to glide across.
    pub auto_fade_secs: f32,
    pub bpm: f32,
    /// Send outputs to their saved screens when this project opens (venue setups).
    pub outputs_on_open: bool,
    /// Master volume for movie sound.
    pub volume: f32,
    /// Every sound Phosphor sends out is silent (movies now; streams and recordings start without audio).
    pub sound_muted: bool,
    /// Even out the movies' loudness: each one is brought towards the same level as it plays.
    pub sound_auto_level: bool,
    /// Phones and browsers (remote and audience play) may not change anything: you have the controls.
    pub web_control_off: bool,
    /// LED / DMX pixel mapping (Art-Net, sACN).
    pub dmx: DmxSettings,
    /// Show control: cues to step through or fire on timecode.
    pub cues: CueList,
}

/// The project file format this Phosphor writes. Bump it (with a `migrate` step) when a saved project would
/// otherwise load wrongly; new fields with defaults don't need a bump.
pub const FORMAT: u32 = 1;

impl Default for Composition {
    fn default() -> Self {
        Self { version: 1, width: 1920, height: 1080, master_fade: 1.0, layers: Vec::new(), groups: Vec::new(), comps: Vec::new(), open_comp: None, next_id: 1, mapping: Mapping::default(), outputs: Vec::new(), master_effects: Vec::new(), control_maps: Vec::new(), control_sends: Vec::new(), control_prefs: ControlPrefs::default(), tracking: Tracking::default(), master_fx_bypass: false, media_bin: Vec::new(), clip_columns: 8, bpm: 120.0, master_fade_mod: None, launch: LaunchSettings::default(), syphon: true, spout: false, ndi: NdiSettings::default(), surface: ControlSurface::default(), play: AudiencePlay::default(), auto_show: AutoShow::default(), modulators: Vec::new(), media_collections: Vec::new(), collection_folders: Vec::new(), crossfader: 0.5, fader_curve: FaderCurve::default(), master_speed: 1.0, notes: String::new(), auto_fade_secs: 2.0, outputs_on_open: false, volume: 1.0, sound_muted: false, sound_auto_level: false, web_control_off: false, dmx: DmxSettings::default(), cues: CueList::default() }
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

    /// How many layers and clips use each media file (path → count).
    pub fn media_uses(&self) -> std::collections::HashMap<String, usize> {
        let mut uses = std::collections::HashMap::new();
        for s in self.sources() {
            if let SourceSpec::Image { path: Some(p), .. } | SourceSpec::Movie { path: Some(p), .. } = s {
                *uses.entry(p.clone()).or_insert(0) += 1;
            }
        }
        uses
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

    /// Adds a layer on top of the open comp's stack.
    pub fn add_layer(&mut self) -> &mut LayerState {
        let id = self.new_id();
        let comp = self.open_comp();
        let name = format!("Layer {}", self.layers.iter().filter(|l| self.comp_of(l) == comp).count() + 1);
        let transition_kind = self.launch.style;
        // On top of its comp: after that comp's last layer (comps keep their layers together).
        let at = self.layers.iter().rposition(|l| self.comp_of(l) == comp).map_or(self.layers.len(), |i| i + 1);
        self.layers.insert(at, LayerState { id, name, transition_kind, comp, ..Default::default() });
        &mut self.layers[at]
    }

    /// How loud a layer's movie sound plays: its volume × the master volume; silent while the layer or its
    /// group is off or another layer is soloed; follows opacity (layer, group, crossfader) unless turned off.
    pub fn sound_gain(&self, layer: &LayerState) -> f32 {
        let group = layer.parent.and_then(|g| self.group(g));
        // Solo works inside a comp.
        let soloed_out = !layer.solo && self.layers.iter().any(|l| l.enabled && l.solo && self.comp_of(l) == self.comp_of(layer));
        let clip_muted = layer.active_clip.is_some_and(|id| layer.clips.iter().any(|k| k.id == id && k.muted));
        if self.sound_muted || !layer.enabled || soloed_out || group.is_some_and(|g| !g.enabled) || layer.muted || clip_muted {
            return 0.0;
        }
        let mut g = layer.volume * self.volume;
        if layer.volume_follows_opacity {
            g *= layer.opacity * layer.group.gain(self.crossfader, self.fader_curve) * group.map_or(1.0, |g| g.opacity);
        }
        g.max(0.0)
    }

    /// Layers and groups whose finished picture another layer uses as its source.
    pub fn feed_sources(&self) -> impl Iterator<Item = u64> + '_ {
        self.layers.iter().filter_map(|l| match &l.source {
            SourceSpec::Feed { id, before_fx: false, .. } if *id != crate::MASTER_ID => Some(*id),
            _ => None,
        })
    }

    /// Layers whose picture before their effects another layer shows.
    pub fn pre_fx_feed_sources(&self) -> impl Iterator<Item = u64> + '_ {
        self.layers.iter().filter_map(|l| match &l.source {
            SourceSpec::Feed { id, before_fx: true, .. } => Some(*id),
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

    pub fn layer(&self, id: u64) -> Option<&LayerState> {
        self.layers.iter().find(|l| l.id == id)
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
        Some(EffectState { id: self.new_id(), effect: effect_id.to_string(), enabled: true, values: desc.defaults(), mix: 1.0, mods: Vec::new(), blend: Default::default() })
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

    /// Triggers column `col` on every layer (in every comp) that has a clip there: a mapped column button does the
    /// same whichever comp is on screen.
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

    /// A clip button went down (its trigger mode decides: play, toggle off, flash…). A row with voices plays
    /// normal clips on the layer `voice_for` picks.
    pub fn press_clip(&mut self, layer: u64, slot: usize, now: f32) {
        let secs = self.launch_secs();
        let shared = self.layer(layer).and_then(|l| {
            let c = l.clips.get(slot).filter(|c| !c.is_empty() && c.trigger == TriggerMode::Normal)?;
            (l.voices != VoiceMode::Own).then(|| c.clone())
        });
        if let Some(clip) = shared {
            let to = self.voice_for(layer);
            if let Some(l) = self.layer_mut(to) {
                l.play_clip(clip, now, secs);
            }
        } else if let Some(l) = self.layer_mut(layer) {
            l.press_clip(slot, now, secs);
        }
    }

    /// The layer a clip from `layer`'s row plays on (its voice mode; the pool is the layers in the same group and
    /// comp, top to bottom). Moves the turn on.
    pub fn voice_for(&mut self, layer: u64) -> u64 {
        let Some(row) = self.layer(layer) else { return layer };
        let (mode, parent, comp, turn) = (row.voices, row.parent, row.comp, row.voice_turn);
        let pool: Vec<(u64, bool)> = self
            .layers
            .iter()
            .filter(|l| l.parent == parent && l.comp == comp)
            .map(|l| (l.id, l.source == SourceSpec::None || !l.playing || !l.enabled))
            .collect();
        if mode == VoiceMode::Own || pool.is_empty() {
            return layer;
        }
        let at = turn % pool.len();
        // Free: the first idle layer from this turn on; none free = the next in turn.
        let pick = match mode {
            VoiceMode::NextFree => (0..pool.len()).map(|k| (at + k) % pool.len()).find(|&k| pool[k].1).unwrap_or(at),
            _ => at,
        };
        if let Some(l) = self.layer_mut(layer) {
            l.voice_turn = pick + 1;
        }
        pool[pick].0
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
        // A file you've used before comes with the settings it last played with.
        let memory: Vec<_> = items.iter().map(|(_, s)| self.media_memory(s)).collect();
        let Some(l) = self.layer_mut(layer) else { return 0 };
        let n = items.len();
        for (i, (((name, source), id), mem)) in items.into_iter().zip(ids).zip(memory).enumerate() {
            let (own, muted) = mem.map_or((None, false), |(s, m)| (Some(s), m));
            *l.clip_slot_mut(start + i) = Clip { id, name, source, own, muted, ..Default::default() };
        }
        self.clip_columns = self.clip_columns.max(start + n);
        n
    }

    /// Puts a new source on a layer using the launch fade.
    pub fn set_layer_source(&mut self, layer: u64, source: SourceSpec, values: Vec<f32>, name: String, now: f32) {
        let secs = self.launch_secs();
        let memory = self.media_memory(&source);
        if let Some(l) = self.layer_mut(layer) {
            if let Some((settings, _)) = &memory {
                l.apply_clip_settings(settings);
            }
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

    /// Each movie playing on a layer remembers its settings in the media bin (call a few times a second).
    pub fn remember_media_settings(&mut self) {
        for l in &self.layers {
            let SourceSpec::Movie { path: Some(path), .. } = &l.source else { continue };
            let Some(item) = self.media_bin.iter_mut().find(|m| m.path == *path) else { continue };
            let settings = l.clip_settings();
            let muted = l.muted || l.active_clip.is_some_and(|id| l.clips.iter().any(|k| k.id == id && k.muted));
            if item.settings.as_ref() != Some(&settings) || item.muted != muted {
                item.settings = Some(settings);
                item.muted = muted;
            }
        }
    }

    /// What a media file remembers (settings, muted), for a clip or layer that's about to use it.
    pub fn media_memory(&self, source: &SourceSpec) -> Option<(crate::ClipSettings, bool)> {
        let SourceSpec::Movie { path: Some(path), .. } = source else { return None };
        let item = self.media_bin.iter().find(|m| m.path == *path)?;
        item.settings.clone().map(|s| (s, item.muted))
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

    /// `add_media`, also putting the file in `collection` (the one the Media panel shows) when there is one, even if
    /// the file was already in the bin.
    pub fn add_media_to(&mut self, name: &str, path: &str, collection: Option<&str>) {
        self.add_media(name, path);
        if let (Some(c), Some(i)) = (collection, self.media_bin.iter().position(|m| m.path == path)) {
            self.add_to_collection(i, c);
        }
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

    /// "Share the picture with other apps" on this platform: Syphon on the Mac, Spout on Windows.
    pub fn share_out(&self) -> bool {
        if cfg!(windows) { self.spout } else { self.syphon }
    }

    pub fn share_out_mut(&mut self) -> &mut bool {
        if cfg!(windows) { &mut self.spout } else { &mut self.syphon }
    }

    /// Saves in the current project format (`FORMAT`).
    pub fn to_json(&self) -> String {
        let mut c = std::borrow::Cow::Borrowed(self);
        if c.version != FORMAT {
            c.to_mut().version = FORMAT;
        }
        serde_json::to_string_pretty(&*c).expect("composition serializes")
    }

    /// Reads a project; older formats are brought up to date (`migrate`). A newer format still opens (unknown
    /// settings are dropped): check `made_by_newer` to warn.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let mut comp: Self = serde_json::from_str(json)?;
        comp.migrate();
        comp.normalize_groups();
        Ok(comp)
    }

    /// Made by a newer Phosphor than this one (some of its settings may not survive a save here).
    pub fn made_by_newer(&self) -> bool {
        self.version > FORMAT
    }

    /// One step per format change, oldest first. Leaves newer files as they are.
    fn migrate(&mut self) {
        // Format 1 (0.1 to 0.2.53) needs nothing yet: every field added since has a default. Add steps like
        //   if self.version < 2 { …; self.version = 2; }
        // when a change can't be expressed as a new field with a default.
        if self.version < 1 {
            self.version = 1;
        }
    }
}

#[cfg(test)]
mod voice_tests {
    use super::*;

    #[test]
    fn clips_share_out_over_the_layers() {
        let mut c = Composition::default();
        for id in 1..=3 {
            c.layers.push(LayerState { id, ..Default::default() });
        }
        c.layers[0].voices = VoiceMode::NextLayer;
        c.layers[0].clips = vec![Clip { id: 50, name: "Kick".into(), source: SourceSpec::Generator("plasma".into()), ..Default::default() }];
        let played: Vec<u64> = (0..4)
            .map(|_| {
                c.press_clip(1, 0, 0.0);
                c.layers.iter().filter(|l| l.active_clip == Some(50)).map(|l| l.id).last().unwrap()
            })
            .collect();
        assert_eq!(&played[..3], &[1, 2, 3], "each on the next layer");
        // Free: layer 1 stops, so it's the one picked.
        for l in &mut c.layers {
            l.active_clip = None;
        }
        c.layers[0].voices = VoiceMode::NextFree;
        c.layers[0].playing = false;
        c.layers[0].voice_turn = 1;
        c.press_clip(1, 0, 0.0);
        assert_eq!(c.layers[0].active_clip, Some(50), "the free layer");
    }
}

#[cfg(test)]
mod media_memory_tests {
    use super::*;

    #[test]
    fn a_movie_comes_back_with_its_settings() {
        let mut c = Composition::default();
        let movie = SourceSpec::Movie { name: "a.mp4".into(), path: Some("/x/a.mp4".into()) };
        c.add_media("a.mp4", "/x/a.mp4");
        let id = c.add_layer().id;
        c.set_layer_source(id, movie.clone(), Vec::new(), String::new(), 0.0);
        c.layers[0].rate = 0.5;
        c.layers[0].direction = crate::PlayDirection::PingPong;
        c.layers[0].muted = true;
        c.remember_media_settings();
        // Into a clip slot elsewhere: the clip keeps those settings.
        let other = c.add_layer().id;
        c.place_clips(other, 0, vec![("a.mp4".into(), movie.clone())]);
        let clip = &c.layers.iter().find(|l| l.id == other).unwrap().clips[0];
        assert_eq!(clip.own.as_ref().map(|s| (s.rate, s.direction)), Some((0.5, crate::PlayDirection::PingPong)));
        assert!(clip.muted);
        // Straight onto a fresh layer: the layer takes them.
        let third = c.add_layer().id;
        c.set_layer_source(third, movie, Vec::new(), String::new(), 0.0);
        assert_eq!(c.layers.iter().find(|l| l.id == third).unwrap().rate, 0.5);
    }

    #[test]
    fn media_uses_count_layers_and_clips() {
        let mut c = Composition::default();
        let movie = SourceSpec::Movie { name: "a.mp4".into(), path: Some("/x/a.mp4".into()) };
        let id = c.add_layer().id;
        c.set_layer_source(id, movie.clone(), Vec::new(), String::new(), 0.0);
        c.place_clips(id, 0, vec![("a".into(), movie.clone()), ("a again".into(), movie)]);
        let uses = c.media_uses();
        assert_eq!(uses.get("/x/a.mp4"), Some(&3));
        assert_eq!(uses.get("/x/b.mp4"), None);
    }

    #[test]
    fn jump_steps_in_beats_or_seconds() {
        let mut l = LayerState::default();
        l.jump = 2.0;
        // 2 beats at 120 BPM, normal speed = 1 s of movie; at half speed, half that.
        assert!((l.jump_secs(Some(10.0), 120.0) - 1.0).abs() < 1e-9);
        l.rate = 0.5;
        assert!((l.jump_secs(Some(10.0), 120.0) - 0.5).abs() < 1e-9);
        l.jump_in_seconds = true;
        assert_eq!(l.jump_secs(Some(10.0), 120.0), 2.0);
    }
}

#[cfg(test)]
mod mute_tests {
    use super::*;

    #[test]
    fn muting_a_layer_or_its_playing_clip_silences_it() {
        let mut c = Composition::default();
        let layer = c.add_layer();
        layer.clips = vec![Clip { id: 9, ..Default::default() }];
        layer.active_clip = Some(9);
        let l = c.layers[0].clone();
        assert!(c.sound_gain(&l) > 0.0);
        c.layers[0].muted = true;
        let l = c.layers[0].clone();
        assert_eq!(c.sound_gain(&l), 0.0);
        c.layers[0].muted = false;
        c.layers[0].clips[0].muted = true;
        let l = c.layers[0].clone();
        assert_eq!(c.sound_gain(&l), 0.0, "the playing clip is muted");
        c.layers[0].active_clip = None;
        let l = c.layers[0].clone();
        assert!(c.sound_gain(&l) > 0.0, "another clip plays with sound");
        c.sound_muted = true;
        assert_eq!(c.sound_gain(&l), 0.0, "mute all silences every layer");
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn projects_carry_their_format() {
        let json = Composition::default().to_json();
        assert!(json.contains(&format!("\"version\": {FORMAT}")));
        let old = json.replace(&format!("\"version\": {FORMAT}"), "\"version\": 0");
        assert_eq!(Composition::from_json(&old).unwrap().version, FORMAT, "old files are brought up to date");
        let newer = json.replace(&format!("\"version\": {FORMAT}"), &format!("\"version\": {}", FORMAT + 1));
        let c = Composition::from_json(&newer).unwrap();
        assert!(c.made_by_newer());
        // Saved here, it's written as this format.
        assert!(c.to_json().contains(&format!("\"version\": {FORMAT}")));
    }

    #[test]
    fn share_out_is_off_on_windows_by_default() {
        let c = Composition::default();
        assert_eq!(c.share_out(), !cfg!(windows));
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
