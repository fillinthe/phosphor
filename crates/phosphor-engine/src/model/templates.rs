//! Ready-made starting points for a new project (File › New from Template): a few common set-ups, built in code.

use super::*;

/// (name, what it's for, the project).
pub const TEMPLATES: &[(&str, &str, fn() -> Composition)] = &[
    ("Starter pack", "Five ready-made looks, each a comp of its own: press Only on a comp to show it, then change anything", starter_pack),
    ("Generators", "Two moving generators blended: the usual start", Composition::starter),
    ("Clip player", "One layer of clips to fill with movies; each plays and the next follows by itself", clip_player),
    ("A / B mixer", "Two layers on the crossfader (A and B), Auto fades between them", ab_mixer),
    ("Four projectors", "One wide picture across four outputs, side by side, with soft-edge blends where they overlap", four_projectors),
    ("LED wall", "A 32 × 18 LED matrix over the whole picture, sent as Art-Net", led_wall),
];

/// A setting swept across part of its travel (`min`..`max` as fractions of it), `rate` cycles a second.
fn sweep(source: ModSource, rate: f32, min: f32, max: f32) -> Modulation {
    Modulation { source, rate, sync: false, range: true, min, max, amount: 1.0, ..Default::default() }
}

/// Sets a shader's settings by name and puts sweeps on some: the values, and the modulations to go with them.
fn tuned(desc: &crate::shaders::ShaderDesc, set: &[(&str, f32)], move_: &[(&str, Modulation)]) -> (Vec<f32>, Vec<ParamMod>) {
    let at = |name: &str| desc.params.iter().position(|p| p.name == name);
    let mut values = desc.defaults();
    for (name, v) in set {
        if let Some(i) = at(name) {
            values[i] = *v;
        }
    }
    let mods = move_.iter().filter_map(|(name, m)| at(name).map(|index| ParamMod { index, m: *m })).collect();
    (values, mods)
}

impl LayerState {
    /// The layer plays generator `id` with these settings and sweeps.
    fn play(&mut self, name: &str, id: &str, set: &[(&str, f32)], move_: &[(&str, Modulation)]) -> &mut Self {
        self.name = name.into();
        self.set_generator(id);
        if let Some(d) = crate::shaders::generator(id) {
            (self.generator_values, self.generator_mods) = tuned(d, set, move_);
        }
        self
    }
}

impl Composition {
    /// Effect `id` with these settings and sweeps (None for an effect that doesn't exist).
    fn tuned_effect(&mut self, id: &str, set: &[(&str, f32)], move_: &[(&str, Modulation)]) -> Option<EffectState> {
        let mut fx = self.new_effect(id)?;
        if let Some(d) = crate::shaders::effect(id) {
            (fx.values, fx.mods) = tuned(d, set, move_);
        }
        Some(fx)
    }

    /// A new comp after the others (no "Main" made for it), open so new layers land in it.
    fn pack_comp(&mut self, name: &str, on: bool) -> u64 {
        let id = self.new_id();
        self.comps.push(Comp { id, name: name.into(), enabled: on, ..Default::default() });
        self.open_comp = Some(id);
        id
    }
}

/// Five looks to click through, built only from what ships in the app (no files): each a comp, the first one on.
fn starter_pack() -> Composition {
    use ModSource::{AudioLevel, AudioLow, LfoSaw, LfoSine};
    let mut c = Composition::default();

    // 1. Aurora: slow plasma through a turning kaleidoscope, glowing.
    let aurora = c.pack_comp("1 Aurora", true);
    c.add_layer().play("Plasma", "plasma", &[("Speed", 0.8), ("Scale", 12.0)], &[("Hue", sweep(LfoSaw, 0.03, 0.0, 1.0))]);
    let fx = [c.tuned_effect("kaleidoscope", &[("Segments", 10.0), ("Zoom", 0.6)], &[("Rotation", sweep(LfoSaw, 0.02, 0.0, 1.0))]), c.tuned_effect("edge_glow", &[("Intensity", 0.7)], &[])];
    c.comp_mut(aurora).expect("just made").effects = fx.into_iter().flatten().collect();

    // 2. Hyperspace: a tunnel with stars flying out of it, the colors pulling apart now and then.
    let space = c.pack_comp("2 Hyperspace", false);
    c.add_layer().play("Tunnel", "tunnel", &[("Speed", 1.2), ("Segments", 12.0)], &[("Hue", sweep(LfoSaw, 0.05, 0.0, 1.0))]);
    c.add_layer().play("Stars", "starfield", &[("Density", 14.0), ("Speed", 3.0)], &[]).blend = BlendMode::Add;
    let fx = [c.tuned_effect("rgb_shift", &[], &[("Amount", sweep(LfoSine, 0.2, 0.0, 0.25))])];
    c.comp_mut(space).expect("just made").effects = fx.into_iter().flatten().collect();

    // 3. Neon: wavy lines mirrored into quarters, edges lit, leaving trails.
    let neon = c.pack_comp("3 Neon", false);
    c.add_layer().play("Lines", "lines", &[("Lines", 8.0), ("Amplitude", 0.3)], &[("Frequency", sweep(LfoSine, 0.07, 0.1, 0.6))]);
    let fx = [
        c.tuned_effect("mirror", &[("Mode", 2.0)], &[]),
        c.tuned_effect("color_adjust", &[("Saturation", 1.6)], &[("Hue", sweep(LfoSaw, 0.04, 0.0, 1.0))]),
        c.tuned_effect("trails", &[("Decay", 0.93), ("Zoom", 1.01)], &[]),
        c.tuned_effect("glow", &[("Intensity", 1.2), ("Threshold", 0.4)], &[]),
    ];
    c.comp_mut(neon).expect("just made").effects = fx.into_iter().flatten().collect();

    // 4. Sound: cells that swell with the bass under the spectrum and the wave of whatever is playing.
    let sound = c.pack_comp("4 Sound", false);
    c.add_layer().play("Cells", "voronoi", &[("Speed", 0.4)], &[("Hue", sweep(LfoSaw, 0.02, 0.0, 1.0)), ("Scale", sweep(AudioLow, 1.0, 0.15, 0.5))]).opacity = 0.8;
    c.add_layer().play("Spectrum", "spectrum", &[("Bars", 64.0), ("Mirror", 1.0), ("Height", 1.6)], &[("Hue", sweep(LfoSaw, 0.03, 0.0, 1.0))]).blend = BlendMode::Add;
    c.add_layer().play("Wave", "waveform", &[("Gain", 1.5), ("Thickness", 0.02)], &[("Glow", sweep(AudioLevel, 1.0, 0.3, 1.0))]).blend = BlendMode::Add;
    let fx = [c.tuned_effect("glow", &[("Intensity", 0.9)], &[])];
    c.comp_mut(sound).expect("just made").effects = fx.into_iter().flatten().collect();

    // 5. Bloom: a Designer network. Sparks mixed with their own last frame, zoomed and turned a little each time
    // around, so it blooms outward; a slow number turns the colors.
    let bloom = c.pack_comp("5 Bloom (Designer)", false);
    c.start_network(bloom);
    if let Some(mut net) = c.comp(bloom).and_then(|x| x.network.clone()) {
        let (pic, out) = (net.nodes[0].id, net.nodes[1].id);
        net.disconnect(out, 0);
        let mix = net.add(NodeKind::Mix, [560.0, 40.0]);
        let back = net.add(NodeKind::Feedback, [40.0, 330.0]);
        let turn = net.add(NodeKind::Effect, [300.0, 330.0]);
        let tint = net.add(NodeKind::Effect, [820.0, 40.0]);
        let slow = net.add(NodeKind::Number, [560.0, 400.0]);
        let note = net.add(NodeKind::Note, [820.0, 400.0]);
        if let Some(n) = net.node_mut(out) {
            n.pos = [1080.0, 40.0];
        }
        if let Some(n) = net.node_mut(back) {
            n.target = mix;
        }
        if let Some(n) = net.node_mut(note) {
            n.name = "The Mix feeds back into itself through Zoom / Rotate. Try its Zoom and Rotate, or the Mix's B opacity.".into();
        }
        for (from, to, input) in [(pic, mix, 0), (back, turn, 0), (turn, mix, 1), (mix, tint, 0), (tint, out, 0)] {
            net.connect(from, to, input);
        }
        c.comp_mut(bloom).expect("just made").network = Some(net);
        c.sync_network(bloom);
        let net = c.comp(bloom).and_then(|x| x.network.clone()).expect("just set");
        let part = |n: u32, k: usize| net.node(n).and_then(|n| n.parts.get(k).copied()).unwrap_or(0);
        if let Some(l) = c.layer_mut(part(pic, 0)) {
            l.play("Sparks", "particles", &[("Count", 160.0), ("Gravity", 0.0), ("Spread", 1.0), ("Size", 0.02), ("Emitter Y", 0.5)], &[]);
        }
        for (node, effect) in [(turn, c.tuned_effect("zoom_rotate", &[("Zoom", 1.04), ("Rotate", 0.008), ("Spin", 0.0)], &[])), (tint, c.tuned_effect("color_adjust", &[("Saturation", 1.4)], &[]))] {
            if let (Some(fx), Some(l)) = (effect, c.layer_mut(part(node, 0))) {
                l.effects = vec![fx];
            }
        }
        if let Some(b) = c.layer_mut(part(mix, 2)) {
            b.opacity = 0.94;
            b.blend = BlendMode::Lighten;
        }
        if let Some(m) = c.modulators.iter_mut().find(|m| m.id == part(slow, 0)) {
            m.signal = Modulation { source: LfoSaw, rate: 0.05, ..m.signal };
            m.name = "Slow turn".into();
        }
        // The slow number into the tint's Hue (its first setting, after the picture input).
        c.connect_boxes(bloom, &[], slow, tint, 1);
        c.sync_network(bloom);
    }
    c.open_comp = Some(aurora);
    c
}

fn clip_player() -> Composition {
    let mut c = Composition::default();
    let l = c.add_layer();
    l.name = "Clips".into();
    l.autopilot = Autopilot::Next;
    c
}

fn ab_mixer() -> Composition {
    let mut c = Composition::default();
    for (name, side, generator) in [("A", CrossGroup::A, "plasma"), ("B", CrossGroup::B, "tunnel")] {
        let l = c.add_layer();
        l.name = name.into();
        l.group = side;
        l.set_generator(generator);
    }
    c.crossfader = 0.0;
    c
}

fn four_projectors() -> Composition {
    let mut c = Composition::starter();
    // Each output shows a quarter of the picture plus an overlap that fades out into the next one.
    let overlap = 0.1;
    let width = (1.0 + 3.0 * overlap) / 4.0;
    for k in 0..4 {
        let out = c.add_output();
        let sid = c.new_id();
        let o = c.outputs.iter_mut().find(|o| o.id == out).expect("just added");
        o.name = format!("Projector {}", k + 1);
        let s = o.mapping.add_surface(sid);
        s.name = format!("Projector {}", k + 1);
        s.corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let x = k as f32 * (width - overlap);
        s.input = [x, 0.0, width, 1.0];
        let blend = overlap / width;
        s.edges = Some([if k > 0 { blend } else { 0.0 }, if k < 3 { blend } else { 0.0 }, 0.0, 0.0]);
        s.blend_gamma = 2.2;
    }
    // The first output came from `ensure_outputs` style defaults: keep only the four.
    c.outputs.retain(|o| o.name.starts_with("Projector"));
    c
}

fn led_wall() -> Composition {
    let mut c = Composition::starter();
    let id = c.new_id();
    c.dmx.enabled = true;
    c.dmx.fixtures.push(Fixture {
        id,
        name: "LED wall".into(),
        shape: FixtureShape::Grid,
        points: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        cols: 32,
        rows: 18,
        zigzag: true,
        ..Default::default()
    });
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_build_what_they_say() {
        for (name, _, make) in TEMPLATES {
            let c = make();
            assert!(!c.layers.is_empty(), "{name}: has a layer");
        }
        // The starter pack: five comps, one on, every look made of things that exist, the Designer one wired up.
        let s = starter_pack();
        assert_eq!(s.comps.len(), 5);
        assert_eq!(s.comps.iter().filter(|c| c.enabled).count(), 1);
        for c in &s.comps {
            assert!(s.comp_layers(Some(c.id)).count() > 0, "{}: has layers", c.name);
            assert!(c.network.is_some() || !c.effects.is_empty(), "{}: its effects were found", c.name);
        }
        assert!(s.layers.iter().all(|l| l.designer || matches!(l.source, SourceSpec::Generator(_))), "every layer plays something");
        assert_eq!(s.comps[0].effects.len(), 2);
        assert_eq!(s.comps[2].effects.len(), 4);
        let bloom = &s.comps[4];
        let net = bloom.network.as_ref().unwrap();
        assert!(net.shown().is_some(), "something is wired into Output");
        assert_eq!(net.wires.len(), 6);
        assert_eq!(Composition::from_json(&s.to_json()).unwrap(), s, "it saves and reopens the same");
        let mut s = s;
        let second = s.comps[1].id;
        s.show_only_comp(second);
        assert_eq!(s.comps.iter().filter(|c| c.enabled).map(|c| c.id).collect::<Vec<_>>(), vec![second]);
        assert_eq!(s.open_comp(), Some(second));
        let p = four_projectors();
        assert_eq!(p.outputs.len(), 4);
        let inputs: Vec<[f32; 4]> = p.outputs.iter().map(|o| o.mapping.surfaces[0].input).collect();
        assert!((inputs[3][0] + inputs[3][2] - 1.0).abs() < 1e-5, "the last ends at the right edge: {inputs:?}");
        assert!(inputs[1][0] < inputs[0][0] + inputs[0][2], "neighbors overlap");
        let w = led_wall();
        assert!(w.dmx.enabled && w.dmx.fixtures[0].cols == 32);
        let m = ab_mixer();
        assert_eq!((m.layers[0].group, m.layers[1].group), (CrossGroup::A, CrossGroup::B));
    }
}
