//! When a shader's parameters change (a patch gains or loses a block, a user shader is edited), everything already
//! using it keeps its settings by name: values, modulations and controller mappings follow their parameter to its
//! new place, new parameters start at their default and removed ones are let go.

use super::*;

impl Composition {
    /// Shader `id` had parameters `old` (names, in order) and now has `new` (name, default): move every effect,
    /// generator layer, clip and MIDI/OSC mapping that uses it over by name.
    pub fn remap_params(&mut self, id: &str, old: &[&str], new: &[(&str, f32)]) {
        if old.len() == new.len() && old.iter().zip(new).all(|(a, (b, _))| a == b) {
            return;
        }
        // Old index → new index.
        let to: Vec<Option<usize>> = old.iter().map(|n| new.iter().position(|(m, _)| m == n)).collect();
        let values = |v: &[f32]| -> Vec<f32> { new.iter().map(|(n, d)| old.iter().position(|o| o == n).and_then(|i| v.get(i)).copied().unwrap_or(*d)).collect() };
        let mods = |m: &mut Vec<ParamMod>| {
            m.retain_mut(|p| match to.get(p.index).copied().flatten() {
                Some(j) => {
                    p.index = j;
                    true
                }
                None => false,
            })
        };
        let mut effects: Vec<u64> = Vec::new();
        for e in self.all_effects_mut().filter(|e| e.effect == id) {
            e.values = values(&e.values);
            mods(&mut e.mods);
            effects.push(e.id);
        }
        let mut layers: Vec<u64> = Vec::new();
        for l in &mut self.layers {
            if matches!(&l.source, SourceSpec::Generator(g) if g == id) {
                l.generator_values = values(&l.generator_values);
                mods(&mut l.generator_mods);
                layers.push(l.id);
            }
            for c in l.clips.iter_mut().filter(|c| matches!(&c.source, SourceSpec::Generator(g) if g == id)) {
                c.generator_values = values(&c.generator_values);
                mods(&mut c.mods);
            }
        }
        self.control_maps.retain_mut(|m| {
            let index = match &mut m.target {
                ControlTarget::EffectParam { effect, index } if effects.contains(effect) => index,
                ControlTarget::GeneratorParam { layer, index } if layers.contains(layer) => index,
                _ => return true,
            };
            match to.get(*index).copied().flatten() {
                Some(j) => {
                    *index = j;
                    true
                }
                None => false,
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_follow_their_parameter_by_name() {
        let mut c = Composition::default();
        let l = c.add_layer().id;
        let fx = EffectState { id: 50, effect: "custom:p".into(), enabled: true, values: vec![0.1, 0.2, 0.3], mix: 1.0, mods: vec![ParamMod { index: 2, m: Modulation::default() }, ParamMod { index: 0, m: Modulation::default() }] };
        c.layer_mut(l).unwrap().effects.push(fx);
        c.control_maps.push(ControlMap::new(ControlSource::MidiCc { channel: 0, cc: 1 }, ControlTarget::EffectParam { effect: 50, index: 2 }));
        c.control_maps.push(ControlMap::new(ControlSource::MidiCc { channel: 0, cc: 1 }, ControlTarget::EffectParam { effect: 50, index: 0 }));
        // "Blur Size" (first) went away; a new "Glow Amount" came in at the end.
        c.remap_params("custom:p", &["Blur Size", "Hue Shift", "Hue Spin"], &[("Hue Shift", 0.0), ("Hue Spin", 0.0), ("Glow Amount", 0.7)]);
        let e = &c.layers[0].effects[0];
        assert_eq!(e.values, vec![0.2, 0.3, 0.7]);
        assert_eq!(e.mods.iter().map(|m| m.index).collect::<Vec<_>>(), vec![1], "Hue Spin's modulation moved, Blur Size's went");
        assert_eq!(c.control_maps.len(), 1);
        assert_eq!(c.control_maps[0].target, ControlTarget::EffectParam { effect: 50, index: 1 });
    }
}
