//! Routing: which outputs show a layer or a group (a "comp") on their own. An output shows a source through a
//! slice (`mapping::Surface`) whose `layer` is that source; `send_to_output` makes one, `outputs_showing` lists
//! them. Every layer / group can go to any number of outputs this way, and stay in (or leave) the main mix.

use super::{Composition, SourceSpec};

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

    /// A slice shows a file of its own (MadMapper's "media on a surface"): a new layer plays it, kept out of the
    /// mix, and the slice shows that layer. Returns the layer's id (None if the slice is gone).
    pub fn show_file_on_slice(&mut self, slice: u64, name: &str, source: SourceSpec) -> Option<u64> {
        self.slice(slice)?;
        let layer = self.add_layer();
        layer.name = name.to_string();
        layer.source = source;
        layer.in_mix = false;
        let id = layer.id;
        self.slice_mut(slice)?.layer = Some(id);
        Some(id)
    }

    /// A new slice on `output` showing a file, centred at `at` (0..1 of the output, y down), a third of the output high
    /// and as wide as the picture's shape (`aspect` = its width / height) needs. Returns the slice id.
    pub fn add_slice_with_file(&mut self, output: u64, at: [f32; 2], aspect: f32, name: &str, source: SourceSpec) -> Option<u64> {
        let canvas = (self.width, self.height);
        let (ow, oh) = self.outputs.iter().find(|o| o.id == output)?.size(canvas);
        let sid = self.new_id();
        let h = 1.0 / 3.0;
        let w = h * aspect.max(0.05) * oh.max(1) as f32 / ow.max(1) as f32;
        let (x0, y0) = (at[0] - w / 2.0, at[1] - h / 2.0);
        let o = self.outputs.iter_mut().find(|o| o.id == output)?;
        o.mapping.enabled = true;
        let s = o.mapping.add_surface(sid);
        s.name = name.to_string();
        s.corners = [[x0, y0], [x0 + w, y0], [x0 + w, y0 + h], [x0, y0 + h]];
        self.show_file_on_slice(sid, name, source)?;
        Some(sid)
    }

    /// Soft edges worked out from overlaps (MadMapper's Auto Setup): every slice showing the same picture gets a fade on
    /// each side where another one's part of the picture (its input area) overlaps it, as wide as the overlap, with
    /// the projector blend curve. Returns how many slices got edges.
    pub fn auto_soft_edges(&mut self) -> usize {
        let all: Vec<(u64, Option<u64>, [f32; 4])> = self.outputs.iter().flat_map(|o| o.mapping.surfaces.iter()).filter(|s| s.line.is_none() && s.model.is_none()).map(|s| (s.id, s.layer, s.input)).collect();
        let mut changed = 0;
        for &(id, src, [x, y, w, h]) in &all {
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            let mut e = [0.0f32; 4];
            for &(oid, osrc, [ox, oy, ow, oh]) in &all {
                if oid == id || osrc != src {
                    continue;
                }
                let x_over = (x + w).min(ox + ow) - x.max(ox);
                let y_over = (y + h).min(oy + oh) - y.max(oy);
                if x_over <= 0.0 || y_over <= 0.0 {
                    continue;
                }
                // Side by side (overlapping mostly along x) or stacked (along y).
                if y_over >= x_over {
                    if ox > x {
                        e[1] = e[1].max(x_over / w);
                    } else if ox + ow < x + w {
                        e[0] = e[0].max(x_over / w);
                    }
                } else if oy > y {
                    e[3] = e[3].max(y_over / h);
                } else if oy + oh < y + h {
                    e[2] = e[2].max(y_over / h);
                }
            }
            if e.iter().any(|v| *v > 0.0)
                && let Some(s) = self.slice_mut(id)
            {
                s.edges = Some(e);
                s.blend_gamma = 2.2;
                changed += 1;
            }
        }
        changed
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

    #[test]
    fn soft_edges_come_from_the_overlaps() {
        let mut c = (crate::TEMPLATES.iter().find(|t| t.0 == "Four projectors").unwrap().2)();
        let want: Vec<Option<[f32; 4]>> = c.outputs.iter().map(|o| o.mapping.surfaces[0].edges).collect();
        for o in &mut c.outputs {
            o.mapping.surfaces[0].edges = None;
        }
        assert_eq!(c.auto_soft_edges(), 4);
        let got: Vec<Option<[f32; 4]>> = c.outputs.iter().map(|o| o.mapping.surfaces[0].edges).collect();
        for (g, w) in got.iter().zip(&want) {
            let (g, w) = (g.unwrap(), w.unwrap());
            assert!(g.iter().zip(&w).all(|(a, b)| (a - b).abs() < 1e-4), "{g:?} vs {w:?}");
        }
    }

    #[test]
    fn a_dropped_file_becomes_a_slice_shaped_like_it() {
        let mut c = Composition { width: 1920, height: 1080, ..Default::default() };
        let out = c.add_output();
        let movie = SourceSpec::Movie { name: "m".into(), path: Some("/x/m.mp4".into()) };
        let sid = c.add_slice_with_file(out, [0.5, 0.5], 16.0 / 9.0, "m", movie.clone()).unwrap();
        let s = c.slice(sid).unwrap();
        let (w, h) = (s.corners[1][0] - s.corners[0][0], s.corners[2][1] - s.corners[1][1]);
        assert!((w * 1920.0 / (h * 1080.0) - 16.0 / 9.0).abs() < 1e-3, "16:9 on the output");
        assert!((s.corners[0][0] + w / 2.0 - 0.5).abs() < 1e-5, "centred where dropped");
        let layer = s.layer.expect("shows its own layer");
        assert_eq!(c.layers.iter().find(|l| l.id == layer).map(|l| (&l.source, l.in_mix)), Some((&movie, false)));
    }

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

#[cfg(test)]
mod file_on_slice_tests {
    use super::*;

    #[test]
    fn a_slice_can_show_a_file_without_it_reaching_the_mix() {
        let mut c = Composition::default();
        c.ensure_outputs();
        let sid = c.new_id();
        c.outputs[0].mapping.add_surface(sid);
        let spec = SourceSpec::Movie { name: "loop.mov".into(), path: Some("/x/loop.mov".into()) };
        let id = c.show_file_on_slice(sid, "loop.mov", spec.clone()).unwrap();
        let l = c.layers.iter().find(|l| l.id == id).unwrap();
        assert!(!l.in_mix && l.source == spec);
        assert_eq!(c.slice(sid).unwrap().layer, Some(id));
        assert!(c.show_file_on_slice(999, "x", SourceSpec::None).is_none());
    }
}
