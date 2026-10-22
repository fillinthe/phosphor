//! Editing several layers at once: what changed on one layer is copied to the others.
use super::*;

/// Settings that belong to one layer and never copy over (who it is, what it plays, where it sits).
const OWN: &[&str] = &["id", "name", "source", "generator_values", "generator_mods", "clips", "parent", "comp", "collection", "effects"];

impl Composition {
    /// Copies every setting that differs between `before` and the layer with its id now onto the layers in `to`.
    /// Returns how many settings changed.
    pub fn copy_layer_changes(&mut self, before: &LayerState, to: &[u64]) -> usize {
        let Some(after) = self.layer(before.id) else { return 0 };
        let (Ok(serde_json::Value::Object(old)), Ok(serde_json::Value::Object(new))) = (serde_json::to_value(before), serde_json::to_value(after)) else {
            return 0;
        };
        let changed: Vec<(String, serde_json::Value)> =
            new.into_iter().filter(|(k, v)| !OWN.contains(&k.as_str()) && old.get(k) != Some(v)).collect();
        if changed.is_empty() {
            return 0;
        }
        for l in self.layers.iter_mut().filter(|l| to.contains(&l.id) && l.id != before.id) {
            let Ok(serde_json::Value::Object(mut v)) = serde_json::to_value(&*l) else { continue };
            for (k, val) in &changed {
                v.insert(k.clone(), val.clone());
            }
            if let Ok(mut edited) = serde_json::from_value::<LayerState>(serde_json::Value::Object(v)) {
                // Not saved, so not in the JSON: keep what's playing.
                edited.active_clip = l.active_clip;
                edited.transition = l.transition.take();
                edited.flash_return = l.flash_return.take();
                edited.voice_turn = l.voice_turn;
                edited.source = std::mem::replace(&mut l.source, SourceSpec::None);
                *l = edited;
            }
        }
        changed.len()
    }
}

impl Composition {
    /// Copies how a clip plays (its trigger mode, mute and own settings) from `before` → the clip at `from` now, onto
    /// the clips at `to` (layer, slot), for whatever changed. Returns how many settings changed.
    pub fn copy_clip_changes(&mut self, before: &Clip, from: (u64, usize), to: &[(u64, usize)]) -> usize {
        let Some(after) = self.layer(from.0).and_then(|l| l.clips.get(from.1)).cloned() else { return 0 };
        let (trigger, muted, own) = (after.trigger != before.trigger, after.muted != before.muted, after.own != before.own);
        for &(layer, slot) in to.iter().filter(|t| **t != from) {
            if let Some(k) = self.layer_mut(layer).and_then(|l| l.clips.get_mut(slot)).filter(|k| !k.is_empty()) {
                if trigger {
                    k.trigger = after.trigger;
                }
                if muted {
                    k.muted = after.muted;
                }
                if own {
                    k.own.clone_from(&after.own);
                }
            }
        }
        [trigger, muted, own].iter().filter(|c| **c).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_changes_go_to_the_selected_clips() {
        let mut c = Composition::default();
        let clip = |id| Clip { id, name: format!("C{id}"), source: SourceSpec::Generator("plasma".into()), ..Default::default() };
        c.layers.push(LayerState { id: 1, clips: vec![clip(10), clip(11), Clip::default()], ..Default::default() });
        let before = c.layers[0].clips[0].clone();
        c.layers[0].clips[0].trigger = TriggerMode::Flash;
        c.layers[0].clips[0].name = "Renamed".into();
        assert_eq!(c.copy_clip_changes(&before, (1, 0), &[(1, 1), (1, 2)]), 1);
        assert_eq!((c.layers[0].clips[1].trigger, c.layers[0].clips[1].name.as_str()), (TriggerMode::Flash, "C11"), "names stay");
        assert!(c.layers[0].clips[2].is_empty(), "empty slots stay empty");
    }

    #[test]
    fn a_change_goes_to_every_selected_layer() {
        let mut c = Composition::default();
        for id in 1..=3 {
            c.layers.push(LayerState { id, name: format!("L{id}"), opacity: 1.0, ..Default::default() });
        }
        let before = c.layers[0].clone();
        c.layers[0].opacity = 0.25;
        c.layers[0].blend = BlendMode::Add;
        c.layers[0].name = "Renamed".into();
        assert_eq!(c.copy_layer_changes(&before, &[2]), 2);
        assert_eq!((c.layers[1].opacity, c.layers[1].blend, c.layers[1].name.as_str()), (0.25, BlendMode::Add, "L2"), "names stay");
        assert_eq!(c.layers[2].opacity, 1.0, "not selected");
    }
}
