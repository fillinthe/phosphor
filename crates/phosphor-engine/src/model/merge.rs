//! Merging another project into this one (MadMapper's Merge Project): its layers (with their clips and effects), its
//! media, its cues and the controller mappings that drive those layers come in with fresh ids, beside what's here.

use std::collections::HashMap;

use super::*;

/// What a merge brought in.
#[derive(Debug, Default, PartialEq)]
pub struct Merged {
    pub layers: usize,
    pub media: usize,
    pub cues: usize,
    pub mappings: usize,
}

/// A target from the other project with its layer / effect ids swapped for the new ones (None: it points at
/// something that didn't come along, like the other project's slices or surface widgets).
fn remap_target(t: &ControlTarget, ids: &HashMap<u64, u64>) -> Option<ControlTarget> {
    let mut v = serde_json::to_value(t).ok()?;
    match &mut v {
        // Unit variants (MasterFade) and app controls come along as they are.
        serde_json::Value::String(_) => {}
        serde_json::Value::Object(o) => {
            let (name, inner) = o.iter_mut().next()?;
            match inner {
                serde_json::Value::Number(n) if name != "TriggerColumn" => *inner = (*ids.get(&n.as_u64()?)?).into(),
                serde_json::Value::Object(fields) => {
                    for (k, f) in fields.iter_mut() {
                        match k.as_str() {
                            "layer" | "effect" => *f = (*ids.get(&f.as_u64()?)?).into(),
                            "fixture" => return None,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        _ => return None,
    }
    serde_json::from_value(v).ok()
}

impl Composition {
    /// Brings `other`'s layers, media, cues and their mappings in on top of this project's (the open comp).
    pub fn merge(&mut self, other: Composition) -> Merged {
        let mut ids: HashMap<u64, u64> = HashMap::new();
        let mut out = Merged::default();
        for mut l in other.layers {
            let old = l.id;
            let old_fx: Vec<u64> = l.effects.iter().map(|e| e.id).collect();
            // Groups and comps stay behind: the layer joins the open comp, ungrouped.
            (l.parent, l.comp) = (None, self.open_comp);
            let l = self.adopt_layer(l);
            ids.insert(old, l.id);
            for (o, n) in old_fx.iter().zip(l.effects.iter().map(|e| e.id)) {
                ids.insert(*o, n);
            }
            self.layers.push(l);
            out.layers += 1;
        }
        for m in other.media_bin {
            if self.add_media(&m.name, &m.path) {
                out.media += 1;
            }
        }
        for mut c in other.cues.cues {
            c.id = self.new_id();
            c.slices.clear();
            c.layers = c
                .layers
                .into_iter()
                .filter_map(|mut cl| {
                    cl.id = *ids.get(&cl.id)?;
                    cl.effects = cl.effects.into_iter().filter_map(|(id, on, mix, vals)| Some((*ids.get(&id)?, on, mix, vals))).collect();
                    Some(cl)
                })
                .collect();
            self.cues.cues.push(c);
            out.cues += 1;
        }
        for mut m in other.control_maps {
            let Some(t) = remap_target(&m.target, &ids) else { continue };
            m.target = t;
            if !self.control_maps.contains(&m) {
                self.control_maps.push(m);
                out.mappings += 1;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_merges_in_with_fresh_ids() {
        let mut other = Composition::default();
        let a = other.add_layer().id;
        let fx = other.new_effect("invert").unwrap();
        let fid = fx.id;
        other.layers[0].effects.push(fx);
        other.add_media("a.mp4", "/x/a.mp4");
        other.control_maps.push(ControlMap::new(ControlSource::MidiCc { channel: 0, cc: 1 }, ControlTarget::LayerOpacity(a)));
        other.control_maps.push(ControlMap::new(ControlSource::MidiCc { channel: 0, cc: 2 }, ControlTarget::EffectParam { effect: fid, index: 0 }));
        other.control_maps.push(ControlMap::new(ControlSource::MidiCc { channel: 0, cc: 3 }, ControlTarget::TriggerColumn(2)));
        other.control_maps.push(ControlMap::new(ControlSource::MidiCc { channel: 0, cc: 4 }, ControlTarget::SliceOpacity(77)));
        let rec = other.record_cue("A".into());
        other.cues.cues.push(rec);

        let mut here = Composition::default();
        here.add_layer();
        let got = here.merge(other);
        assert_eq!(got, Merged { layers: 1, media: 1, cues: 1, mappings: 3 }, "the slice mapping stays behind");
        let new_layer = here.layers[1].id;
        let new_fx = here.layers[1].effects[0].id;
        assert_ne!(new_layer, a, "fresh id");
        assert!(here.control_maps.iter().any(|m| m.target == ControlTarget::LayerOpacity(new_layer)));
        assert!(here.control_maps.iter().any(|m| m.target == ControlTarget::EffectParam { effect: new_fx, index: 0 }));
        assert!(here.control_maps.iter().any(|m| m.target == ControlTarget::TriggerColumn(2)), "a column number is not an id");
        assert_eq!(here.cues.cues[0].layers[0].id, new_layer);
    }
}
