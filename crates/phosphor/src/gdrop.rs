//! G-Drop visualizers (from Gamp, the same author's Winamp-style player: crates/gamp-vis) as layers: a layer whose
//! source is `Live { format: "gdrop", url: preset name }` ("" = it changes preset by itself every 20-40 s) is drawn on
//! the CPU on a thread of its own, fed with the sound Phosphor is listening to; the newest finished picture goes up
//! as the layer's picture each frame. Only layers that are on are drawn.

use std::collections::HashMap;
use std::sync::mpsc;

use phosphor_engine::{Composition, Engine, SourceSpec};

/// The live-source format that marks a G-Drop layer.
pub const FORMAT: &str = "gdrop";
/// Drawn this wide (a soft, feedback look: the layer scales it up). About 2.5 ms a frame on one core.
const WIDTH: usize = 640;
/// Samples per frame handed over (and the spectrum's size).
const BLOCK: usize = 1024;

/// Every preset's name, in G-Drop's order.
pub fn presets() -> impl Iterator<Item = &'static str> {
    gamp_vis::PRESETS.iter().map(|p| p.name)
}

/// A layer source for one preset; `None` = changes preset by itself.
pub fn source(preset: Option<&str>) -> SourceSpec {
    let name = preset.map_or_else(|| "G-Drop (changing)".to_string(), |p| format!("G-Drop · {p}"));
    SourceSpec::Live { name, format: FORMAT.into(), url: preset.unwrap_or_default().into() }
}

struct Runner {
    preset: String,
    size: (usize, usize),
    /// The newest sound (mono samples, rate). Dropped = the thread ends.
    sound: mpsc::SyncSender<(Vec<f32>, u32)>,
    frames: mpsc::Receiver<Vec<u8>>,
}

/// Every G-Drop layer's thread.
#[derive(Default)]
pub struct GDrops {
    running: HashMap<u64, Runner>,
}

impl GDrops {
    /// Hands this frame's sound to every G-Drop layer that's on and shows the newest picture each has finished.
    pub fn update(&mut self, comp: &Composition, engine: &mut Engine, audio: &crate::audio::AudioInput) {
        let size = (WIDTH, (WIDTH as u64 * comp.height.max(1) as u64 / comp.width.max(1) as u64).clamp(16, 2048) as usize);
        let wanted: Vec<(u64, &str)> = comp
            .layers
            .iter()
            .filter(|l| l.enabled)
            .filter_map(|l| match &l.source {
                SourceSpec::Live { format, url, .. } if format == FORMAT => Some((l.id, url.as_str())),
                _ => None,
            })
            .collect();
        self.running.retain(|id, r| wanted.iter().any(|(w, p)| w == id && *p == r.preset && r.size == size));
        if wanted.is_empty() {
            return;
        }
        let (pcm, rate) = audio.recent(BLOCK);
        let mono: Vec<f32> = pcm.chunks(2).map(|s| (s[0] + s.get(1).copied().unwrap_or(s[0])) * 0.5).collect();
        for (id, preset) in wanted {
            let r = self.running.entry(id).or_insert_with(|| start(preset, size));
            // Busy with the last frame: this sound is skipped (the next is newer anyway).
            let _ = r.sound.try_send((mono.clone(), rate));
            if let Some(rgba) = r.frames.try_iter().last() {
                engine.upload_rgba(id, size.0 as u32, size.1 as u32, &rgba);
            }
        }
    }
}

fn start(preset: &str, size: (usize, usize)) -> Runner {
    let (sound, sound_rx) = mpsc::sync_channel::<(Vec<f32>, u32)>(1);
    let (frames_tx, frames) = mpsc::sync_channel::<Vec<u8>>(2);
    let index = gamp_vis::PRESETS.iter().position(|p| p.name == preset);
    let _ = std::thread::Builder::new().name("phosphor-gdrop".into()).spawn(move || {
        let mut g = gamp_vis::GDrop::new(size.0, size.1);
        if let Some(i) = index {
            g.auto = false;
            g.select(i, true);
        }
        let fft = rustfft::FftPlanner::<f32>::new().plan_fft_forward(BLOCK);
        let window: Vec<f32> = (0..BLOCK).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / BLOCK as f32).cos()).collect();
        let mut buf = vec![rustfft::num_complex::Complex32::default(); BLOCK];
        let mut last = std::time::Instant::now();
        while let Ok((wave, rate)) = sound_rx.recv() {
            let dt = last.elapsed().as_secs_f32();
            last = std::time::Instant::now();
            for (i, b) in buf.iter_mut().enumerate() {
                *b = rustfft::num_complex::Complex32::new(wave.get(i).copied().unwrap_or(0.0) * window[i], 0.0);
            }
            fft.process(&mut buf);
            let spectrum: Vec<f32> = buf[..BLOCK / 2].iter().map(|c| c.norm() * 2.0 / BLOCK as f32).collect();
            let rgba = to_rgba(g.render(&wave, &spectrum, rate.max(8000), dt));
            if let Err(mpsc::TrySendError::Disconnected(_)) = frames_tx.try_send(rgba) {
                return;
            }
        }
    });
    Runner { preset: preset.to_string(), size, sound, frames }
}

/// G-Drop's 0xAARRGGBB pixels as RGBA bytes.
fn to_rgba(px: &[u32]) -> Vec<u8> {
    px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 0xFF]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_make_layers_and_pixels_turn_into_rgba() {
        assert_eq!(presets().count(), gamp_vis::PRESETS.len());
        let first = presets().next().unwrap();
        assert_eq!(source(Some(first)), SourceSpec::Live { name: format!("G-Drop · {first}"), format: FORMAT.into(), url: first.into() });
        assert_eq!(to_rgba(&[0xFF11_2233]), vec![0x11, 0x22, 0x33, 0xFF]);
    }

    /// A running G-Drop draws something from a tone, on its own thread, and stops when it's dropped.
    #[test]
    fn a_gdrop_thread_draws_from_sound() {
        let r = start(presets().next().unwrap(), (64, 36));
        let tone: Vec<f32> = (0..BLOCK).map(|k| (k as f32 * 0.07).sin() * 0.6).collect();
        let mut lit = 0;
        for _ in 0..60 {
            let _ = r.sound.send((tone.clone(), 48_000));
            if let Ok(f) = r.frames.recv_timeout(std::time::Duration::from_secs(2)) {
                assert_eq!(f.len(), 64 * 36 * 4);
                lit = f.chunks(4).filter(|p| p[0].max(p[1]).max(p[2]) > 16).count();
            }
        }
        assert!(lit > 0, "nothing drawn");
    }
}
