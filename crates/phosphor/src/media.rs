//! Turns files (paths natively, bytes in the browser) into engine textures.

use std::collections::HashMap;

use phosphor_engine::{Composition, Engine, SourceSpec};

#[cfg(not(target_arch = "wasm32"))]
pub mod video;

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff"];
const MOVIE_EXTS: &[&str] =
    &["mov", "mp4", "m4v", "webm", "mkv", "avi", "gif", "mpg", "mpeg", "wmv", "flv", "3gp", "mxf", "ts", "m2ts"];

/// Extensions offered in the Open… dialog.
pub fn dialog_extensions() -> Vec<&'static str> {
    IMAGE_EXTS.iter().chain(MOVIE_EXTS).copied().collect()
}

fn extension(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

pub fn is_movie(name: &str) -> bool {
    MOVIE_EXTS.contains(&extension(name).as_str())
}

pub fn is_supported(name: &str) -> bool {
    let ext = extension(name);
    IMAGE_EXTS.contains(&ext.as_str()) || MOVIE_EXTS.contains(&ext.as_str())
}

pub fn spec_for_path(path: &str) -> SourceSpec {
    spec_for(&file_name(path), Some(path.to_string()))
}

pub fn spec_for(name: &str, path: Option<String>) -> SourceSpec {
    if MOVIE_EXTS.contains(&extension(name).as_str()) {
        SourceSpec::Movie { name: name.to_string(), path }
    } else {
        SourceSpec::Image { name: name.to_string(), path }
    }
}

/// Tracks which source each layer's GPU media was built from, so files load once.
#[derive(Default)]
pub struct MediaManager {
    loaded: HashMap<u64, SourceSpec>,
    pub errors: HashMap<u64, String>,
    #[cfg(not(target_arch = "wasm32"))]
    players: HashMap<u64, video::VideoPlayer>,
}

impl MediaManager {
    /// Loads newly assigned media and drops media for layers whose source changed.
    pub fn sync(&mut self, comp: &Composition, engine: &mut Engine) {
        for layer in &comp.layers {
            if self.loaded.get(&layer.id) == Some(&layer.source) {
                continue;
            }
            engine.clear_media(layer.id);
            self.errors.remove(&layer.id);
            #[cfg(not(target_arch = "wasm32"))]
            self.players.remove(&layer.id);
            match &layer.source {
                SourceSpec::Image { path: Some(path), .. } => {
                    if let Err(e) = load_image_path(engine, layer.id, path) {
                        self.errors.insert(layer.id, e);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                SourceSpec::Movie { path: Some(path), .. } => match video::VideoPlayer::open(path) {
                    Ok(player) => {
                        self.players.insert(layer.id, player);
                    }
                    Err(e) => {
                        self.errors.insert(layer.id, e);
                    }
                },
                SourceSpec::Movie { .. } => {
                    self.errors.insert(layer.id, "Video playback in the browser is coming soon".into());
                }
                _ => {}
            }
            self.loaded.insert(layer.id, layer.source.clone());
        }
        let live: std::collections::HashSet<u64> = comp.layers.iter().map(|l| l.id).collect();
        self.loaded.retain(|id, _| live.contains(id));
        #[cfg(not(target_arch = "wasm32"))]
        self.players.retain(|id, _| live.contains(id));
    }

    /// Pulls the newest due frame from every movie into the engine. `now` is the frame clock in seconds.
    pub fn update_video(&mut self, comp: &Composition, engine: &mut Engine, now: f64) {
        #[cfg(not(target_arch = "wasm32"))]
        for layer in &comp.layers {
            let Some(player) = self.players.get_mut(&layer.id) else { continue };
            player.set_playing(layer.playing && layer.enabled);
            player.set_rate(layer.rate);
            player.set_looping(layer.looping);
            if !layer.enabled {
                continue;
            }
            if let Some(frame) = player.poll_frame(now) {
                match &frame {
                    video::VideoFrame::Nv12 { width, height, luma, luma_stride, chroma, chroma_stride } => {
                        engine.upload_nv12(layer.id, *width, *height, luma, *luma_stride, chroma, *chroma_stride);
                    }
                    video::VideoFrame::Rgba { width, height, data } => engine.upload_rgba(layer.id, *width, *height, data),
                }
                player.recycle(frame);
            }
            if let Some(err) = player.error() {
                self.errors.entry(layer.id).or_insert_with(|| err.to_string());
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (comp, engine, now);
    }
}

impl MediaManager {
    /// Codec and decoder details for a movie layer, for the Inspector.
    pub fn video_info(&self, layer: u64) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let p = self.players.get(&layer)?;
            let s = p.stats();
            let (w, h) = p.size();
            let dur = p.duration().map(|d| format!(" · {d:.1}s")).unwrap_or_default();
            let decode = if s.hardware { "hardware decode" } else { "software decode" };
            let skipped = if s.frames_skipped > 0 { format!(" · {} skipped", s.frames_skipped) } else { String::new() };
            Some(format!("{} · {w}×{h}{dur} · {decode} · {} frames{skipped}", s.codec, s.frames_decoded))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = layer;
            None
        }
    }
}

fn load_image_path(engine: &mut Engine, layer: u64, path: &str) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let img = image::open(path).map_err(|e| format!("Couldn't open {}: {e}", file_name(path)))?;
        let rgba = img.to_rgba8();
        engine.upload_rgba(layer, rgba.width(), rgba.height(), rgba.as_raw());
        Ok(())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (engine, layer);
        Err(format!("{}: loading files in the browser is coming soon", file_name(path)))
    }
}
