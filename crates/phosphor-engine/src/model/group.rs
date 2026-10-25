//! Layer groups: neighbouring layers mixed together first, then treated like one layer (opacity, blend, effects).
//! All stack rules live here so the Layers panel, drag and drop and controllers share them.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerGroup {
    /// From `Composition::new_id`, so it never clashes with a layer id (a mapping surface can show either).
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub opacity: f32,
    pub opacity_mod: Option<Modulation>,
    pub blend: BlendMode,
    /// Run on the group's mixed picture.
    pub effects: Vec<EffectState>,
    pub fx_bypass: bool,
    /// Blended into the mix. Off = only mapping surfaces that pick this group show it.
    pub in_mix: bool,
    /// Members hidden in the Layers panel.
    pub collapsed: bool,
}

/// A layer's or group's mix controls, borrowed (see `Composition::mix_controls_mut`).
pub struct MixControls<'a> {
    pub opacity: &'a mut f32,
    pub enabled: &'a mut bool,
    pub fx_bypass: &'a mut bool,
}

impl Default for LayerGroup {
    fn default() -> Self {
        Self { id: 0, name: "Group".into(), enabled: true, opacity: 1.0, opacity_mod: None, blend: BlendMode::Normal, effects: Vec::new(), fx_bypass: false, in_mix: true, collapsed: false }
    }
}

impl Composition {
    pub fn group(&self, id: u64) -> Option<&LayerGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    pub fn group_mut(&mut self, id: u64) -> Option<&mut LayerGroup> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    /// The mix controls (opacity, on/off, effects bypass) of a layer, a group or a comp: what controllers drive.
    pub fn mix_controls(&self, id: u64) -> Option<(f32, bool, bool)> {
        match self.layers.iter().find(|l| l.id == id) {
            Some(l) => Some((l.opacity, l.enabled, l.fx_bypass)),
            None => self.group(id).map(|g| (g.opacity, g.enabled, g.fx_bypass)).or_else(|| self.comp(id).map(|c| (c.opacity, c.enabled, c.fx_bypass))),
        }
    }

    pub fn mix_controls_mut(&mut self, id: u64) -> Option<MixControls<'_>> {
        if let Some(i) = self.layers.iter().position(|l| l.id == id) {
            let l = &mut self.layers[i];
            return Some(MixControls { opacity: &mut l.opacity, enabled: &mut l.enabled, fx_bypass: &mut l.fx_bypass });
        }
        if self.group(id).is_some() {
            return self.group_mut(id).map(|g| MixControls { opacity: &mut g.opacity, enabled: &mut g.enabled, fx_bypass: &mut g.fx_bypass });
        }
        self.comp_mut(id).map(|c| MixControls { opacity: &mut c.opacity, enabled: &mut c.enabled, fx_bypass: &mut c.fx_bypass })
    }

    /// The effect chain of MASTER (`MASTER_ID`), a layer, a group, a comp or a mapping slice.
    pub fn effects(&self, target: u64) -> Option<&Vec<EffectState>> {
        if target == crate::MASTER_ID {
            return Some(&self.master_effects);
        }
        self.layers.iter().find(|l| l.id == target).map(|l| &l.effects).or_else(|| self.group(target).map(|g| &g.effects)).or_else(|| self.comp(target).map(|c| &c.effects)).or_else(|| self.slice(target).map(|s| &s.effects))
    }

    pub fn effects_mut(&mut self, target: u64) -> Option<&mut Vec<EffectState>> {
        if target == crate::MASTER_ID {
            return Some(&mut self.master_effects);
        }
        if self.layers.iter().any(|l| l.id == target) {
            return self.layers.iter_mut().find(|l| l.id == target).map(|l| &mut l.effects);
        }
        if self.groups.iter().any(|g| g.id == target) {
            return self.groups.iter_mut().find(|g| g.id == target).map(|g| &mut g.effects);
        }
        if self.comps.iter().any(|c| c.id == target) {
            return self.comp_mut(target).map(|c| &mut c.effects);
        }
        self.slice_mut(target).map(|s| &mut s.effects)
    }

    /// The effects that reach a layer from above its own chain, nearest first: its group's, its comp's (when the
    /// project has comps), then MASTER's. Each is (where it is, which effect, whether it's running). A removed
    /// layer effect that "stays on" is usually one of these.
    /// Is this layer in the show now (on, in the mix, not soloed away, its fader and crossfade side up)? A layer
    /// that isn't can still be watched in its preview before it goes live.
    pub fn layer_live(&self, layer: u64) -> bool {
        let Some(l) = self.layers.iter().find(|l| l.id == layer) else { return false };
        let any_solo = self.layers.iter().any(|l| l.solo);
        l.enabled && l.in_mix && (!any_solo || l.solo) && l.opacity * l.group.gain(self.crossfader, self.fader_curve) > 0.001
    }

    pub fn inherited_effects(&self, layer: u64) -> Vec<(String, String, bool)> {
        let Some(l) = self.layers.iter().find(|l| l.id == layer) else { return Vec::new() };
        let name = |id: &str| crate::shaders::effect(id).map_or_else(|| id.to_string(), |d| d.name.to_string());
        let mut out = Vec::new();
        let mut add = |scope: String, chain: &[EffectState], off: bool| {
            for e in chain {
                out.push((scope.clone(), name(&e.effect), e.enabled && !off));
            }
        };
        if let Some(g) = l.parent.and_then(|id| self.group(id)) {
            add(format!("group {}", g.name), &g.effects, g.fx_bypass);
        }
        if let Some(c) = self.comp_of(l).and_then(|id| self.comp(id)) {
            add(format!("comp {}", c.name), &c.effects, c.fx_bypass);
        }
        add("MASTER".to_string(), &self.master_effects, self.master_fx_bypass);
        out
    }

    /// Every effect in the project (MASTER, layers, groups, comps, mapping slices).
    pub fn all_effects(&self) -> impl Iterator<Item = &EffectState> {
        self.master_effects
            .iter()
            .chain(self.layers.iter().flat_map(|l| l.effects.iter()))
            .chain(self.groups.iter().flat_map(|g| g.effects.iter()))
            .chain(self.comps.iter().flat_map(|c| c.effects.iter()))
            .chain(self.outputs.iter().flat_map(|o| o.mapping.surfaces.iter().flat_map(|s| s.effects.iter())))
    }

    pub fn all_effects_mut(&mut self) -> impl Iterator<Item = &mut EffectState> {
        self.master_effects
            .iter_mut()
            .chain(self.layers.iter_mut().flat_map(|l| l.effects.iter_mut()))
            .chain(self.groups.iter_mut().flat_map(|g| g.effects.iter_mut()))
            .chain(self.comps.iter_mut().flat_map(|c| c.effects.iter_mut()))
            .chain(self.outputs.iter_mut().flat_map(|o| o.mapping.surfaces.iter_mut().flat_map(|s| s.effects.iter_mut())))
    }

    /// Stack positions (bottom, top) of a group's members.
    pub fn group_span(&self, id: u64) -> Option<(usize, usize)> {
        let first = self.layers.iter().position(|l| l.parent == Some(id))?;
        let last = self.layers.iter().rposition(|l| l.parent == Some(id))?;
        Some((first, last))
    }

    /// Puts these layers in a new group (taking them out of any group they were in). They gather where the top one is.
    pub fn create_group(&mut self, layers: &[u64]) -> Option<u64> {
        // All from one comp (the first layer's).
        let home = layers.first().and_then(|id| self.layers.iter().find(|l| l.id == *id)).map(|l| self.comp_of(l));
        let members: Vec<u64> = layers.iter().copied().filter(|id| self.layers.iter().any(|l| l.id == *id && Some(self.comp_of(l)) == home)).collect();
        if members.is_empty() {
            return None;
        }
        let id = self.new_id();
        let n = self.groups.len() + 1;
        self.groups.push(LayerGroup { id, name: format!("Group {n}"), ..Default::default() });
        for l in self.layers.iter_mut().filter(|l| members.contains(&l.id)) {
            l.parent = Some(id);
        }
        self.normalize_groups();
        Some(id)
    }

    /// Dissolves a group; its layers stay where they are.
    pub fn ungroup(&mut self, id: u64) {
        for l in self.layers.iter_mut().filter(|l| l.parent == Some(id)) {
            l.parent = None;
        }
        self.groups.retain(|g| g.id != id);
    }

    /// Moves a layer one step up (towards the top) or down. A member at its group's edge steps out of the group;
    /// a layer next to a group jumps over the whole group.
    pub fn move_layer(&mut self, id: u64, up: bool) {
        let Some(i) = self.layers.iter().position(|l| l.id == id) else { return };
        let next = if up { i.checked_add(1).filter(|j| *j < self.layers.len()) } else { i.checked_sub(1) };
        // The edge of its comp is the edge of the stack.
        let Some(j) = next.filter(|j| self.same_comp(i, *j)) else {
            // Already at the end of the stack: only leaving the group is left to do.
            self.layers[i].parent = None;
            self.normalize_groups();
            return;
        };
        let (mine, theirs) = (self.layers[i].parent, self.layers[j].parent);
        match (mine, theirs) {
            (Some(g), Some(h)) if g == h => self.layers.swap(i, j),
            (Some(_), _) => self.layers[i].parent = None,
            (None, Some(h)) => {
                let Some((a, b)) = self.group_span(h) else { return };
                let layer = self.layers.remove(i);
                self.layers.insert(if up { b } else { a }, layer);
            }
            (None, None) => self.layers.swap(i, j),
        }
        self.normalize_groups();
    }

    /// Moves a whole group one step (past the next layer, or past the next group as a whole).
    pub fn move_group(&mut self, id: u64, up: bool) {
        let Some((a, b)) = self.group_span(id) else { return };
        let len = b - a + 1;
        if up {
            let Some(next) = self.layers.get(b + 1).filter(|_| self.same_comp(b, b + 1)) else { return };
            let end = next.parent.and_then(|h| self.group_span(h)).map_or(b + 1, |(_, d)| d);
            self.layers[a..=end].rotate_left(len);
        } else {
            let Some(prev) = a.checked_sub(1).filter(|p| self.same_comp(a, *p)).and_then(|p| self.layers.get(p)) else { return };
            let start = prev.parent.and_then(|h| self.group_span(h)).map_or(a - 1, |(c, _)| c);
            self.layers[start..=b].rotate_right(len);
        }
    }

    /// Drag and drop: `from` goes where `onto` is and joins `onto`'s group (or leaves its own).
    pub fn drop_layer(&mut self, from: u64, onto: u64) {
        if from == onto {
            return;
        }
        let (Some(a), Some(b)) = (self.layers.iter().position(|l| l.id == from), self.layers.iter().position(|l| l.id == onto)) else { return };
        let parent = self.layers[b].parent;
        let comp = self.layers[b].comp;
        let mut moved = self.layers.remove(a);
        moved.parent = parent;
        moved.comp = comp;
        self.layers.insert(b, moved);
        self.normalize_groups();
    }

    /// Repairs groups after any change (or loading): unknown groups are dropped, empty groups removed, and each
    /// group's members gathered next to each other at its top member's place.
    pub fn normalize_groups(&mut self) {
        self.normalize_comps();
        let known: HashSet<u64> = self.groups.iter().map(|g| g.id).collect();
        for l in &mut self.layers {
            if l.parent.is_some_and(|p| !known.contains(&p)) {
                l.parent = None;
            }
        }
        let layers = &self.layers;
        self.groups.retain(|g| layers.iter().any(|l| l.parent == Some(g.id)));
        // A group lives in one comp: its top member's.
        for id in self.groups.iter().map(|g| g.id).collect::<Vec<_>>() {
            let home = self.layers.iter().rev().find(|l| l.parent == Some(id)).map(|l| l.comp);
            for l in self.layers.iter_mut().filter(|l| l.parent == Some(id)) {
                l.comp = home.flatten();
            }
        }
        self.normalize_comps();
        for id in self.groups.iter().map(|g| g.id).collect::<Vec<_>>() {
            let at: Vec<usize> = self.layers.iter().enumerate().filter(|(_, l)| l.parent == Some(id)).map(|(i, _)| i).collect();
            let (first, top) = (at[0], at[at.len() - 1]);
            if top - first + 1 == at.len() {
                continue;
            }
            let mut members = Vec::with_capacity(at.len());
            for &i in at.iter().rev() {
                members.push(self.layers.remove(i));
            }
            members.reverse();
            let start = top + 1 - members.len();
            self.layers.splice(start..start, members);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Layers named by id, bottom first, with their group ids.
    fn stack(c: &Composition) -> Vec<(u64, Option<u64>)> {
        c.layers.iter().map(|l| (l.id, l.parent)).collect()
    }

    fn comp(n: usize) -> Composition {
        let mut c = Composition::default();
        for _ in 0..n {
            c.add_layer();
        }
        c
    }

    #[test]
    fn grouping_gathers_members_at_the_top_one() {
        let mut c = comp(4); // ids 1..=4, bottom to top
        let g = c.create_group(&[1, 3]).unwrap();
        assert_eq!(stack(&c), vec![(2, None), (1, Some(g)), (3, Some(g)), (4, None)]);
        assert_eq!(c.group_span(g), Some((1, 2)));
    }

    #[test]
    fn moving_steps_out_of_groups_and_over_them() {
        let mut c = comp(4);
        let g = c.create_group(&[2, 3]).unwrap();
        // 1 jumps over the whole group.
        c.move_layer(1, true);
        assert_eq!(stack(&c), vec![(2, Some(g)), (3, Some(g)), (1, None), (4, None)]);
        // Inside the group: swap.
        c.move_layer(2, true);
        assert_eq!(stack(&c), vec![(3, Some(g)), (2, Some(g)), (1, None), (4, None)]);
        // At the group's top edge: leaves the group, stays put.
        c.move_layer(2, true);
        assert_eq!(stack(&c), vec![(3, Some(g)), (2, None), (1, None), (4, None)]);
        // Last member leaves at the bottom of the stack: group disappears.
        c.move_layer(3, false);
        assert!(c.groups.is_empty());
    }

    #[test]
    fn groups_move_as_blocks_and_drops_join() {
        let mut c = comp(5);
        let g = c.create_group(&[1, 2]).unwrap();
        let h = c.create_group(&[3, 4]).unwrap();
        c.move_group(g, true);
        assert_eq!(stack(&c), vec![(3, Some(h)), (4, Some(h)), (1, Some(g)), (2, Some(g)), (5, None)]);
        c.move_group(g, false);
        assert_eq!(stack(&c), vec![(1, Some(g)), (2, Some(g)), (3, Some(h)), (4, Some(h)), (5, None)]);
        c.drop_layer(5, 1);
        assert_eq!(c.layers.iter().find(|l| l.id == 5).unwrap().parent, Some(g));
        assert_eq!(c.group_span(g).map(|(a, b)| b - a), Some(2));
        c.ungroup(h);
        assert!(c.layers.iter().all(|l| l.parent != Some(h)) && c.group(h).is_none());
    }

    /// Effects from the group, comp and MASTER are listed for a layer, so one that "stays on" can be found.
    #[test]
    fn a_layer_lists_the_effects_from_above_it() {
        let mut c = Composition::default();
        let a = c.add_layer().id;
        let b = c.add_layer().id;
        c.add_comp("Side");
        // (The first comp, "Main", is made for the layers that exist: that's the layers' comp.)
        let comp = c.comps[0].id;
        let g = c.create_group(&[a, b]).expect("group");
        for (target, fx) in [(g, "invert"), (comp, "kaleidoscope"), (crate::MASTER_ID, "blur")] {
            if let Some(e) = c.new_effect(fx) {
                c.effects_mut(target).unwrap().push(e);
            }
        }
        let own = c.new_effect("invert").unwrap();
        c.layer_mut(a).unwrap().effects.push(own);
        let inh = c.inherited_effects(a);
        let scopes: Vec<&str> = inh.iter().map(|(s, _, _)| s.split(' ').next().unwrap()).collect();
        assert_eq!(scopes, ["group", "comp", "MASTER"], "nearest first; the layer's own chain isn't in it: {inh:?}");
        c.comp_mut(comp).unwrap().fx_bypass = true;
        assert!(!c.inherited_effects(a)[1].2, "a bypassed chain shows as not running");
    }

}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// Live = on, in the mix, not soloed away, fader up and its crossfade side up.
    #[test]
    fn a_layer_is_live_only_when_it_shows() {
        let mut c = Composition::default();
        let a = c.add_layer().id;
        let b = c.add_layer().id;
        c.layer_mut(a).unwrap().opacity = 1.0;
        c.layer_mut(b).unwrap().opacity = 1.0;
        assert!(c.layer_live(a));
        c.layer_mut(a).unwrap().opacity = 0.0;
        assert!(!c.layer_live(a), "fader down");
        c.layer_mut(a).unwrap().opacity = 1.0;
        c.layer_mut(b).unwrap().solo = true;
        assert!(!c.layer_live(a) && c.layer_live(b), "soloed away");
        c.layer_mut(b).unwrap().solo = false;
        c.layer_mut(a).unwrap().group = crate::CrossGroup::A;
        c.crossfader = 1.0;
        assert!(!c.layer_live(a), "crossfaded to B");
    }
}
