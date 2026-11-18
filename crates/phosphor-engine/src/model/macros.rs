//! Dials (Resolume's dashboard): up to eight named dials on a layer, group, clip or MASTER, saved with it. Each dial
//! drives any number of settings anywhere, each through its own part of the dial, value effects and range (a
//! `Binding`), so one knob plays a whole look. A dial is a control like any other (`ControlTarget::Macro`): map a
//! MIDI knob or OSC to it, or let it follow something.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::*;
use crate::MASTER_ID;

/// Dials one layer, group, clip or MASTER can have.
pub const MAX_DIALS: usize = 8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MacroDial {
    pub name: String,
    /// 0..1.
    pub value: f32,
    /// What it drives.
    pub targets: Vec<Binding>,
}

impl Default for MacroDial {
    fn default() -> Self {
        Self { name: "Dial".into(), value: 0.0, targets: Vec::new() }
    }
}

impl MacroDial {
    /// A new dial named after its place ("Dial 3").
    pub fn numbered(n: usize) -> Self {
        Self { name: format!("Dial {n}"), ..Default::default() }
    }
}

/// A control that drives others through its own list of targets (a Control Surface widget, one of its bars, a
/// dial): a link dropped on or from it goes into that list rather than making a follow.
pub fn is_driver(t: &ControlTarget) -> bool {
    matches!(t, ControlTarget::Surface(_) | ControlTarget::SurfaceY(_) | ControlTarget::SurfaceBar { .. } | ControlTarget::Macro { .. })
}

/// Who drives whom when control `carried` is dropped on control `landed`: a driver carried onto a control drives it;
/// a control carried onto a driver is driven by it; None = neither is a driver, so `landed` follows `carried`.
pub fn link_roles(carried: &ControlTarget, landed: &ControlTarget) -> Option<(ControlTarget, ControlTarget)> {
    if is_driver(carried) {
        Some((carried.clone(), landed.clone()))
    } else if is_driver(landed) {
        Some((landed.clone(), carried.clone()))
    } else {
        None
    }
}

impl Composition {
    /// The dials of a layer, group, clip (by id) or MASTER (`MASTER_ID`).
    pub fn dials(&self, owner: u64) -> Option<&Vec<MacroDial>> {
        if owner == MASTER_ID {
            return Some(&self.master_dials);
        }
        if let Some(l) = self.layers.iter().find(|l| l.id == owner) {
            return Some(&l.dials);
        }
        if let Some(g) = self.groups.iter().find(|g| g.id == owner) {
            return Some(&g.dials);
        }
        (owner != 0).then(|| self.layers.iter().flat_map(|l| &l.clips).find(|c| c.id == owner).map(|c| &c.dials)).flatten()
    }

    pub fn dials_mut(&mut self, owner: u64) -> Option<&mut Vec<MacroDial>> {
        if owner == MASTER_ID {
            return Some(&mut self.master_dials);
        }
        if let Some(i) = self.layers.iter().position(|l| l.id == owner) {
            return Some(&mut self.layers[i].dials);
        }
        if let Some(i) = self.groups.iter().position(|g| g.id == owner) {
            return Some(&mut self.groups[i].dials);
        }
        if owner == 0 {
            return None;
        }
        self.layers.iter_mut().flat_map(|l| &mut l.clips).find(|c| c.id == owner).map(|c| &mut c.dials)
    }

    /// Every dial in the project: (owner, the owner's name, its place, the dial).
    pub fn all_dials(&self) -> Vec<(u64, String, usize, &MacroDial)> {
        let mut owners: Vec<(u64, String, &[MacroDial])> = vec![(MASTER_ID, "MASTER".into(), &self.master_dials)];
        owners.extend(self.groups.iter().filter(|g| !g.dials.is_empty()).map(|g| (g.id, g.name.clone(), &g.dials[..])));
        for l in &self.layers {
            if !l.dials.is_empty() {
                owners.push((l.id, l.name.clone(), &l.dials));
            }
            owners.extend(l.clips.iter().filter(|c| !c.dials.is_empty()).map(|c| (c.id, format!("{} · clip {}", l.name, c.name), &c.dials[..])));
        }
        owners.into_iter().flat_map(|(owner, name, dials)| dials.iter().enumerate().map(move |(i, d)| (owner, name.clone(), i, d))).collect()
    }

    /// Whether the project has any dial (nothing to run per frame without one).
    pub fn has_dials(&self) -> bool {
        !self.master_dials.is_empty() || self.groups.iter().any(|g| !g.dials.is_empty()) || self.layers.iter().any(|l| !l.dials.is_empty() || l.clips.iter().any(|c| !c.dials.is_empty()))
    }

    /// The list of targets a driver (`is_driver`) drives: a surface widget's (its Y axis's, one bar's) or a dial's.
    pub fn driver_bindings_mut(&mut self, t: &ControlTarget) -> Option<&mut Vec<Binding>> {
        match t {
            ControlTarget::Surface(id) => Some(&mut self.surface.widget_mut(*id)?.bindings),
            ControlTarget::SurfaceY(id) => Some(&mut self.surface.widget_mut(*id)?.bindings_y),
            ControlTarget::SurfaceBar { widget, bar } => Some(&mut self.surface.widget_mut(*widget)?.bars.get_mut(*bar as usize)?.bindings),
            ControlTarget::Macro { owner, index } => Some(&mut self.dials_mut(*owner)?.get_mut(*index as usize)?.targets),
            _ => None,
        }
    }

    /// `driver` drives `target` from now on (added to its list once; a driver can't drive itself). Returns whether
    /// it was added.
    pub fn link_driver(&mut self, driver: &ControlTarget, target: ControlTarget) -> bool {
        if *driver == target {
            return false;
        }
        let Some(list) = self.driver_bindings_mut(driver) else { return false };
        if list.iter().any(|b| b.target == target) {
            return false;
        }
        list.push(Binding::new(target));
        true
    }

    /// Dials whose value changed since `seen` was last filled: what each of their targets gets now. A dial seen for
    /// the first time (a project just opened, a dial just made) pushes nothing.
    pub fn dial_sends(&self, seen: &mut HashMap<(u64, usize), f32>) -> Vec<(ControlTarget, f32)> {
        let mut sends = Vec::new();
        if !self.has_dials() {
            seen.clear();
            return sends;
        }
        let dials = self.all_dials();
        for (owner, _, i, d) in &dials {
            if seen.get(&(*owner, *i)).is_some_and(|old| (old - d.value).abs() > 1e-5) {
                sends.extend(d.targets.iter().map(|b| (b.target.clone(), b.value(d.value))));
            }
        }
        seen.clear();
        seen.extend(dials.iter().map(|(owner, _, i, d)| ((*owner, *i), d.value)));
        sends
    }
}

impl ControlTarget {
    /// The same control on a copy whose ids changed (a duplicated or loaded layer): every id `new_id` knows is
    /// swapped for its new one; anything else is left as it is.
    pub fn remap_ids(&mut self, new_id: impl Fn(u64) -> Option<u64>) {
        use ControlTarget as T;
        let id = match self {
            T::LayerOpacity(id)
            | T::LayerEnabled(id)
            | T::EffectMix(id)
            | T::EffectEnabled(id)
            | T::LayerSpeed(id)
            | T::LayerFxBypass(id)
            | T::LayerLoopIn(id)
            | T::LayerLoopOut(id)
            | T::LayerDirection(id)
            | T::LayerScrub(id)
            | T::LayerVolume(id)
            | T::LayerText(id)
            | T::LayerClipPick(id)
            | T::LayerX(id)
            | T::LayerY(id)
            | T::LayerScale(id)
            | T::LayerRotation(id)
            | T::LayerBlend(id)
            | T::LayerRandomClip(id)
            | T::StopLayer(id)
            | T::SelectLayer(id) => id,
            T::GeneratorParam { layer: id, .. }
            | T::EffectParam { effect: id, .. }
            | T::TriggerClip { layer: id, .. }
            | T::LayerJump { layer: id, .. }
            | T::LayerTextStep { layer: id, .. }
            | T::LayerCorner { layer: id, .. }
            | T::LayerTransform { layer: id, .. }
            | T::TextPart { layer: id, .. }
            | T::TransitionParam { layer: id, .. }
            | T::MixerParam { layer: id, .. }
            | T::Macro { owner: id, .. } => id,
            _ => return,
        };
        if let Some(n) = new_id(*id) {
            *id = n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dial_drives_its_targets_when_it_moves() {
        let mut comp = Composition::default();
        let a = comp.add_layer().id;
        let b = comp.add_layer().id;
        comp.layer_mut(a).unwrap().dials.push(MacroDial::numbered(1));
        let dial = ControlTarget::Macro { owner: a, index: 0 };
        assert!(comp.link_driver(&dial, ControlTarget::LayerOpacity(b)));
        assert!(!comp.link_driver(&dial, ControlTarget::LayerOpacity(b)), "once");
        assert!(!comp.link_driver(&dial, dial.clone()), "not itself");
        // Its second target listens to the top half only, upside down.
        comp.link_driver(&dial, ControlTarget::LayerSpeed(a));
        let t = &mut comp.dials_mut(a).unwrap()[0].targets[1];
        t.part = FaderPart { from: 0.5, to: 1.0 };
        t.range.invert = true;
        let mut seen = HashMap::new();
        assert!(comp.dial_sends(&mut seen).is_empty(), "first look: nothing moved");
        assert!(comp.dial_sends(&mut seen).is_empty(), "still nothing");
        comp.dials_mut(a).unwrap()[0].value = 0.75;
        let sends = comp.dial_sends(&mut seen);
        assert_eq!(sends.len(), 2);
        assert_eq!(sends[0], (ControlTarget::LayerOpacity(b), 0.75));
        assert_eq!(sends[1].0, ControlTarget::LayerSpeed(a));
        assert!((sends[1].1 - 0.5).abs() < 1e-5, "half way up its part, inverted: {}", sends[1].1);
        assert!(comp.dial_sends(&mut seen).is_empty(), "no change, nothing sent");
    }

    #[test]
    fn dials_live_on_layers_groups_clips_and_master() {
        let mut comp = Composition::default();
        let l = comp.add_layer().id;
        let clip_id = comp.new_id();
        comp.layer_mut(l).unwrap().clips.push(Clip { id: clip_id, ..Default::default() });
        let g = comp.new_id();
        comp.groups.push(LayerGroup { id: g, ..Default::default() });
        for owner in [l, g, clip_id, MASTER_ID] {
            comp.dials_mut(owner).unwrap().push(MacroDial::numbered(1));
            assert_eq!(comp.dials(owner).map(Vec::len), Some(1), "{owner}");
        }
        assert!(comp.dials(0).is_none() && comp.dials(12345).is_none());
        assert_eq!(comp.all_dials().len(), 4);
        // A surface widget is a driver too.
        assert!(is_driver(&ControlTarget::Surface(1)) && is_driver(&ControlTarget::Macro { owner: l, index: 0 }));
        assert!(!is_driver(&ControlTarget::LayerOpacity(l)));
        // Dropped links: a widget onto a slider drives it; a slider onto a dial is driven by the dial; two plain
        // controls: the one landed on follows.
        let (w, d, o) = (ControlTarget::Surface(1), ControlTarget::Macro { owner: l, index: 0 }, ControlTarget::LayerOpacity(l));
        assert_eq!(link_roles(&w, &o), Some((w.clone(), o.clone())));
        assert_eq!(link_roles(&o, &d), Some((d.clone(), o.clone())));
        assert_eq!(link_roles(&d, &o), Some((d.clone(), o.clone())));
        assert_eq!(link_roles(&o, &ControlTarget::LayerSpeed(l)), None);
    }

    #[test]
    fn old_saves_open_without_dials_and_new_ones_keep_them() {
        // A layer saved before dials existed opens with none; one with a dial keeps it.
        let old: LayerState = serde_json::from_str(r#"{"id": 4, "name": "Old"}"#).unwrap();
        assert!(old.dials.is_empty());
        let json = serde_json::to_string(&LayerState::default()).unwrap();
        assert!(!json.contains("dials"), "nothing written while there are none");
        let mut l = LayerState { id: 9, ..Default::default() };
        l.dials.push(MacroDial { name: "Look".into(), value: 0.4, targets: vec![Binding::new(ControlTarget::LayerOpacity(9))] });
        let back: LayerState = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert_eq!(back.dials, l.dials);
    }

    #[test]
    fn ids_follow_a_copy() {
        let mut t = ControlTarget::EffectParam { effect: 5, index: 2 };
        t.remap_ids(|id| (id == 5).then_some(50));
        assert_eq!(t, ControlTarget::EffectParam { effect: 50, index: 2 });
        let mut other = ControlTarget::LayerOpacity(7);
        other.remap_ids(|id| (id == 5).then_some(50));
        assert_eq!(other, ControlTarget::LayerOpacity(7), "not one of the copy's ids");
    }
}
