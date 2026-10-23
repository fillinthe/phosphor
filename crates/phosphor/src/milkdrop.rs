//! MilkDrop presets (.milk / .milk2, Winamp's visualiser, projectM's preset packs) as layers: a layer whose source is
//! `Live { format: "milkdrop", url: preset path }` is drawn by lactea (a MilkDrop engine on wgpu) into a texture on
//! Phosphor's own GPU device every frame, fed with the sound Phosphor is listening to, and shown as the layer's
//! picture (`Engine::set_layer_texture`). Only layers that are on are drawn.

use std::collections::HashMap;

use phosphor_engine::{Composition, Engine, SourceSpec};

/// The live-source format that marks a MilkDrop layer.
pub const FORMAT: &str = "milkdrop";
/// Drawn at most this wide (MilkDrop's look doesn't need more; the layer scales it).
const MAX_WIDTH: u32 = 1920;

pub fn is_preset(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case("milk") || e.eq_ignore_ascii_case("milk2"))
}

/// A layer source for a preset file.
pub fn source(path: &str) -> SourceSpec {
    let name = std::path::Path::new(path).file_stem().map_or_else(|| "MilkDrop".to_string(), |s| s.to_string_lossy().into_owned());
    SourceSpec::Live { name, format: FORMAT.into(), url: path.into() }
}

struct Visualizer {
    preset: String,
    size: (u32, u32),
    engine: lactea::Engine,
    texture: eframe::wgpu::Texture,
    view: eframe::wgpu::TextureView,
    frame: u32,
    started: f64,
}

/// Every MilkDrop layer's engine.
#[derive(Default)]
pub struct Visualizers {
    running: HashMap<u64, Visualizer>,
}

impl Visualizers {
    /// Draws this frame of every MilkDrop layer that's on, with the newest sound from `audio`; failures go into
    /// `errors` (the layer's error, shown on its card).
    pub fn update(&mut self, comp: &Composition, engine: &mut Engine, audio: &crate::audio::AudioInput, now: f64, errors: &mut HashMap<u64, String>) {
        let size = {
            let w = comp.width.clamp(16, MAX_WIDTH);
            (w, ((w as u64 * comp.height.max(1) as u64 / comp.width.max(1) as u64) as u32).max(16))
        };
        let wanted: Vec<(u64, &str)> = comp
            .layers
            .iter()
            .filter(|l| l.enabled)
            .filter_map(|l| match &l.source {
                SourceSpec::Live { format, url, .. } if format == FORMAT => Some((l.id, url.as_str())),
                _ => None,
            })
            .collect();
        self.running.retain(|id, v| wanted.iter().any(|(w, p)| w == id && *p == v.preset && v.size == size));
        if wanted.is_empty() {
            return;
        }
        // About one frame's worth of sound (MilkDrop does its own analysis of it).
        let (pcm, rate) = audio.recent(1024);
        for (id, preset) in wanted {
            if !self.running.contains_key(&id) {
                if errors.contains_key(&id) {
                    continue;
                }
                match start(engine, preset, size, rate, now) {
                    Ok(v) => {
                        self.running.insert(id, v);
                    }
                    Err(e) => {
                        errors.insert(id, e);
                        continue;
                    }
                }
            }
            let Some(v) = self.running.get_mut(&id) else { continue };
            v.engine.push_pcm_f32(&pcm, 2);
            let mut enc = engine.device().create_command_encoder(&Default::default());
            v.engine.render(&mut enc, &v.view, lactea::FrameClock { time: now - v.started, frame: v.frame });
            engine.queue().submit(Some(enc.finish()));
            v.frame = v.frame.wrapping_add(1);
            engine.set_layer_texture(id, v.texture.clone());
        }
    }
}

fn start(engine: &Engine, preset: &str, size: (u32, u32), rate: u32, now: f64) -> Result<Visualizer, String> {
    use eframe::wgpu;
    let name = std::path::Path::new(preset).file_name().map_or_else(|| preset.to_string(), |s| s.to_string_lossy().into_owned());
    let bytes = std::fs::read(preset).map_err(|e| format!("{name}: {e}"))?;
    let cfg = lactea::EngineConfig {
        width: size.0,
        height: size.1,
        sample_rate: rate.max(8000),
        auto_compat: true,
        output_format: wgpu::TextureFormat::Rgba8Unorm,
        ..Default::default()
    };
    let mut md = lactea::Engine::new(engine.device(), engine.queue(), cfg).map_err(|e| format!("{name}: {e}"))?;
    md.load_preset(&bytes, &name, lactea::Transition::Cut).map_err(|e| format!("{name}: {e}"))?;
    let texture = engine.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("milkdrop"),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    Ok(Visualizer { preset: preset.to_string(), size, engine: md, texture, view, frame: 0, started: now })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A preset file becomes a MilkDrop layer, named after the file, wherever it's dropped or opened.
    #[test]
    fn preset_files_make_milkdrop_layers() {
        assert!(is_preset("/x/Geiss - Spiral.milk") && is_preset("a.MILK2") && !is_preset("a.mov"));
        let s = crate::media::spec_for_path("/presets/Geiss - Spiral.milk");
        assert_eq!(s, SourceSpec::Live { name: "Geiss - Spiral".into(), format: FORMAT.into(), url: "/presets/Geiss - Spiral.milk".into() });
        assert!(crate::media::is_supported("/presets/x.milk"));
    }
}
