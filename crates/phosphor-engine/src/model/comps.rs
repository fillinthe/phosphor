//! Comps: canvases of their own inside a project. Each has its own layers (and their groups), size, effects,
//! opacity and blend; the master mixes the comps in order like layers; outputs and other layers can show a comp
//! by its id, like a layer or group.
//!
//! Layers stay in one list (`Composition::layers`) with `LayerState::comp` saying which comp they're in, so ids,
//! controls, cues and slices find them as before. A project without comps has one, "Main", holding every layer;
//! the first comp is also home to any layer that names none. A comp's layers sit next to each other, in comp order
//! (`normalize_comps`), so stacking rules stay inside one comp; a group lives in one comp; solo works inside a comp.
//! Column triggers fire every comp (a mapped button does the same whichever comp is on screen).

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Comp {
    /// From `Composition::new_id` (never clashes with a layer or group).
    pub id: u64,
    pub name: String,
    /// Its own size in pixels; 0 = the master's.
    pub width: u32,
    pub height: u32,
    pub enabled: bool,
    pub opacity: f32,
    pub opacity_mod: Option<Modulation>,
    pub blend: BlendMode,
    /// How a comp of another shape sits in the master.
    pub fit: FitMode,
    /// Run on the comp's finished picture.
    pub effects: Vec<EffectState>,
    pub fx_bypass: bool,
    /// Mixed into the master. Off = only outputs, slices and Feed sources that pick it show it.
    pub in_mix: bool,
    /// Its layers folded away in the Stack panel.
    pub collapsed: bool,
    /// Built as a network of boxes (Designer) rather than a stack: the boxes' layers live here, hidden.
    pub network: Option<Network>,
}

impl Default for Comp {
    fn default() -> Self {
        Self { id: 0, name: "Comp".into(), width: 0, height: 0, enabled: true, opacity: 1.0, opacity_mod: None, blend: BlendMode::Normal, fit: FitMode::Fit, effects: Vec::new(), fx_bypass: false, in_mix: true, collapsed: false, network: None }
    }
}

impl Comp {
    /// Its size, given the master's.
    pub fn size(&self, master: (u32, u32)) -> (u32, u32) {
        if self.width == 0 || self.height == 0 { master } else { (self.width.clamp(16, 8192), self.height.clamp(16, 8192)) }
    }

    /// Mixes into the master exactly like having no comp at all (full size, full opacity, no effects).
    pub fn is_plain(&self) -> bool {
        self.width == 0 && self.enabled && self.in_mix && self.opacity >= 0.999 && self.opacity_mod.is_none() && self.blend == BlendMode::Normal && !self.effects.iter().any(|e| e.enabled)
    }
}

impl Composition {
    pub fn comp(&self, id: u64) -> Option<&Comp> {
        self.comps.iter().find(|c| c.id == id)
    }

    pub fn comp_mut(&mut self, id: u64) -> Option<&mut Comp> {
        self.comps.iter_mut().find(|c| c.id == id)
    }

    /// The comp a layer is in: its own if it still exists, else the first. None when the project has no comps.
    pub fn comp_of(&self, layer: &LayerState) -> Option<u64> {
        layer.comp.filter(|c| self.comp(*c).is_some()).or_else(|| self.comps.first().map(|c| c.id))
    }

    /// The comp the panels show (None when the project has no comps: every layer).
    pub fn open_comp(&self) -> Option<u64> {
        self.open_comp.filter(|c| self.comp(*c).is_some()).or_else(|| self.comps.first().map(|c| c.id))
    }

    /// The layer is in the comp the panels show.
    pub fn in_open_comp(&self, layer: &LayerState) -> bool {
        self.comp_of(layer) == self.open_comp()
    }

    /// The layers of comp `id` (None = every layer of a project without comps), bottom first.
    pub fn comp_layers(&self, id: Option<u64>) -> impl Iterator<Item = &LayerState> {
        self.layers.iter().filter(move |l| self.comp_of(l) == id)
    }

    /// Adds a comp (the master's size) after the others and opens it. A project without comps first gets "Main"
    /// for the layers it has. Returns the new comp's id.
    pub fn add_comp(&mut self, name: &str) -> u64 {
        if self.comps.is_empty() {
            let main = self.new_id();
            self.comps.push(Comp { id: main, name: "Main".into(), ..Default::default() });
            for l in &mut self.layers {
                l.comp = Some(main);
            }
        }
        let id = self.new_id();
        self.comps.push(Comp { id, name: name.to_string(), ..Default::default() });
        self.open_comp = Some(id);
        id
    }

    /// Removes a comp; its layers move to the first remaining comp (nothing is lost). The last comp stays.
    pub fn remove_comp(&mut self, id: u64) {
        if self.comps.len() <= 1 || self.comp(id).is_none() {
            return;
        }
        self.comps.retain(|c| c.id != id);
        let home = self.comps[0].id;
        for l in self.layers.iter_mut().filter(|l| l.comp == Some(id)) {
            l.comp = Some(home);
        }
        if self.open_comp == Some(id) {
            self.open_comp = Some(home);
        }
        self.normalize_groups();
    }

    /// Moves a layer into another comp (on top of its stack), out of any group.
    pub fn move_to_comp(&mut self, layer: u64, comp: u64) {
        if self.comp(comp).is_none() {
            return;
        }
        let Some(i) = self.layers.iter().position(|l| l.id == layer) else { return };
        let mut l = self.layers.remove(i);
        l.comp = Some(comp);
        l.parent = None;
        let at = self.layers.iter().rposition(|x| self.comp_of(x) == Some(comp)).map_or(self.layers.len(), |p| p + 1);
        self.layers.insert(at, l);
        self.normalize_groups();
    }

    /// Moves a comp one place earlier or later in the master's mix (earlier = further down).
    pub fn move_comp(&mut self, id: u64, later: bool) {
        let Some(i) = self.comps.iter().position(|c| c.id == id) else { return };
        let j = if later { i + 1 } else { i.wrapping_sub(1) };
        if j < self.comps.len() {
            self.comps.swap(i, j);
            self.normalize_comps();
        }
    }

    /// Keeps each comp's layers together, in comp order (their own order kept): stacking stays inside a comp.
    pub(crate) fn normalize_comps(&mut self) {
        if self.comps.len() < 2 {
            return;
        }
        let order: Vec<u64> = self.comps.iter().map(|c| c.id).collect();
        let first = order[0];
        let key = |l: &LayerState| order.iter().position(|c| Some(*c) == l.comp).unwrap_or(0);
        let mut layers = std::mem::take(&mut self.layers);
        // Layers naming a gone comp belong to the first.
        for l in &mut layers {
            if !order.iter().any(|c| Some(*c) == l.comp) {
                l.comp = Some(first);
            }
        }
        layers.sort_by_key(key);
        self.layers = layers;
    }

    /// A layer's neighbour at stack position `j` is in the same comp (stacking moves stop at a comp's edge).
    pub(crate) fn same_comp(&self, i: usize, j: usize) -> bool {
        match (self.layers.get(i), self.layers.get(j)) {
            (Some(a), Some(b)) => self.comp_of(a) == self.comp_of(b),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_without_comps_has_one_and_new_ones_keep_their_layers_apart() {
        let mut c = Composition::default();
        let a = c.add_layer().id;
        let b = c.add_layer().id;
        assert_eq!(c.open_comp(), None);
        assert!(c.layers.iter().all(|l| c.in_open_comp(l)), "no comps: every layer is shown");
        let side = c.add_comp("Side wall");
        let main = c.comps[0].id;
        assert_eq!(c.comps[0].name, "Main");
        assert_eq!(c.open_comp(), Some(side), "a new comp opens");
        let x = c.add_layer().id;
        assert_eq!(c.comp_layers(Some(side)).map(|l| l.id).collect::<Vec<_>>(), [x]);
        assert_eq!(c.comp_layers(Some(main)).map(|l| l.id).collect::<Vec<_>>(), [a, b]);
        assert_eq!(c.layers.iter().find(|l| l.id == x).unwrap().name, "Layer 1", "numbered within its comp");
        // Back in Main, a new layer goes on Main's stack (before Side wall's layers).
        c.open_comp = Some(main);
        let y = c.add_layer().id;
        assert_eq!(c.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [a, b, y, x]);
        // Moving up stops at the comp's edge.
        c.move_layer(y, true);
        assert_eq!(c.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [a, b, y, x]);
        // A layer can't be grouped with one in another comp.
        assert_eq!(c.create_group(&[x, b]).map(|g| c.layers.iter().filter(|l| l.parent == Some(g)).count()), Some(1));
        c.ungroup(c.groups[0].id);
        // Solo stays inside its comp.
        c.layer_mut(x).unwrap().solo = true;
        assert_eq!(c.sound_gain(&c.layers.iter().find(|l| l.id == a).unwrap().clone()) > 0.0, c.layers.iter().find(|l| l.id == a).unwrap().volume > 0.0);
        c.layer_mut(x).unwrap().solo = false;
        // Moving a layer across, and removing a comp keeps its layers.
        c.move_to_comp(a, side);
        assert_eq!(c.comp_layers(Some(side)).map(|l| l.id).collect::<Vec<_>>(), [x, a]);
        c.remove_comp(side);
        assert_eq!(c.comps.len(), 1);
        assert_eq!(c.comp_layers(Some(main)).count(), 4, "nothing lost");
        c.remove_comp(main);
        assert_eq!(c.comps.len(), 1, "the last comp stays");
    }

    #[test]
    fn comps_survive_a_save_and_old_projects_load_as_one() {
        let mut c = Composition::default();
        c.add_layer();
        let side = c.add_comp("Side");
        c.comp_mut(side).unwrap().width = 1024;
        c.comp_mut(side).unwrap().height = 768;
        let back = Composition::from_json(&c.to_json()).unwrap();
        assert_eq!(back.comps, c.comps);
        assert_eq!(back.comp(side).unwrap().size((1920, 1080)), (1024, 768));
        let old = Composition::from_json(r#"{"layers":[{"id":1,"name":"Old"}]}"#).unwrap();
        assert!(old.comps.is_empty() && old.open_comp().is_none());
        assert!(Comp::default().is_plain());
    }

    /// An effect added to a comp or a group goes on that chain (it runs over everything under it): both are
    /// targets with an effect chain, like a layer, and the comp's lives apart from its layers'.
    #[test]
    fn a_comp_and_a_group_have_effect_chains_of_their_own() {
        let mut c = Composition::default();
        let a = c.add_layer().id;
        let b = c.add_layer().id;
        let g = c.create_group(&[a, b]).expect("a group");
        let comp = c.add_comp("Side");
        for target in [g, comp] {
            let fx = c.new_effect("invert").or_else(|| c.new_effect("blur")).expect("some effect exists");
            assert!(c.effects(target).is_some(), "{target} has a chain");
            c.effects_mut(target).unwrap().push(fx);
            assert_eq!(c.effects(target).unwrap().len(), 1);
        }
        assert!(c.layers.iter().all(|l| l.effects.is_empty()), "the layers underneath are untouched");
    }
}