//! Slice groups (MadMapper's groups): a named set of slices on one output, saved with the project. Groups can sit
//! inside other groups. A group is selected, switched, faded and locked as one; its id works as a slice id for the
//! slice controls (`Mapping::group_slices` lists what it reaches).

use serde::{Deserialize, Serialize};

use super::Mapping;

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SliceGroup {
    pub id: u64,
    pub name: String,
    /// The group it sits in (None = top level).
    pub parent: Option<u64>,
    /// Folded shut in the slice list.
    pub collapsed: bool,
}

impl Mapping {
    pub fn group(&self, id: u64) -> Option<&SliceGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// Puts `slices` into a new group `id`. When they all share a group, the new one goes inside it (nesting).
    /// Returns false (nothing made) when none of them is on this output.
    pub fn group_slices(&mut self, id: u64, slices: &[u64]) -> bool {
        let members: Vec<usize> = (0..self.surfaces.len()).filter(|i| slices.contains(&self.surfaces[*i].id)).collect();
        let Some(first) = members.first() else { return false };
        let shared = self.surfaces[*first].group;
        let parent = shared.filter(|g| members.iter().all(|i| self.surfaces[*i].group == Some(*g)));
        let n = self.groups.len() + 1;
        self.groups.push(SliceGroup { id, name: format!("Group {n}"), parent, collapsed: false });
        for i in members {
            self.surfaces[i].group = Some(id);
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
        for g in self.groups.iter_mut().filter(|g| g.parent == Some(id)) {
            g.parent = parent;
        }
        self.groups.retain(|g| g.id != id);
    }

    /// Moves slices into group `to` (None = out of every group).
    pub fn move_to_group(&mut self, slices: &[u64], to: Option<u64>) {
        let to = to.filter(|g| self.group(*g).is_some());
        for s in self.surfaces.iter_mut().filter(|s| slices.contains(&s.id)) {
            s.group = to;
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

    /// Drops groups with nothing in them (no slices, no groups that have slices) and slices' links to groups that
    /// are gone.
    pub fn prune_groups(&mut self) {
        let ids: Vec<u64> = self.groups.iter().map(|g| g.id).collect();
        for s in &mut self.surfaces {
            if s.group.is_some_and(|g| !ids.contains(&g)) {
                s.group = None;
            }
        }
        let keep: Vec<u64> = ids.iter().copied().filter(|g| !self.group_members(*g).is_empty()).collect();
        self.groups.retain(|g| keep.contains(&g.id));
        for g in &mut self.groups {
            if g.parent.is_some_and(|p| !keep.contains(&p)) {
                g.parent = None;
            }
        }
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
    fn groups_survive_save_and_load() {
        let mut m = three();
        m.group_slices(10, &[1, 3]);
        let back: Mapping = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back.group_members(10), vec![1, 3]);
    }
}
