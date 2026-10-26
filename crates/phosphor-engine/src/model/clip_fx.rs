//! Effects that follow a clip or a media file. The playing clip's (or file's) chain sits on its layer as
//! `LayerState::clip_effects`, runs before the layer's own effects, and changes made to it go back into the clip
//! or the file. The layer's own chain never changes when clips change.

use serde::{Deserialize, Serialize};

use super::*;

/// Whose effects a layer's clip chain holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectsOwner {
    #[default]
    None,
    Clip(u64),
    /// A media file (its path in the media bin).
    File(String),
}

impl EffectsOwner {
    pub fn is_none(&self) -> bool {
        *self == EffectsOwner::None
    }
}

/// This bit marks a layer's clip chain in `Composition::effects` (`clip_chain(layer)`); ids never get this high.
const CLIP_CHAIN: u64 = 1 << 62;

/// The id that names a layer's clip chain (effect browser, paste, copy: anything that takes a chain id).
pub fn clip_chain(layer: u64) -> u64 {
    layer | CLIP_CHAIN
}

/// The layer whose clip chain `id` names, if it names one.
pub fn chain_layer(id: u64) -> Option<u64> {
    (id != crate::MASTER_ID && id & CLIP_CHAIN != 0).then_some(id & !CLIP_CHAIN)
}

/// The media file a source plays, if it's one.
pub fn file_of(source: &SourceSpec) -> Option<&str> {
    match source {
        SourceSpec::Movie { path: Some(p), .. } | SourceSpec::Image { path: Some(p), .. } => Some(p),
        _ => None,
    }
}

/// Whose effects belong on a layer now (borrowed, so the per-frame check doesn't allocate).
#[derive(Clone, Copy, PartialEq)]
enum Now<'a> {
    None,
    Clip(u64),
    File(&'a str),
}

impl Now<'_> {
    fn is(self, owner: &EffectsOwner) -> bool {
        match (self, owner) {
            (Now::None, EffectsOwner::None) => true,
            (Now::Clip(a), EffectsOwner::Clip(b)) => a == *b,
            (Now::File(a), EffectsOwner::File(b)) => a == b,
            _ => false,
        }
    }

    fn owned(self) -> EffectsOwner {
        match self {
            Now::None => EffectsOwner::None,
            Now::Clip(id) => EffectsOwner::Clip(id),
            Now::File(p) => EffectsOwner::File(p.to_string()),
        }
    }
}

impl LayerState {
    /// The clip playing, else the file playing, else nobody.
    fn effects_owner_now(&self) -> Now<'_> {
        if let Some(id) = self.active_clip {
            return Now::Clip(id);
        }
        // A reopened project doesn't know which clip was playing: the one whose effects these are still is.
        if let EffectsOwner::Clip(id) = self.clip_effects_from
            && self.clips.iter().any(|k| k.id == id && k.source == self.source)
        {
            return Now::Clip(id);
        }
        file_of(&self.source).map_or(Now::None, Now::File)
    }

    /// The effects this layer runs, in order: the clip's (or file's), then its own.
    pub fn chain(&self) -> impl Iterator<Item = &EffectState> {
        self.clip_effects.iter().chain(&self.effects)
    }
}

impl Composition {
    /// Puts the right clip / file effects on each layer when what it plays changes, and keeps changes made to
    /// them in the clip they belong to (call every frame; files are kept by `store_file_effects`).
    pub fn sync_clip_effects(&mut self) {
        for i in 0..self.layers.len() {
            let now = self.layers[i].effects_owner_now();
            if now.is(&self.layers[i].clip_effects_from) {
                if matches!(now, Now::Clip(_)) {
                    self.store_clip_effects(i);
                }
                continue;
            }
            let owner = now.owned();
            // What was on it goes home first.
            self.store_clip_effects(i);
            self.store_file_effects_of(i);
            let effects = match &owner {
                EffectsOwner::None => Vec::new(),
                EffectsOwner::Clip(id) => self.layers.iter().flat_map(|l| &l.clips).find(|k| k.id == *id).map(|k| k.effects.clone()).unwrap_or_default(),
                EffectsOwner::File(p) => self.media_bin.iter().find(|m| m.path == *p).map(|m| m.effects.clone()).unwrap_or_default(),
            };
            let l = &mut self.layers[i];
            l.clip_effects = effects;
            l.clip_effects_from = owner;
        }
    }

    /// Layer `i`'s clip chain back into its clip (on whichever layer's row the clip is).
    fn store_clip_effects(&mut self, i: usize) {
        let EffectsOwner::Clip(id) = self.layers[i].clip_effects_from else { return };
        let Some((li, slot)) = self.layers.iter().enumerate().find_map(|(li, l)| l.clips.iter().position(|k| k.id == id).map(|s| (li, s))) else { return };
        if self.layers[li].clips[slot].effects != self.layers[i].clip_effects {
            let fx = self.layers[i].clip_effects.clone();
            self.layers[li].clips[slot].effects = fx;
        }
    }

    fn store_file_effects_of(&mut self, i: usize) {
        let l = &self.layers[i];
        let EffectsOwner::File(path) = &l.clip_effects_from else { return };
        if let Some(item) = self.media_bin.iter_mut().find(|m| m.path == *path)
            && item.effects != l.clip_effects
        {
            item.effects = l.clip_effects.clone();
        }
    }

    /// A file's own effects are dropped (also from layers playing it now). Returns how many there were.
    pub fn forget_file_effects(&mut self, path: &str) -> usize {
        let owner = EffectsOwner::File(path.to_string());
        for l in self.layers.iter_mut().filter(|l| l.clip_effects_from == owner) {
            l.clip_effects.clear();
        }
        self.media_bin.iter_mut().find(|m| m.path == path).map_or(0, |m| std::mem::take(&mut m.effects).len())
    }

    /// Changes made to a file's effects while it plays go back into the media bin (a few times a second).
    pub fn store_file_effects(&mut self) {
        for i in 0..self.layers.len() {
            self.store_file_effects_of(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(c: &mut Composition, name: &str) -> EffectState {
        EffectState { id: c.new_id(), effect: name.into(), enabled: true, values: Vec::new(), mix: 1.0, mods: Vec::new(), blend: Default::default() }
    }

    fn movie(name: &str) -> SourceSpec {
        SourceSpec::Movie { name: name.into(), path: Some(format!("/x/{name}")) }
    }

    #[test]
    fn a_clips_effects_come_and_go_with_it() {
        let mut c = Composition::default();
        let id = c.add_layer().id;
        c.place_clips(id, 0, vec![("a".into(), movie("a.mp4")), ("b".into(), movie("b.mp4"))]);
        let own = fx(&mut c, "invert");
        c.layer_mut(id).unwrap().effects.push(own.clone());
        c.press_clip(id, 0, 0.0);
        c.sync_clip_effects();
        let kaleido = fx(&mut c, "kaleido");
        // Added while clip A plays: it's A's.
        c.layer_mut(id).unwrap().clip_effects.push(kaleido.clone());
        c.sync_clip_effects();
        assert_eq!(c.layer(id).unwrap().clips[0].effects, vec![kaleido.clone()], "kept in the clip");
        c.press_clip(id, 1, 0.0);
        c.sync_clip_effects();
        let l = c.layer(id).unwrap();
        assert!(l.clip_effects.is_empty(), "B has none");
        assert_eq!(l.effects, vec![own.clone()], "the layer's own effects never change");
        c.press_clip(id, 0, 0.0);
        c.sync_clip_effects();
        assert_eq!(c.layer(id).unwrap().chain().map(|e| e.effect.as_str()).collect::<Vec<_>>(), ["kaleido", "invert"], "A's come back, before the layer's");
        // Taken off A: gone from A only.
        c.layer_mut(id).unwrap().clip_effects.clear();
        c.press_clip(id, 1, 0.0);
        c.sync_clip_effects();
        assert!(c.layer(id).unwrap().clips[0].effects.is_empty());
        assert_eq!(c.layer(id).unwrap().effects, vec![own]);
    }

    #[test]
    fn a_files_effects_follow_it_to_any_layer() {
        let mut c = Composition::default();
        c.add_media("a.mp4", "/x/a.mp4");
        let one = c.add_layer().id;
        c.set_layer_source(one, movie("a.mp4"), Vec::new(), String::new(), 0.0);
        c.sync_clip_effects();
        let glow = fx(&mut c, "glow");
        c.layer_mut(one).unwrap().clip_effects.push(glow.clone());
        c.store_file_effects();
        assert_eq!(c.media_bin[0].effects, vec![glow.clone()]);
        let two = c.add_layer().id;
        c.set_layer_source(two, movie("a.mp4"), Vec::new(), String::new(), 0.0);
        c.sync_clip_effects();
        assert_eq!(c.layer(two).unwrap().clip_effects, vec![glow.clone()], "on another layer too");
        assert!(c.layer(two).unwrap().effects.is_empty(), "not on the layer's own chain");
        // A clip made from it starts with them.
        c.place_clips(two, 3, vec![("a".into(), movie("a.mp4"))]);
        assert_eq!(c.layer(two).unwrap().clips[3].effects, vec![glow]);
        assert_eq!(c.forget_file_effects("/x/a.mp4"), 1);
        c.store_file_effects();
        assert!(c.media_bin[0].effects.is_empty() && c.layer(two).unwrap().clip_effects.is_empty(), "forgotten, also where it plays");
        // Something else on the layer: the file's effects leave with it.
        c.set_layer_source(one, movie("b.mp4"), Vec::new(), String::new(), 0.0);
        c.sync_clip_effects();
        assert!(c.layer(one).unwrap().clip_effects.is_empty());
    }

    #[test]
    fn a_reopened_project_keeps_the_playing_clips_effects() {
        let mut c = Composition::default();
        let id = c.add_layer().id;
        c.place_clips(id, 0, vec![("a".into(), movie("a.mp4"))]);
        c.press_clip(id, 0, 0.0);
        c.sync_clip_effects();
        let wave = fx(&mut c, "wave");
        c.layer_mut(id).unwrap().clip_effects.push(wave.clone());
        let mut back: Composition = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back.layer(id).unwrap().active_clip, None);
        back.sync_clip_effects();
        assert_eq!(back.layer(id).unwrap().clip_effects, vec![wave.clone()]);
        assert_eq!(back.layer(id).unwrap().clips[0].effects, vec![wave], "and they're the clip's");
    }

    #[test]
    fn clip_chains_are_named_apart_from_layers() {
        let mut c = Composition::default();
        let id = c.add_layer().id;
        c.place_clips(id, 0, vec![("a".into(), movie("a.mp4"))]);
        c.press_clip(id, 0, 0.0);
        c.sync_clip_effects();
        let blur = fx(&mut c, "blur");
        c.effects_mut(clip_chain(id)).unwrap().push(blur.clone());
        assert_eq!(c.layer(id).unwrap().clip_effects, vec![blur.clone()]);
        assert!(c.layer(id).unwrap().effects.is_empty());
        assert!(c.all_effects().any(|e| e.id == blur.id), "its effects can be mapped and modulated");
        assert_eq!(chain_layer(clip_chain(id)), Some(id));
        assert_eq!(chain_layer(id), None);
        assert_eq!(chain_layer(crate::MASTER_ID), None);
    }

    #[test]
    fn older_saves_open_without_clip_effects() {
        let k: Clip = serde_json::from_str(r#"{"id":3,"name":"a"}"#).unwrap();
        assert!(k.effects.is_empty());
        let l: LayerState = serde_json::from_str(r#"{"id":4}"#).unwrap();
        assert!(l.clip_effects.is_empty() && l.clip_effects_from.is_none());
        let m: MediaItem = serde_json::from_str(r#"{"name":"a","path":"/a"}"#).unwrap();
        assert!(m.effects.is_empty());
    }
}
