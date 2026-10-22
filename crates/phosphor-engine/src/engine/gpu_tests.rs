//! Tests that render on the GPU (skipped where there is none): `test_engine` + a frame read back.

use super::*;
use crate::{Composition, LayerState, ModInputs, SourceSpec};

/// An engine on the default GPU with the features the app asks for, or None (no GPU here).
pub(crate) fn test_engine() -> Option<Engine> {
    // No GPU backend compiled into this test build (the engine crate leaves that to the app; Metal is added for
    // tests on the Mac only): skip rather than panic.
    if wgpu::Instance::enabled_backend_features().is_empty() {
        return None;
    }
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
    let required_features = adapter.features() & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_features, ..Default::default() })).ok()?;
    Some(Engine::new(device, queue))
}

/// The middle pixel of the program after one frame of `comp`.
fn middle(engine: &mut Engine, comp: &Composition) -> [u8; 4] {
    engine.render(comp, 0.0, &ModInputs::default());
    let (w, h, px) = engine.read_output().expect("output");
    let i = ((h / 2 * w + w / 2) * 4) as usize;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

/// Hap Q Alpha is straight alpha like every other source: grey 80% at alpha 50% over black = 40% grey (it was
/// multiplied by alpha twice: 20%).
#[test]
fn hap_q_alpha_composites_like_straight_alpha() {
    let Some(mut engine) = test_engine() else { return };
    if !engine.compressed_supported() {
        return;
    }
    let mut comp = Composition { width: 16, height: 16, ..Default::default() };
    let mut black = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    black.generator_values = vec![0.0, 0.0, 0.0];
    comp.layers.push(black);
    comp.layers.push(LayerState { id: 2, source: SourceSpec::Movie { name: "q".into(), path: Some("q.mov".into()) }, ..Default::default() });
    // One DXT5 block per 4x4: alpha part = Y (204 = 0.8) everywhere; colour part = Co, Cg at 128 (no tint), scale 1.
    let colour = (16u16 << 11) | (32 << 5);
    let mut block = vec![204u8, 204, 0, 0, 0, 0, 0, 0];
    block.extend_from_slice(&colour.to_le_bytes());
    block.extend_from_slice(&colour.to_le_bytes());
    block.extend_from_slice(&[0, 0, 0, 0]);
    let blocks: Vec<u8> = block.iter().copied().cycle().take(16 * 16).collect();
    // BC4 alpha: 128 everywhere.
    let alpha: Vec<u8> = [128u8, 128, 0, 0, 0, 0, 0, 0].iter().copied().cycle().take(16 * 8).collect();
    engine.upload_compressed(2, false, 16, 16, CompressedFormat::Bc3YCoCg, &blocks, Some(&alpha));
    let px = middle(&mut engine, &comp);
    assert!((px[1] as i32 - 102).abs() <= 6, "40% grey expected, got {px:?}");
}

/// A zero-copy AYUV frame (ProRes 4444): TV-range white at alpha 50% over black = 50% grey.
#[test]
fn ayuv_frames_convert_with_their_alpha() {
    let Some(mut engine) = test_engine() else { return };
    if !engine.device().features().contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM) {
        return;
    }
    let mut comp = Composition { width: 16, height: 16, ..Default::default() };
    let mut black = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    black.generator_values = vec![0.0, 0.0, 0.0];
    comp.layers.push(black);
    comp.layers.push(LayerState { id: 2, source: SourceSpec::Movie { name: "p".into(), path: Some("p.mov".into()) }, ..Default::default() });
    let texture = engine.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("ayuv"),
        size: wgpu::Extent3d { width: 16, height: 16, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // A = 50%, Y = 235 (TV white), Cb = Cr = 128, as 16-bit.
    let texel: [u16; 4] = [32768, 235 * 257, 128 * 257, 128 * 257];
    let data: Vec<u8> = texel.iter().flat_map(|v| v.to_le_bytes()).cycle().take(16 * 16 * 8).collect();
    engine.gpu.queue.write_texture(texture.as_image_copy(), &data, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(16 * 8), rows_per_image: None }, texture.size());
    engine.set_layer_ayuv_texture(2, false, texture);
    let px = middle(&mut engine, &comp);
    for c in &px[..3] {
        assert!((*c as i32 - 128).abs() <= 4, "50% grey expected, got {px:?}");
    }
}

/// Every blend mode against its formula: 50% grey over 25% grey (mattes cut the 25% by the top layer's luma/alpha).
#[test]
fn blend_modes_match_their_formulas() {
    use crate::BlendMode as B;
    let Some(mut engine) = test_engine() else { return };
    let solid = |id: u64, v: f32, blend: B| {
        let mut l = LayerState { id, source: SourceSpec::Generator("solid".into()), blend, ..Default::default() };
        l.generator_values = vec![v, v, v];
        l
    };
    let expect = [
        (B::Normal, 0.5),
        (B::Add, 0.75),
        (B::Multiply, 0.125),
        (B::Screen, 0.625),
        (B::Overlay, 0.25),
        (B::Difference, 0.25),
        (B::Lighten, 0.5),
        (B::Darken, 0.25),
        (B::Subtract, 0.0),
        (B::Exclusion, 0.5),
        // At 50% the light modes leave the layer below as it is.
        (B::SoftLight, 0.25),
        (B::HardLight, 0.25),
        (B::VividLight, 0.25),
        (B::LinearLight, 0.25),
        (B::PinLight, 0.25),
        (B::HardMix, 0.0),
        (B::ColorBurn, 0.0),
        (B::ColorDodge, 0.5),
        (B::Divide, 0.5),
        // Greys: hue / saturation / colour keep the grey below; luminosity takes the layer's.
        (B::Hue, 0.25),
        (B::Saturation, 0.25),
        (B::Color, 0.25),
        (B::Luminosity, 0.5),
        // Wipes at 100% opacity: fully in.
        (B::WipeRight, 0.5),
        (B::WipeDown, 0.5),
        (B::WipeRadial, 0.5),
        (B::WipeDiamond, 0.5),
        (B::WipeBlocks, 0.5),
        (B::Dissolve, 0.5),
        (B::LumaMask, 0.125),
        (B::InverseLumaMask, 0.125),
        (B::AlphaMask, 0.25),
    ];
    assert_eq!(expect.len(), B::ALL.len(), "a new blend mode needs a line here");
    let mut wrong = Vec::new();
    for (mode, want) in expect {
        let mut comp = Composition { width: 16, height: 16, ..Default::default() };
        comp.layers.push(solid(1, 0.25, B::Normal));
        comp.layers.push(solid(2, 0.5, mode));
        let got = middle(&mut engine, &comp)[0] as f32 / 255.0;
        if (got - want).abs() > 0.02 {
            wrong.push(format!("{mode:?}: {got:.3}, expected {want}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
    // 75% over 25%, where the light modes differ from each other.
    for (mode, want) in [(B::SoftLight, 0.375), (B::HardLight, 0.625), (B::VividLight, 0.5), (B::LinearLight, 0.75), (B::PinLight, 0.5), (B::ColorDodge, 1.0), (B::Divide, 0.333)] {
        let mut comp = Composition { width: 16, height: 16, ..Default::default() };
        comp.layers.push(solid(1, 0.25, B::Normal));
        comp.layers.push(solid(2, 0.75, mode));
        let got = middle(&mut engine, &comp)[0] as f32 / 255.0;
        if (got - want).abs() > 0.02 {
            wrong.push(format!("{mode:?} at 75%: {got:.3}, expected {want}"));
        }
    }
    // Colour: red over 50% grey = red at the grey's brightness (1.0, 0.286, 0.286).
    let mut comp = Composition { width: 16, height: 16, ..Default::default() };
    comp.layers.push(solid(1, 0.5, B::Normal));
    let mut red = solid(2, 0.0, B::Color);
    red.generator_values = vec![1.0, 0.0, 0.0];
    comp.layers.push(red);
    let px = middle(&mut engine, &comp);
    if (px[0] as i32 - 255).abs() > 4 || (px[1] as i32 - 73).abs() > 5 || (px[2] as i32 - 73).abs() > 5 {
        wrong.push(format!("Color: red over grey gave {px:?}, expected about [255, 73, 73]"));
    }
    assert!(wrong.is_empty(), "{wrong:?}");
}

/// Opacity mixes over what's below, and a group's opacity applies to its members as one picture.
#[test]
fn layer_and_group_opacity() {
    let Some(mut engine) = test_engine() else { return };
    let solid = |id: u64, rgb: [f32; 3]| {
        let mut l = LayerState { id, source: SourceSpec::Generator("solid".into()), ..Default::default() };
        l.generator_values = rgb.to_vec();
        l
    };
    // White at 25% over black: 25% grey.
    let mut comp = Composition { width: 16, height: 16, next_id: 100, ..Default::default() };
    comp.layers.push(solid(1, [0.0, 0.0, 0.0]));
    let mut white = solid(2, [1.0, 1.0, 1.0]);
    white.opacity = 0.25;
    comp.layers.push(white);
    let px = middle(&mut engine, &comp);
    assert!((px[0] as i32 - 64).abs() <= 3, "25% white over black: {px:?}");
    // The same white, fully opaque inside a group at 50%: 50% grey.
    comp.layers[1].opacity = 1.0;
    let g = comp.create_group(&[2]).expect("group");
    comp.groups.iter_mut().find(|x| x.id == g).unwrap().opacity = 0.5;
    let px = middle(&mut engine, &comp);
    assert!((px[0] as i32 - 128).abs() <= 3, "group at 50%: {px:?}");
}

/// An output can be frozen (keeps its picture), held back N frames, and mirrored.
#[test]
fn outputs_freeze_delay_and_flip() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 32, height: 16, next_id: 100, ..Default::default() };
    let mut grey = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    grey.generator_values = vec![0.2, 0.2, 0.2];
    comp.layers.push(grey);
    let out = comp.add_output();
    // (left quarter, right quarter) red of the output after one frame.
    let frame = |engine: &mut Engine, comp: &Composition, v: f32| {
        let mut c = comp.clone();
        c.layers[0].generator_values = vec![v, v, v];
        engine.render(&c, 0.0, &ModInputs::default());
        let (w, h, px) = engine.read_output_of(out).expect("output");
        let at = |x: u32| px[((h / 2 * w + x) * 4) as usize] as f32 / 255.0;
        (at(w / 4), at(w * 3 / 4))
    };
    let near = |a: f32, b: f32| (a - b).abs() < 0.03;
    assert!(near(frame(&mut engine, &comp, 0.2).0, 0.2));
    comp.outputs[0].frozen = true;
    assert!(near(frame(&mut engine, &comp, 0.8).0, 0.2), "frozen keeps the old picture");
    comp.outputs[0].frozen = false;
    assert!(near(frame(&mut engine, &comp, 0.8).0, 0.8), "live again");
    // Two frames late: the third frame shows the first.
    comp.outputs[0].delay_frames = 2;
    let shown: Vec<f32> = [0.3, 0.5, 0.7, 0.9].iter().map(|v| frame(&mut engine, &comp, *v).0).collect();
    assert!(near(shown[2], 0.3) && near(shown[3], 0.5), "delayed: {shown:?}");
    // Mirrored: a picture on the left only ends up on the right.
    comp.outputs[0].delay_frames = 0;
    comp.layers[0].transform.crop = [0.0, 0.5, 0.0, 0.0];
    let (l, r) = frame(&mut engine, &comp, 1.0);
    assert!(near(l, 1.0) && near(r, 0.0), "cropped: left {l} right {r}");
    comp.outputs[0].flip_x = true;
    let (l, r) = frame(&mut engine, &comp, 1.0);
    assert!(near(l, 0.0) && near(r, 1.0), "flipped: left {l} right {r}");
}

/// A layer can show another layer's picture before or after that layer's effects.
#[test]
fn feeds_before_and_after_effects() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 16, height: 16, next_id: 100, ..Default::default() };
    let mut grey = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    grey.generator_values = vec![0.2, 0.2, 0.2];
    grey.effects.push(comp.new_effect("invert").unwrap());
    comp.layers.push(grey);
    let mut copy = LayerState { id: 2, source: SourceSpec::Feed { id: 1, name: "grey".into(), before_fx: false }, ..Default::default() };
    copy.fit = crate::FitMode::Stretch;
    comp.layers.push(copy);
    let after = middle(&mut engine, &comp)[0] as f32 / 255.0;
    comp.layers[1].source = SourceSpec::Feed { id: 1, name: "grey".into(), before_fx: true };
    middle(&mut engine, &comp);
    let before = middle(&mut engine, &comp)[0] as f32 / 255.0;
    assert!((after - 0.8).abs() < 0.03, "after the invert: {after}");
    assert!((before - 0.2).abs() < 0.03, "before the invert: {before}");
}

/// An effect with a blend mode lands on its own input that way: grey 25% inverted (75%), multiplied = 18.75%.
#[test]
fn effects_blend_onto_their_input() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 16, height: 16, next_id: 100, ..Default::default() };
    let mut grey = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    grey.generator_values = vec![0.25, 0.25, 0.25];
    let mut fx = comp.new_effect("invert").unwrap();
    fx.blend = crate::BlendMode::Multiply;
    grey.effects.push(fx);
    comp.layers.push(grey);
    let got = middle(&mut engine, &comp)[0] as f32 / 255.0;
    assert!((got - 0.1875).abs() < 0.02, "{got}");
    comp.layers[0].effects[0].blend = crate::BlendMode::Normal;
    let got = middle(&mut engine, &comp)[0] as f32 / 255.0;
    assert!((got - 0.75).abs() < 0.02, "plain: {got}");
}

/// A wipe at half opacity: in on the left half (left to right), out on the right.
#[test]
fn wipes_follow_the_opacity() {
    use crate::BlendMode as B;
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 32, height: 16, ..Default::default() };
    let mut black = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    black.generator_values = vec![0.0, 0.0, 0.0];
    let mut white = LayerState { id: 2, source: SourceSpec::Generator("solid".into()), blend: B::WipeRight, opacity: 0.5, ..Default::default() };
    white.generator_values = vec![1.0, 1.0, 1.0];
    comp.layers = vec![black, white];
    engine.render(&comp, 0.0, &ModInputs::default());
    let (w, h, px) = engine.read_output().expect("output");
    let at = |x: u32| px[((h / 2 * w + x) * 4) as usize];
    assert!(at(w / 8) > 240 && at(w * 7 / 8) < 15, "left {} right {}", at(w / 8), at(w * 7 / 8));
    // Dissolve at 30%: about 30% of the pixels are in.
    comp.layers[1].blend = B::Dissolve;
    comp.layers[1].opacity = 0.3;
    engine.render(&comp, 0.0, &ModInputs::default());
    let (_, _, px) = engine.read_output().expect("output");
    let lit = px.chunks(4).filter(|p| p[0] > 128).count() as f32 / (px.len() / 4) as f32;
    assert!((lit - 0.3).abs() < 0.12, "{lit} of the pixels in");
}

/// A generator at half render size draws a quarter of the pixels and still fills the canvas.
#[test]
fn generators_render_smaller_when_asked() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 64, height: 32, ..Default::default() };
    let mut l = LayerState { id: 7, source: SourceSpec::Generator("solid".into()), render_scale: 0.5, ..Default::default() };
    l.generator_values = vec![0.6, 0.6, 0.6];
    comp.layers.push(l);
    engine.render(&comp, 0.0, &ModInputs::default());
    let (w, h, px) = engine.read_output().expect("output");
    let corner = px[((h - 2) * w + w - 2) as usize * 4] as f32 / 255.0;
    assert!((corner - 0.6).abs() < 0.03, "fills the canvas: {corner}");
    let g = engine.layers.get(&7).and_then(|r| r.generator.as_ref()).map(|t| (t.width, t.height));
    assert_eq!(g, Some((32, 16)));
}

/// A layer at 10 fps keeps its picture between its frames.
#[test]
fn layers_can_run_at_their_own_frame_rate() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 16, height: 16, ..Default::default() };
    let mut l = LayerState { id: 3, source: SourceSpec::Generator("solid".into()), render_fps: 10.0, ..Default::default() };
    l.generator_values = vec![0.2, 0.2, 0.2];
    comp.layers.push(l);
    let at = |engine: &mut Engine, comp: &Composition, t: f32| {
        engine.render(comp, t, &ModInputs::default());
        let (w, h, px) = engine.read_output().expect("output");
        px[((h / 2 * w + w / 2) * 4) as usize] as f32 / 255.0
    };
    assert!((at(&mut engine, &comp, 0.0) - 0.2).abs() < 0.03);
    comp.layers[0].generator_values = vec![0.8, 0.8, 0.8];
    assert!((at(&mut engine, &comp, 0.05) - 0.2).abs() < 0.03, "held between frames");
    assert!((at(&mut engine, &comp, 0.11) - 0.8).abs() < 0.03, "next frame");
}

/// A mapped output's background picture shows where no slice covers it.
#[test]
fn output_backgrounds_show_behind_slices() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 32, height: 32, next_id: 100, ..Default::default() };
    let mut l = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    l.generator_values = vec![0.0, 1.0, 0.0];
    comp.layers.push(l);
    let out = comp.add_output();
    let sid = comp.new_id();
    let o = comp.outputs.iter_mut().find(|o| o.id == out).unwrap();
    let s = o.mapping.add_surface(sid);
    s.corners = [[0.0, 0.0], [0.5, 0.0], [0.5, 0.5], [0.0, 0.5]];
    o.mapping.enabled = true;
    o.background = "red.png".into();
    o.background_opacity = 0.5;
    let red: Vec<u8> = [255u8, 0, 0, 255].repeat(4 * 4);
    engine.set_output_background(out, Some((4, 4, &red)));
    engine.render(&comp, 0.0, &ModInputs::default());
    let (w, h, px) = engine.read_output_of(out).expect("output");
    let at = |x: u32, y: u32| { let i = ((y * w + x) * 4) as usize; [px[i], px[i + 1], px[i + 2]] };
    let slice = at(w / 4, h / 4);
    let bg = at(w * 3 / 4, h * 3 / 4);
    assert!(slice[1] > 200 && slice[0] < 30, "the slice: {slice:?}");
    assert!((bg[0] as i32 - 128).abs() < 10 && bg[1] < 20, "half-strength red behind: {bg:?}");
}

/// The particle fountain draws: some pixels lit, most of the picture dark.
#[test]
fn particles_draw_a_fountain() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 128, height: 72, ..Default::default() };
    let desc = crate::shaders::generator("particles").expect("built in");
    comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator("particles".into()), generator_values: desc.defaults(), ..Default::default() });
    for _ in 0..3 {
        engine.render(&comp, 2.0, &ModInputs { time: 2.0, ..Default::default() });
    }
    let (_, _, px) = engine.read_output().expect("output");
    let lit = px.chunks(4).filter(|p| p[0].max(p[1]).max(p[2]) > 60).count() as f32 / (px.len() / 4) as f32;
    assert!(lit > 0.002 && lit < 0.5, "{lit} of the pixels lit");
}

/// A look fade: the old picture shows first and gives way to the new one over the fade.
#[test]
fn a_new_look_fades_in() {
    let Some(mut engine) = test_engine() else { return };
    let solid = |v: f32| {
        let mut comp = Composition { width: 32, height: 32, ..Default::default() };
        comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator("solid".into()), generator_values: vec![v, v, v], ..Default::default() });
        comp
    };
    let red = |engine: &mut Engine, comp: &Composition, t: f32| {
        engine.render(comp, t, &ModInputs { time: t, ..Default::default() });
        let (_, _, px) = engine.read_output().expect("output");
        px[0] as f32 / 255.0
    };
    let (white, black) = (solid(1.0), solid(0.0));
    assert!(red(&mut engine, &white, 0.0) > 0.95);
    engine.fade_in_next(2.0);
    assert!(red(&mut engine, &black, 1.0) > 0.95, "starts from the old picture");
    let mid = red(&mut engine, &black, 2.0);
    assert!((0.3..0.7).contains(&mid), "halfway: {mid}");
    assert!(red(&mut engine, &black, 3.5) < 0.05, "done");
}

/// A held monitor keeps its last picture while the layer changes; released, it follows again.
#[test]
fn held_monitors_keep_their_picture() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 32, height: 32, ..Default::default() };
    comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator("solid".into()), generator_values: vec![1.0, 1.0, 1.0], ..Default::default() });
    let shown = |engine: &mut Engine, comp: &Composition, hold: bool| {
        engine.request_monitors(vec![(1, 16, 16)]);
        engine.hold_monitors(if hold { vec![0] } else { Vec::new() });
        engine.render(comp, 0.0, &ModInputs::default());
        let t = engine.monitor(0).expect("monitor");
        engine.read_target(t).expect("read").2[0]
    };
    assert!(shown(&mut engine, &comp, false) > 240);
    comp.layers[0].generator_values = vec![0.0, 0.0, 0.0];
    assert!(shown(&mut engine, &comp, true) > 240, "held: still white");
    assert!(shown(&mut engine, &comp, false) < 15, "released: black now");
}

/// A layer's thumbnail reads back (a clip's chosen picture comes from it).
#[test]
fn a_layer_thumbnail_reads_back() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 64, height: 36, ..Default::default() };
    comp.layers.push(LayerState { id: 3, source: SourceSpec::Generator("solid".into()), generator_values: vec![0.0, 1.0, 0.0], ..Default::default() });
    engine.render(&comp, 0.0, &ModInputs::default());
    let (w, h, px) = engine.read_layer_thumbnail(3).expect("thumbnail");
    assert!(w > 0 && h > 0);
    assert!(px[1] > 240 && px[0] < 15, "green: {:?}", &px[..4]);
    assert!(engine.read_layer_thumbnail(99).is_none());
}
