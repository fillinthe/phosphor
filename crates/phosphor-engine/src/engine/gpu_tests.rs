//! Tests that render on the GPU (skipped where there is none): `test_engine` + a frame read back.

use super::*;
use crate::{Composition, LayerState, ModInputs, SourceSpec};

/// An engine on the default GPU with the features the app asks for, or None (no GPU here).
pub(crate) fn test_engine() -> Option<Engine> {
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
