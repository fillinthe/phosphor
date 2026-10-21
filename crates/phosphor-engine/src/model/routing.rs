//! Routing: which outputs show a layer or a group (a "comp") on their own. An output shows a source through a
//! slice (`mapping::Surface`) whose `layer` is that source; `send_to_output` makes one, `outputs_showing` lists
//! them. Every layer / group can go to any number of outputs this way, and stay in (or leave) the main mix.

use super::Composition;

impl Composition {
    /// Shows `source` (a layer or group id) on output `output`: adds a full-size slice for it (or re-enables the one
    /// that already shows it) and turns the output's mapping on. Returns the slice id, or None for an unknown output.
    pub fn send_to_output(&mut self, source: u64, output: u64) -> Option<u64> {
        let slice_id = self.new_id();
        let o = self.outputs.iter_mut().find(|o| o.id == output)?;
        o.mapping.enabled = true;
        if let Some(s) = o.mapping.surfaces.iter_mut().find(|s| s.layer == Some(source)) {
            s.enabled = true;
            return Some(s.id);
        }
        let s = o.mapping.add_surface(slice_id);
        s.layer = Some(source);
        Some(slice_id)
    }

    /// Stops output `output` showing `source` on its own (its slice goes; the mix stays as it was).
    pub fn remove_from_output(&mut self, source: u64, output: u64) {
        if let Some(o) = self.outputs.iter_mut().find(|o| o.id == output) {
            o.mapping.remove_slices(|s| s.layer == Some(source));
        }
    }

    /// A mapping slice by id, on whichever output it is.
    pub fn slice(&self, id: u64) -> Option<&crate::mapping::Surface> {
        self.outputs.iter().flat_map(|o| o.mapping.surfaces.iter()).find(|s| s.id == id)
    }

    pub fn slice_mut(&mut self, id: u64) -> Option<&mut crate::mapping::Surface> {
        self.outputs.iter_mut().flat_map(|o| o.mapping.surfaces.iter_mut()).find(|s| s.id == id)
    }

    /// A copy of slice `sid` just after it (a little offset, fresh ids for it and its effects). Returns the copy's id.
    pub fn duplicate_slice(&mut self, sid: u64) -> Option<u64> {
        let mut copy = self.slice(sid)?.clone();
        copy.id = self.new_id();
        copy.name = format!("{} copy", copy.name);
        copy.translate(0.03, 0.03);
        copy.effects = self.adopt_effects(std::mem::take(&mut copy.effects));
        let id = copy.id;
        let mapping = self.outputs.iter_mut().map(|o| &mut o.mapping).find(|m| m.surfaces.iter().any(|s| s.id == sid))?;
        let i = mapping.surfaces.iter().position(|s| s.id == sid)?;
        mapping.surfaces.insert(i + 1, copy);
        Some(id)
    }

    /// The slices a slice control reaches: slice `id`, or every slice in slice group `id` (and the groups inside it).
    pub fn slices_reached(&self, id: u64) -> Vec<u64> {
        self.outputs.iter().flat_map(|o| o.mapping.group_members(id)).collect()
    }

    /// Ids of the outputs with an enabled slice showing `source`.
    pub fn outputs_showing(&self, source: u64) -> Vec<u64> {
        self.outputs
            .iter()
            .filter(|o| o.mapping.enabled && o.mapping.surfaces.iter().any(|s| s.enabled && s.layer == Some(source)))
            .map(|o| o.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_goes_to_an_output_once_and_comes_back() {
        let mut c = Composition::default();
        let layer = c.add_layer().id;
        let out = c.add_output();
        assert!(c.outputs_showing(layer).is_empty());
        let slice = c.send_to_output(layer, out).expect("output exists");
        assert_eq!(c.send_to_output(layer, out), Some(slice), "the same slice again, not a second one");
        assert_eq!(c.outputs_showing(layer), vec![out]);
        assert_eq!(c.outputs.iter().find(|o| o.id == out).unwrap().mapping.surfaces.len(), 1);
        assert!(c.send_to_output(layer, 999).is_none());
        c.remove_from_output(layer, out);
        assert!(c.outputs_showing(layer).is_empty());
    }
}
