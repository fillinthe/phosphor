//! What's been changed on a layer, at a glance (the badges on layer cards).

use super::*;

/// A layer's changes from plain: what its card shows as badges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayerMarks {
    /// Effects running: the clip's (or file's) and the layer's own.
    pub clip_fx: usize,
    pub fx: usize,
    /// Speed, direction or BPM sync changed.
    pub playback: bool,
    /// An In / Out point or a beat loop.
    pub cut: bool,
    /// Moved, scaled, rotated, flipped, cropped, corner-pinned or placed in 3D.
    pub moved: bool,
    /// How many values move by themselves (LFO, sound, beat…).
    pub animated: usize,
}

impl LayerState {
    pub fn marks(&self) -> LayerMarks {
        let on = |fx: &[EffectState]| fx.iter().filter(|e| e.enabled).count();
        let settings = self.clip_settings();
        let t = &self.transform;
        let still = LayerTransform { scale_mod: None, rotation_mod: None, x_mod: None, y_mod: None, ..t.clone() };
        let transform_mods = [&t.scale_mod, &t.rotation_mod, &t.x_mod, &t.y_mod].iter().filter(|m| m.is_some()).count();
        let fx_mods: usize = self.chain().map(|e| e.mods.len()).sum();
        LayerMarks {
            clip_fx: on(&self.clip_effects),
            fx: on(&self.effects),
            playback: matches!(self.source, SourceSpec::Movie { .. }) && settings.changes_playback(),
            cut: matches!(self.source, SourceSpec::Movie { .. }) && settings.is_cut(),
            moved: still != LayerTransform::default(),
            animated: transform_mods + fx_mods + self.generator_mods.len() + usize::from(self.opacity_mod.is_some()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_layer_has_no_marks() {
        let mut l = LayerState { source: SourceSpec::Movie { name: "a".into(), path: Some("/a".into()) }, ..Default::default() };
        assert_eq!(l.marks(), LayerMarks::default());
        l.rate = 2.0;
        l.loop_in = 0.25;
        l.transform.rotation = 10.0;
        l.opacity_mod = Some(Default::default());
        let m = l.marks();
        assert!(m.playback && m.cut && m.moved && m.animated == 1);
    }
}
