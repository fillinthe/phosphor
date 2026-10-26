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
    let deep = crate::gpu::deep_colour_usable(&adapter);
    let mut required_features = adapter.features() & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM);
    if deep {
        required_features |= crate::gpu::deep_colour_features();
    }
    crate::gpu::set_deep_colour(deep);
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

/// The playing clip's own effects run on its layer, then the layer's: one invert = 80%, two = back to 20%.
#[test]
fn a_clips_effects_run_before_the_layers() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 16, height: 16, next_id: 100, ..Default::default() };
    let mut grey = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    grey.generator_values = vec![0.2, 0.2, 0.2];
    grey.clip_effects.push(comp.new_effect("invert").unwrap());
    comp.layers.push(grey);
    let once = middle(&mut engine, &comp)[0] as f32 / 255.0;
    let fx = comp.new_effect("invert").unwrap();
    comp.layers[0].effects.push(fx);
    let twice = middle(&mut engine, &comp)[0] as f32 / 255.0;
    assert!((once - 0.8).abs() < 0.03, "the clip's invert: {once}");
    assert!((twice - 0.2).abs() < 0.03, "then the layer's: {twice}");
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

/// A user blend mode: `src` is the layer, `below` the stack under it; the layer's opacity mixes the result in.
#[test]
fn a_user_blend_mode_blends() {
    let Some(mut engine) = test_engine() else { return };
    let source = "// @name Difference\n// @kind blend\n\n@fragment\nfn main(in: VOut) -> @location(0) vec4f {\n    return vec4f(abs(src(in.uv).rgb - below(in.uv).rgb), 1.0);\n}\n";
    let desc = engine.compile_custom("customblend:difference", source).expect("compiles");
    assert!(crate::shaders::mixer(desc.id).is_some(), "listed as a mixer");
    let mut comp = Composition { width: 32, height: 32, ..Default::default() };
    let solid = |id: u64, v: f32| LayerState { id, source: SourceSpec::Generator("solid".into()), generator_values: vec![v, v, v], ..Default::default() };
    comp.layers.push(solid(1, 0.8));
    let mut top = solid(2, 0.3);
    top.mixer = Some(desc.id.to_string());
    comp.layers.push(top);
    let red = |engine: &mut Engine, comp: &Composition| {
        engine.render(comp, 0.0, &ModInputs::default());
        engine.read_output().expect("output").2[0] as f32 / 255.0
    };
    let v = red(&mut engine, &comp);
    assert!((v - 0.5).abs() < 0.03, "|0.3 - 0.8| = {v}");
    comp.layers[1].opacity = 0.5;
    let v = red(&mut engine, &comp);
    assert!((v - 0.65).abs() < 0.03, "half way back to 0.8: {v}");
}

/// Hide Face (black): the face's spot goes dark, the rest of the picture stays.
#[test]
fn hide_face_covers_only_the_face() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 64, height: 64, ..Default::default() };
    comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator("solid".into()), generator_values: vec![1.0, 1.0, 1.0], ..Default::default() });
    // Face at the top left quarter (Y counts up), black style.
    let values = vec![0.25, 0.75, 0.3, 1.0, 2.0, 24.0];
    comp.master_effects.push(EffectState { id: 9, effect: "hide_face".into(), enabled: true, values, mix: 1.0, mods: Vec::new(), blend: Default::default() });
    engine.render(&comp, 0.0, &ModInputs::default());
    let (w, _, px) = engine.read_output().expect("output");
    let at = |x: u32, y: u32| px[((y * w + x) * 4) as usize];
    assert!(at(16, 16) < 10, "face hidden");
    assert!(at(48, 48) > 245, "the rest untouched");
    comp.master_effects[0].values[3] = 0.0;
    engine.render(&comp, 0.0, &ModInputs::default());
    let (w, _, px) = engine.read_output().expect("output");
    assert!(px[((16 * w + 16) * 4) as usize] > 245, "no face found: nothing hidden");
}

/// Blur Background: where the matte says "person" (left half) the checkerboard stays sharp, the rest is blurred.
#[test]
fn blur_background_keeps_the_person_sharp() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 128, height: 64, ..Default::default() };
    let mut layer = LayerState { id: 1, source: SourceSpec::Generator("checker".into()), ..Default::default() };
    layer.generator_values = crate::shaders::generator("checker").unwrap().defaults();
    layer.generator_values[0] = 40.0;
    if let Some(v) = layer.generator_values.get_mut(1) {
        *v = 0.0;
    }
    layer.effects.push(EffectState { id: 5, effect: "blur_bg".into(), enabled: true, values: vec![1.0, 0.5, 0.05, 0.0], mix: 1.0, mods: Vec::new(), blend: Default::default() });
    comp.layers.push(layer);
    let mask: Vec<u8> = (0..32 * 16).map(|i| if i % 32 < 16 { 255 } else { 0 }).collect();
    engine.set_matte(1, 32, 16, &mask);
    engine.render(&comp, 0.0, &ModInputs::default());
    let (w, h, px) = engine.read_output().expect("output");
    // How far each half's pixels are from mid-grey: sharp squares are far, blur pulls them in.
    let spread = |x0: u32, x1: u32| {
        let mut sum = 0.0;
        let mut n = 0.0;
        for y in 8..h - 8 {
            for x in x0..x1 {
                sum += (px[((y * w + x) * 4) as usize] as f32 / 255.0 - 0.5).abs();
                n += 1.0;
            }
        }
        sum / n
    };
    let (person, background) = (spread(8, 56), spread(72, 120));
    assert!(person > 0.4, "sharp: {person}");
    assert!(background < person - 0.15, "blurred: {background} vs {person}");
}

/// Palette: a dark grey with only black and white to choose from becomes black; a light one white.
#[test]
fn palette_snaps_to_the_nearest_colour() {
    let Some(mut engine) = test_engine() else { return };
    let red_of = |engine: &mut Engine, grey: f32| {
        let mut comp = Composition { width: 16, height: 16, ..Default::default() };
        let mut l = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), generator_values: vec![grey, grey, grey], ..Default::default() };
        let mut values = crate::shaders::effect("palette").unwrap().defaults();
        values[..6].copy_from_slice(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        values[18] = 2.0;
        l.effects.push(EffectState { id: 2, effect: "palette".into(), enabled: true, values, mix: 1.0, mods: Vec::new(), blend: Default::default() });
        comp.layers.push(l);
        engine.render(&comp, 0.0, &ModInputs::default());
        engine.read_output().expect("output").2[0]
    };
    assert!(red_of(&mut engine, 0.3) < 5);
    assert!(red_of(&mut engine, 0.7) > 250);
}

/// Smooth edges: a diamond-shaped slice has in-between pixels along its edges only when the output smooths them.
#[test]
fn smooth_outputs_soften_slice_edges() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 64, height: 64, ..Default::default() };
    comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator("solid".into()), generator_values: vec![1.0, 1.0, 1.0], ..Default::default() });
    let out = comp.add_output();
    let o = comp.outputs.iter_mut().find(|o| o.id == out).unwrap();
    (o.width, o.height) = (64, 64);
    o.mapping.enabled = true;
    o.mapping.add_surface(50).corners = [[0.5, 0.05], [0.95, 0.5], [0.5, 0.95], [0.05, 0.5]];
    let soft = |engine: &mut Engine, comp: &Composition| {
        engine.render(comp, 0.0, &ModInputs::default());
        let (_, _, px) = engine.read_output_of(out).expect("output");
        px.chunks(4).filter(|p| p[0] > 30 && p[0] < 225).count()
    };
    let hard = soft(&mut engine, &comp);
    comp.outputs[0].smooth = true;
    let smooth = soft(&mut engine, &comp);
    assert!(smooth > hard + 40, "edges: {hard} in-between pixels sharp, {smooth} smooth");
    let (w, h, _) = engine.read_output_of(out).unwrap();
    assert_eq!((w, h), (64, 64), "the output keeps its size");
}

/// LED pixels that average their cell: 9 samples each go to the GPU and come back, and every pixel gets its colour.
#[test]
fn averaged_led_pixels_sample_on_the_gpu() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 32, height: 32, ..Default::default() };
    let mut grey = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    grey.generator_values = vec![0.5, 0.25, 1.0];
    comp.layers.push(grey);
    let strip = crate::Fixture { cols: 3, points: [[0.1, 0.5], [0.9, 0.5], [0.0; 2], [0.0; 2]], average: true, gamma: 1.0, ..Default::default() };
    comp.dmx.fixtures.push(strip);
    let points = comp.dmx.sample_points();
    assert_eq!(points.len(), 27);
    engine.set_sample_points(&points);
    let mut colors = None;
    for _ in 0..20 {
        engine.request_samples();
        engine.render(&comp, 0.0, &ModInputs::default());
        if let Some(c) = engine.take_samples() {
            colors = Some(c);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let colors = colors.expect("samples came back");
    assert_eq!(colors.len(), 27);
    let u = comp.dmx.universes(&colors, 0.0);
    let data = u.values().next().unwrap();
    for px in data[..9].chunks(3) {
        assert!(px[0].abs_diff(128) < 3 && px[1].abs_diff(64) < 3 && px[2] > 250, "{px:?}");
    }
}

/// Mesh handles reach the output: bending a mesh point's curve moves the picture between the points (and the
/// cached mesh is rebuilt for it), straightening it puts the picture back.
#[test]
fn mesh_handles_bend_the_output() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 64, height: 64, ..Default::default() };
    comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator("solid".into()), generator_values: vec![1.0, 1.0, 1.0], ..Default::default() });
    let out = comp.add_output();
    let o = comp.outputs.iter_mut().find(|o| o.id == out).unwrap();
    (o.width, o.height) = (64, 64);
    o.mapping.enabled = true;
    let s = o.mapping.add_surface(50);
    s.corners = [[0.2, 0.2], [0.8, 0.2], [0.8, 0.8], [0.2, 0.8]];
    s.mesh = Some(crate::mapping::Mesh::from_corners(&s.corners, 2, 2));
    let lit = |engine: &mut Engine, comp: &Composition| {
        engine.render(comp, 0.0, &ModInputs::default());
        let (_, _, px) = engine.read_output_of(out).expect("output");
        px.chunks(4).map(|p| p[0] > 128).collect::<Vec<_>>()
    };
    let flat = lit(&mut engine, &comp);
    // The top edge's middle point: its curve across pulled steeply up and down (an S along the edge).
    fn mesh(comp: &mut Composition) -> &mut crate::mapping::Mesh {
        comp.outputs[0].mapping.surfaces[0].mesh.as_mut().unwrap()
    }
    mesh(&mut comp).set_direction(1, 0, [0.3, 0.4]);
    let bent = lit(&mut engine, &comp);
    let moved = flat.iter().zip(&bent).filter(|(a, b)| a != b).count();
    assert!(moved > 20, "{moved} pixels changed");
    mesh(&mut comp).reset_handles(1);
    assert_eq!(lit(&mut engine, &comp), flat, "straightened: as before");
}

/// Deep colour keeps detail 8 bits lose: white under black at 99.8% leaves 0.2% (8-bit rounds that up to 1/255),
/// then three gain effects multiply it 64 times. Deep colour shows about 13%, 8-bit about 25%.
#[test]
fn deep_colour_keeps_fine_detail() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 16, height: 16, ..Default::default() };
    comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator("solid".into()), generator_values: vec![1.0, 1.0, 1.0], ..Default::default() });
    comp.layers.push(LayerState { id: 2, source: SourceSpec::Generator("solid".into()), generator_values: vec![0.0, 0.0, 0.0], opacity: 0.998, ..Default::default() });
    for id in 10..13 {
        comp.master_effects.push(crate::EffectState { id, effect: "color_match".into(), enabled: true, values: vec![4.0, 4.0, 4.0, 0.0, 0.0, 0.0], mix: 1.0, mods: Vec::new(), blend: Default::default() });
    }
    if !crate::gpu::deep_colour() {
        return;
    }
    let shallow = middle(&mut engine, &comp)[0];
    comp.deep_color = true;
    let deep = middle(&mut engine, &comp)[0];
    assert!(shallow > 55 && deep < 45 && deep > 20, "8-bit {shallow}, deep {deep}");
}

/// Deep colour changes no look: every saved test project renders the same at the same moment with it on and off
/// (only rounding differs).
#[test]
fn deep_colour_keeps_every_look() {
    let Some(mut engine) = test_engine() else { return };
    if !crate::gpu::deep_colour() {
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/old_projects");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
    files.sort();
    for path in files {
        let Ok(mut comp) = Composition::from_json(&std::fs::read_to_string(&path).unwrap()) else { continue };
        (comp.width, comp.height) = (160, 90);
        let picture = |engine: &mut Engine, comp: &Composition| {
            for _ in 0..3 {
                engine.render(comp, 1.5, &ModInputs::default());
            }
            engine.read_output().expect("output").2
        };
        comp.deep_color = false;
        let flat = picture(&mut engine, &comp);
        comp.deep_color = true;
        let deep = picture(&mut engine, &comp);
        let diff = flat.iter().zip(&deep).map(|(a, b)| a.abs_diff(*b) as u64).sum::<u64>() as f64 / flat.len().max(1) as f64;
        assert!(diff < 2.0, "{}: mean difference {diff:.2}", path.display());
    }
}

/// A 3D model's light and edges reach the output: shaded, its grey faces come out at different brightnesses;
/// with edges, bright lines appear.
#[test]
fn model_light_and_edges_draw() {
    let Some(mut engine) = test_engine() else { return };
    let cube = "v -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nv -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n\
f 1/1 2/2 3/3 4/4\nf 6/1 5/2 8/3 7/4\nf 5/1 1/2 4/3 8/4\nf 2/1 6/2 7/3 3/4\nf 4/1 3/2 7/3 8/4\nf 5/1 6/2 2/3 1/4\n";
    let path = std::env::temp_dir().join("phosphor-gpu-lit-cube.obj");
    std::fs::write(&path, cube).unwrap();
    let mut comp = Composition { width: 64, height: 64, ..Default::default() };
    comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator("solid".into()), generator_values: vec![0.5, 0.5, 0.5], ..Default::default() });
    let out = comp.add_output();
    let o = comp.outputs.iter_mut().find(|o| o.id == out).unwrap();
    (o.width, o.height) = (128, 128);
    o.mapping.enabled = true;
    o.mapping.add_surface(50).model = Some(crate::mapping::Model3d { path: path.to_string_lossy().to_string(), yaw: 0.08, pitch: 0.06, ..Default::default() });
    let levels = |engine: &mut Engine, comp: &Composition| {
        engine.render(comp, 0.0, &ModInputs::default());
        let (_, _, px) = engine.read_output_of(out).expect("output");
        let lit: Vec<u8> = px.chunks(4).map(|p| p[0]).filter(|v| *v > 8).collect();
        let mut kinds: Vec<u8> = lit.iter().map(|v| v / 16).collect();
        kinds.sort();
        kinds.dedup();
        (kinds.len(), lit.iter().filter(|v| **v > 200).count())
    };
    let model = |comp: &mut Composition| comp.outputs[0].mapping.surfaces[0].model.as_mut().unwrap().clone();
    let (flat_kinds, flat_bright) = levels(&mut engine, &comp);
    let mut m = model(&mut comp);
    m.shade = 1.0;
    m.light = [0.05, 0.0];
    comp.outputs[0].mapping.surfaces[0].model = Some(m.clone());
    let (lit_kinds, _) = levels(&mut engine, &comp);
    m.edges = 3.0;
    comp.outputs[0].mapping.surfaces[0].model = Some(m);
    let (_, edge_bright) = levels(&mut engine, &comp);
    if let Ok(dump) = std::env::var("PHOSPHOR_DUMP_PPM") {
        let (w, h, px) = engine.read_output_of(out).unwrap();
        let mut ppm = format!("P6 {w} {h} 255\n").into_bytes();
        ppm.extend(px.chunks(4).flat_map(|p| [p[0], p[1], p[2]]));
        std::fs::write(dump, ppm).unwrap();
    }
    let _ = std::fs::remove_file(&path);
    assert!(lit_kinds >= flat_kinds + 2, "shaded faces: {flat_kinds} brightness bands flat, {lit_kinds} lit");
    assert!(edge_bright > flat_bright + 100, "edges: {flat_bright} bright pixels before, {edge_bright} after");
}

/// Shadows reach the output: a box over a floor under a light from above darkens part of the lit floor. The model
/// has no texture coordinates: Box wrapping spreads the picture over the floor (from the front it gets one row).
#[test]
fn model_shadows_draw() {
    let Some(mut engine) = test_engine() else { return };
    let obj = "v -3 -1 -3\nv -3 -1 3\nv 3 -1 3\nv 3 -1 -3\n\
v -0.5 0 0.5\nv 0.5 0 0.5\nv 0.5 1 0.5\nv -0.5 1 0.5\nv -0.5 0 -0.5\nv 0.5 0 -0.5\nv 0.5 1 -0.5\nv -0.5 1 -0.5\n\
f 1 2 3 4\nf 5 6 7 8\nf 10 9 12 11\nf 9 5 8 12\nf 6 10 11 7\nf 8 7 11 12\nf 9 10 6 5\n";
    let path = std::env::temp_dir().join("phosphor-gpu-shadow.obj");
    std::fs::write(&path, obj).unwrap();
    let mut comp = Composition { width: 64, height: 64, ..Default::default() };
    comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator("solid".into()), generator_values: vec![0.8, 0.8, 0.8], ..Default::default() });
    let out = comp.add_output();
    let o = comp.outputs.iter_mut().find(|o| o.id == out).unwrap();
    (o.width, o.height) = (160, 160);
    o.mapping.enabled = true;
    let model = crate::mapping::Model3d { path: path.to_string_lossy().to_string(), yaw: 0.05, pitch: 0.08, cull: false, light: [0.03, 0.22], uv_mode: crate::mapping::UvMode::Box, ..Default::default() };
    o.mapping.add_surface(50).model = Some(model.clone());
    let dark = |engine: &mut Engine, comp: &Composition| {
        engine.render(comp, 0.0, &ModInputs::default());
        let (_, _, px) = engine.read_output_of(out).expect("output");
        (px.chunks(4).filter(|p| p[0] > 0 && p[0] < 60).count(), px.chunks(4).filter(|p| p[0] > 150).count())
    };
    let (before, _) = dark(&mut engine, &comp);
    comp.outputs[0].mapping.surfaces[0].model = Some(crate::mapping::Model3d { shadows: 0.9, ..model });
    let (after, bright) = dark(&mut engine, &comp);
    if let Ok(dump) = std::env::var("PHOSPHOR_DUMP_PPM") {
        let (w, h, px) = engine.read_output_of(out).unwrap();
        let mut ppm = format!("P6 {w} {h} 255\n").into_bytes();
        ppm.extend(px.chunks(4).flat_map(|p| [p[0], p[1], p[2]]));
        std::fs::write(dump, ppm).unwrap();
    }
    let _ = std::fs::remove_file(&path);
    assert!(after > before + 100 && bright > after, "dark pixels {before} then {after}, still lit {bright}");
}

/// An output with mapping on and only a mask ("only inside shows"): the picture still shows inside it with a corner
/// dragged past the output's edge, and all of it once the mask is dragged right off (it's left out).
#[test]
fn a_mask_past_the_edge_keeps_the_picture() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 32, height: 32, next_id: 100, ..Default::default() };
    let mut l = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    l.generator_values = vec![0.0, 1.0, 0.0];
    comp.layers.push(l);
    let out = comp.add_output();
    let mid = |engine: &mut Engine, comp: &Composition| {
        engine.render(comp, 0.0, &ModInputs::default());
        let (w, h, px) = engine.read_output_of(out).expect("output");
        px[((h / 2 * w + w / 2) * 4 + 1) as usize]
    };
    let mask_id = comp.new_id();
    let o = comp.outputs.iter_mut().find(|o| o.id == out).unwrap();
    o.mapping.enabled = true;
    let m = o.mapping.add_mask(mask_id);
    m.points = vec![[0.2, 0.2], [0.8, 0.2], [0.8, 0.8], [0.2, 0.8]];
    m.invert = true;
    let inside = mid(&mut engine, &comp);
    comp.outputs[0].mapping.masks[0].points[1] = [1.4, 0.1];
    let past = mid(&mut engine, &comp);
    comp.outputs[0].mapping.masks[0].points = vec![[1.2, 0.2], [1.8, 0.2], [1.8, 0.8], [1.2, 0.8]];
    let off = mid(&mut engine, &comp);
    assert!(inside > 200 && past > 200 && off > 200, "inside {inside} past the edge {past} all off {off}");
}

/// A mask's transform moves what it hides on the output (and so can be animated): the hidden square moves right.
#[test]
fn a_mask_transform_moves_it_on_the_output() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 32, height: 32, next_id: 100, ..Default::default() };
    let mut l = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    l.generator_values = vec![0.0, 1.0, 0.0];
    comp.layers.push(l);
    let out = comp.add_output();
    let mask_id = comp.new_id();
    let o = comp.outputs.iter_mut().find(|o| o.id == out).unwrap();
    o.mapping.enabled = true;
    o.mapping.add_mask(mask_id).points = vec![[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]];
    let green_at = |engine: &mut Engine, comp: &Composition, x: f32| {
        engine.render(comp, 0.0, &ModInputs::default());
        let (w, h, px) = engine.read_output_of(out).expect("output");
        px[((h / 2 * w + (x * w as f32) as u32) * 4 + 1) as usize]
    };
    assert!(green_at(&mut engine, &comp, 0.25) < 30 && green_at(&mut engine, &comp, 0.75) > 200, "the left half is hidden");
    comp.outputs[0].mapping.masks[0].transform.x = 0.5;
    assert!(green_at(&mut engine, &comp, 0.25) > 200 && green_at(&mut engine, &comp, 0.75) < 30, "moved: the right half is hidden");
}

/// A slice's transform moves its picture on the output (so it can be animated): the slice moves to the right half.
#[test]
fn a_slice_transform_moves_its_picture() {
    let Some(mut engine) = test_engine() else { return };
    let mut comp = Composition { width: 32, height: 32, next_id: 100, ..Default::default() };
    let mut l = LayerState { id: 1, source: SourceSpec::Generator("solid".into()), ..Default::default() };
    l.generator_values = vec![0.0, 1.0, 0.0];
    comp.layers.push(l);
    let out = comp.add_output();
    let sid = comp.new_id();
    let o = comp.outputs.iter_mut().find(|o| o.id == out).unwrap();
    o.mapping.enabled = true;
    o.mapping.add_surface(sid).corners = [[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]];
    let green_at = |engine: &mut Engine, comp: &Composition, x: f32| {
        engine.render(comp, 0.0, &ModInputs::default());
        let (w, h, px) = engine.read_output_of(out).expect("output");
        px[((h / 2 * w + (x * w as f32) as u32) * 4 + 1) as usize]
    };
    assert!(green_at(&mut engine, &comp, 0.25) > 200 && green_at(&mut engine, &comp, 0.75) < 30, "on the left");
    comp.outputs[0].mapping.surfaces[0].transform.x = 0.5;
    assert!(green_at(&mut engine, &comp, 0.25) < 30 && green_at(&mut engine, &comp, 0.75) > 200, "moved right");
}
