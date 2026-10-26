//! Slice groups (MadMapper's groups): a named set of slices on one output, saved with the project. Groups can sit
//! inside other groups. A group is selected, switched, faded and locked as one; its id works as a slice id for the
//! slice controls (`Mapping::group_slices` lists what it reaches).

use serde::{Deserialize, Serialize};

use super::{Mapping, Mask, Surface};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SliceGroup {
    pub id: u64,
    pub name: String,
    /// The group it sits in (None = top level).
    pub parent: Option<u64>,
    /// Folded shut in the slice list.
    pub collapsed: bool,
    /// Off = everything in it (slices, masks, groups) is hidden.
    pub enabled: bool,
    /// Fades everything in it (on top of each slice's / mask's own opacity).
    pub opacity: f32,
    /// A layout: one whole arrangement of this output (MadMapper's scenes), switched to or faded between.
    pub layout: bool,
}

impl Default for SliceGroup {
    fn default() -> Self {
        Self { id: 0, name: String::new(), parent: None, collapsed: false, enabled: true, opacity: 1.0, layout: false }
    }
}

impl Mapping {
    pub fn group(&self, id: u64) -> Option<&SliceGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// Puts `slices` (and masks, by id) into a new group `id`. When they all share a group, the new one goes inside it (nesting).
    /// Returns false (nothing made) when none of them is on this output.
    pub fn group_slices(&mut self, id: u64, slices: &[u64]) -> bool {
        let members: Vec<usize> = (0..self.surfaces.len()).filter(|i| slices.contains(&self.surfaces[*i].id)).collect();
        // Where each picked slice / mask sits now: they nest inside a group they all share.
        let now: Vec<Option<u64>> = members.iter().map(|i| self.surfaces[*i].group).chain(self.masks.iter().filter(|m| slices.contains(&m.id)).map(|m| m.group)).collect();
        let Some(shared) = now.first().copied() else { return false };
        let parent = shared.filter(|g| now.iter().all(|x| *x == Some(*g)));
        let n = self.groups.len() + 1;
        self.groups.push(SliceGroup { id, name: format!("Group {n}"), parent, ..Default::default() });
        for i in members {
            self.surfaces[i].group = Some(id);
        }
        // Masks picked with them go in too.
        for m in self.masks.iter_mut().filter(|m| slices.contains(&m.id)) {
            m.group = Some(id);
        }
        self.prune_groups();
        true
    }

    /// Takes group `id` apart: its slices and groups move up to where it was.
    pub fn ungroup(&mut self, id: u64) {
        let Some(parent) = self.group(id).map(|g| g.parent) else { return };
        for s in self.surfaces.iter_mut().filter(|s| s.group == Some(id)) {
            s.group = parent;
        }
        for m in self.masks.iter_mut().filter(|m| m.group == Some(id)) {
            m.group = parent;
        }
        for g in self.groups.iter_mut().filter(|g| g.parent == Some(id)) {
            g.parent = parent;
        }
        self.groups.retain(|g| g.id != id);
    }

    /// Moves slices and masks into group `to` (None = out of every group).
    pub fn move_to_group(&mut self, slices: &[u64], to: Option<u64>) {
        let to = to.filter(|g| self.group(*g).is_some());
        for s in self.surfaces.iter_mut().filter(|s| slices.contains(&s.id)) {
            s.group = to;
        }
        for m in self.masks.iter_mut().filter(|m| slices.contains(&m.id)) {
            m.group = to;
        }
        self.prune_groups();
    }

    /// Moves group `id` inside `to` (None = top level). Refused if `to` is `id` or inside it.
    pub fn move_group(&mut self, id: u64, to: Option<u64>) -> bool {
        if to.is_some_and(|t| t == id || self.inside(t, id)) {
            return false;
        }
        match self.groups.iter_mut().find(|g| g.id == id) {
            Some(g) => {
                g.parent = to;
                true
            }
            None => false,
        }
    }

    /// Is group `id` somewhere inside group `outer`?
    pub fn inside(&self, id: u64, outer: u64) -> bool {
        let mut at = self.group(id).and_then(|g| g.parent);
        let mut hops = 0;
        while let Some(g) = at {
            if g == outer {
                return true;
            }
            hops += 1;
            if hops > self.groups.len() {
                return false;
            }
            at = self.group(g).and_then(|g| g.parent);
        }
        false
    }

    /// Every slice in group `id` and the groups inside it, in drawing order. A slice id gives just that slice.
    pub fn group_members(&self, id: u64) -> Vec<u64> {
        if self.group(id).is_none() {
            return self.surfaces.iter().filter(|s| s.id == id).map(|s| s.id).collect();
        }
        self.surfaces.iter().filter(|s| s.group.is_some_and(|g| g == id || self.inside(g, id))).map(|s| s.id).collect()
    }

    /// How many groups deep group `id` sits (0 = top level).
    pub fn group_depth(&self, id: u64) -> usize {
        let mut depth = 0;
        let mut at = self.group(id).and_then(|g| g.parent);
        while let Some(g) = at {
            depth += 1;
            if depth > self.groups.len() {
                break;
            }
            at = self.group(g).and_then(|g| g.parent);
        }
        depth
    }

    /// Drops groups with nothing in them (no slices, masks, or groups that have some; layouts stay even empty) and
    /// links to groups that are gone.
    pub fn prune_groups(&mut self) {
        let ids: Vec<u64> = self.groups.iter().map(|g| g.id).collect();
        for s in &mut self.surfaces {
            if s.group.is_some_and(|g| !ids.contains(&g)) {
                s.group = None;
            }
        }
        for m in &mut self.masks {
            if m.group.is_some_and(|g| !ids.contains(&g)) {
                m.group = None;
            }
        }
        let has = |m: &Mapping, g: u64| {
            m.group(g).is_some_and(|x| x.layout)
                || m.surfaces.iter().any(|s| s.group.is_some_and(|x| x == g || m.inside(x, g)))
                || m.masks.iter().any(|k| k.group.is_some_and(|x| x == g || m.inside(x, g)))
        };
        let keep: Vec<u64> = ids.iter().copied().filter(|g| has(self, *g)).collect();
        self.groups.retain(|g| keep.contains(&g.id));
        for g in &mut self.groups {
            if g.parent.is_some_and(|p| !keep.contains(&p)) {
                g.parent = None;
            }
        }
    }

    /// Whether what sits in `group` shows, and how much: every group up the chain on, their opacities multiplied.
    pub fn group_mix(&self, group: Option<u64>) -> (bool, f32) {
        let (mut on, mut opacity) = (true, 1.0);
        let mut at = group;
        let mut hops = 0;
        while let Some(g) = at.and_then(|id| self.group(id)) {
            on &= g.enabled;
            opacity *= g.opacity.clamp(0.0, 1.0);
            at = g.parent;
            hops += 1;
            if hops > self.groups.len() {
                break;
            }
        }
        (on && opacity > 0.0, opacity)
    }

    /// This output's layouts, in order.
    pub fn layouts(&self) -> impl Iterator<Item = &SliceGroup> + '_ {
        self.groups.iter().filter(|g| g.layout && g.parent.is_none())
    }

    /// A new, empty layout `id`. The first one made takes in everything that's on the output now (so it becomes
    /// "Layout 1" and the next starts empty).
    pub fn add_layout(&mut self, id: u64) {
        let first = self.layouts().next().is_none();
        let n = self.layouts().count() + 1;
        self.groups.push(SliceGroup { id, name: format!("Layout {n}"), layout: true, opacity: if first { 1.0 } else { 0.0 }, ..Default::default() });
        if first {
            for s in self.surfaces.iter_mut().filter(|s| s.group.is_none()) {
                s.group = Some(id);
            }
            for m in self.masks.iter_mut().filter(|m| m.group.is_none()) {
                m.group = Some(id);
            }
            for g in self.groups.iter_mut().filter(|g| g.parent.is_none() && g.id != id) {
                g.parent = Some(id);
            }
        }
    }

    /// The layout something sits in (its top-level group, if that's a layout).
    pub fn layout_of(&self, group: Option<u64>) -> Option<u64> {
        let mut at = group?;
        let mut hops = 0;
        while let Some(p) = self.group(at).and_then(|g| g.parent) {
            at = p;
            hops += 1;
            if hops > self.groups.len() {
                return None;
            }
        }
        self.group(at).filter(|g| g.layout).map(|g| g.id)
    }

    /// The layout showing most now (where new slices and masks go).
    pub fn current_layout(&self) -> Option<u64> {
        self.layouts().filter(|g| g.enabled).max_by(|a, b| a.opacity.total_cmp(&b.opacity)).or_else(|| self.layouts().next()).map(|g| g.id)
    }

    /// A copy of group `id` with everything in it (slices, masks, groups inside), each with a fresh id from
    /// `new_id`. Masks attached to a copied slice follow the copy. Returns the copy's id.
    pub fn duplicate_group(&mut self, id: u64, new_id: &mut impl FnMut() -> u64) -> Option<u64> {
        let top = self.group(id)?.clone();
        // Old → new ids for the group and every group inside it.
        let mut ids = std::collections::HashMap::new();
        let inner: Vec<SliceGroup> = self.groups.iter().filter(|g| g.id == id || self.inside(g.id, id)).cloned().collect();
        for g in &inner {
            ids.insert(g.id, new_id());
        }
        for g in inner {
            let parent = if g.id == id { g.parent } else { g.parent.map(|p| ids[&p]) };
            let name = if g.id == id { format!("{} copy", top.name) } else { g.name.clone() };
            self.groups.push(SliceGroup { id: ids[&g.id], parent, name, ..g });
        }
        let slices: Vec<Surface> = self.surfaces.iter().filter(|s| s.group.is_some_and(|g| ids.contains_key(&g))).cloned().collect();
        let mut slice_ids = std::collections::HashMap::new();
        for mut s in slices {
            let fresh = new_id();
            slice_ids.insert(s.id, fresh);
            s.id = fresh;
            s.group = s.group.map(|g| ids[&g]);
            self.surfaces.push(s);
        }
        let masks: Vec<Mask> = self.masks.iter().filter(|m| m.group.is_some_and(|g| ids.contains_key(&g))).cloned().collect();
        for mut m in masks {
            m.id = new_id();
            m.group = m.group.map(|g| ids[&g]);
            m.attached = m.attached.map(|a| slice_ids.get(&a).copied().unwrap_or(a));
            self.masks.push(m);
        }
        Some(ids[&id])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> Mapping {
        let mut m = Mapping::default();
        for id in 1..=3 {
            m.add_surface(id);
        }
        m
    }

    #[test]
    fn groups_nest_and_come_apart() {
        let mut m = three();
        assert!(m.group_slices(10, &[1, 2]));
        assert_eq!(m.group_members(10), vec![1, 2]);
        assert!(m.group_slices(11, &[2]), "a group inside a group");
        assert_eq!(m.group(11).unwrap().parent, Some(10));
        assert_eq!(m.group_depth(11), 1);
        assert_eq!(m.group_members(10), vec![1, 2], "the outer group still reaches the inner one's slices");
        assert_eq!(m.group_members(11), vec![2]);
        assert_eq!(m.group_members(3), vec![3], "a slice id gives the slice");
        m.ungroup(10);
        assert_eq!(m.group(11).unwrap().parent, None, "moved up");
        assert_eq!(m.surfaces[0].group, None);
        assert_eq!(m.surfaces[1].group, Some(11));
    }

    #[test]
    fn no_loops_and_empty_groups_go() {
        let mut m = three();
        m.group_slices(10, &[1, 2]);
        m.group_slices(11, &[1]);
        assert!(!m.move_group(10, Some(11)), "a group can't go inside its own child");
        assert!(!m.move_group(10, Some(10)));
        m.move_to_group(&[1], None);
        assert!(m.group(11).is_none(), "nothing left in it");
        m.remove_slices(|s| s.id == 2);
        assert!(m.groups.is_empty());
        assert!(!m.group_slices(12, &[99]), "not on this output");
    }

    #[test]
    fn only_selected_shows_just_those() {
        let mut m = three();
        m.enabled = true;
        m.solo = vec![2];
        assert_eq!(m.shown().map(|s| s.id).collect::<Vec<_>>(), vec![2]);
        m.solo.clear();
        assert_eq!(m.shown().count(), 3);
        m.solo = vec![2];
        let back: Mapping = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert!(back.solo.is_empty(), "not saved");
    }

    #[test]
    fn groups_survive_save_and_load() {
        let mut m = three();
        m.group_slices(10, &[1, 3]);
        let back: Mapping = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back.group_members(10), vec![1, 3]);
    }

    #[test]
    fn groups_fade_and_hide_slices_and_masks() {
        let mut m = three();
        m.enabled = true;
        m.add_mask(20);
        m.group_slices(10, &[1, 20]);
        assert_eq!(m.masks[0].group, Some(10), "a mask picked with the slices joins too");
        m.groups[0].opacity = 0.5;
        let shown = Mapping::animated(std::borrow::Cow::Borrowed(&m), &Default::default(), 1.0);
        assert_eq!(shown.surfaces[0].opacity, 0.5, "faded by its group");
        assert_eq!(shown.surfaces[1].opacity, 1.0, "the others aren't");
        assert_eq!(m.placed_masks()[0].opacity, 0.5);
        m.groups[0].enabled = false;
        assert_eq!(m.shown().map(|s| s.id).collect::<Vec<_>>(), vec![2, 3], "a group that's off hides its slices");
        assert!(m.placed_masks().is_empty(), "and its masks");
        let mask = Mask { opacity: 0.25, ..Default::default() };
        assert!(mask.triangles().iter().all(|v| v[2] <= 0.25), "a mask's opacity is how dark it makes things");
    }

    #[test]
    fn layouts_switch_between_whole_arrangements() {
        let mut m = three();
        m.add_mask(20);
        m.add_layout(100);
        assert!(m.surfaces.iter().all(|s| s.group == Some(100)) && m.masks[0].group == Some(100), "the first takes in what's there");
        assert_eq!(m.group(100).unwrap().opacity, 1.0);
        m.add_layout(101);
        assert_eq!(m.group(101).unwrap().opacity, 0.0, "the next starts empty and faded out");
        assert!(m.group(101).is_some(), "an empty layout isn't tidied away");
        m.prune_groups();
        assert!(m.group(101).is_some());
        assert_eq!(m.current_layout(), Some(100));
        let mut next = 500;
        let copy = m.duplicate_group(100, &mut || {
            next += 1;
            next
        })
        .unwrap();
        assert_eq!(m.surfaces.len(), 6, "every slice copied");
        assert_eq!(m.masks.len(), 2);
        assert!(m.group(copy).unwrap().layout && m.group(copy).unwrap().name.ends_with("copy"));
        assert_eq!(m.layout_of(m.surfaces[4].group), Some(copy));
        assert_eq!(m.layouts().count(), 3);
    }

    #[test]
    fn older_groups_open_shown() {
        let g: SliceGroup = serde_json::from_str(r#"{"id":1,"name":"a","parent":null,"collapsed":false}"#).unwrap();
        assert!(g.enabled && g.opacity == 1.0 && !g.layout);
        let m: Mask = serde_json::from_str(r#"{"id":2}"#).unwrap();
        assert!(m.opacity == 1.0 && m.group.is_none());
    }
}
