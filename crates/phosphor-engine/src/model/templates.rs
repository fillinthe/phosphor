//! Ready-made starting points for a new project (File › New from Template): a few common set-ups, built in code.

use super::*;

/// (name, what it's for, the project).
pub const TEMPLATES: &[(&str, &str, fn() -> Composition)] = &[
    ("Generators", "Two moving generators blended: the usual start", Composition::starter),
    ("Clip player", "One layer of clips to fill with movies; each plays and the next follows by itself", clip_player),
    ("A / B mixer", "Two layers on the crossfader (A and B), Auto fades between them", ab_mixer),
    ("Four projectors", "One wide picture across four outputs, side by side, with soft-edge blends where they overlap", four_projectors),
    ("LED wall", "A 32 × 18 LED matrix over the whole picture, sent as Art-Net", led_wall),
];

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
        let p = four_projectors();
        assert_eq!(p.outputs.len(), 4);
        let inputs: Vec<[f32; 4]> = p.outputs.iter().map(|o| o.mapping.surfaces[0].input).collect();
        assert!((inputs[3][0] + inputs[3][2] - 1.0).abs() < 1e-5, "the last ends at the right edge: {inputs:?}");
        assert!(inputs[1][0] < inputs[0][0] + inputs[0][2], "neighbours overlap");
        let w = led_wall();
        assert!(w.dmx.enabled && w.dmx.fixtures[0].cols == 32);
        let m = ab_mixer();
        assert_eq!((m.layers[0].group, m.layers[1].group), (CrossGroup::A, CrossGroup::B));
    }
}
