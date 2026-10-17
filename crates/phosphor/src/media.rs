//! Turns files (paths natively, bytes in the browser) into engine textures.

use std::collections::HashMap;

use phosphor_engine::{Composition, Engine, SourceSpec};

#[cfg(not(target_arch = "wasm32"))]
pub mod live;
#[cfg(not(target_arch = "wasm32"))]
pub mod rhythm;
#[cfg(not(target_arch = "wasm32"))]
pub mod video;

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff"];
const MOVIE_EXTS: &[&str] = &["mov", "mp4", "m4v", "webm", "mkv", "avi", "gif", "mpg", "mpeg", "wmv", "flv", "3gp", "mxf", "ts", "m2ts"];
/// Sound files play as clips with an empty picture (their sound can drive the visuals: Audio panel → Clip sound).
const SOUND_EXTS: &[&str] = &["mp3", "wav", "aac", "m4a", "flac", "ogg", "aiff", "aif", "opus"];

/// A sound-only file (plays as a clip with no picture).
pub fn is_sound_file(name: &str) -> bool {
    SOUND_EXTS.contains(&extension(name).as_str())
}

/// Extensions offered in the Open… dialog.
pub fn dialog_extensions() -> Vec<&'static str> {
    IMAGE_EXTS.iter().chain(MOVIE_EXTS).chain(SOUND_EXTS).copied().collect()
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

/// A clip's label for a file: its name without the extension.
pub fn clip_name(file: &str) -> String {
    std::path::Path::new(file).file_stem().map_or_else(|| file.to_string(), |s| s.to_string_lossy().into_owned())
}

pub fn spec_for_path(path: &str) -> SourceSpec {
    spec_for(&file_name(path), Some(path.to_string()))
}

pub fn spec_for(name: &str, path: Option<String>) -> SourceSpec {
    if MOVIE_EXTS.contains(&extension(name).as_str()) || is_sound_file(name) {
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
    /// Images decoding on background threads: (layer, source, result).
    #[cfg(not(target_arch = "wasm32"))]
    decoded: Option<(std::sync::mpsc::Sender<Decoded>, std::sync::mpsc::Receiver<Decoded>)>,
    /// Players for the outgoing side of a transition; they keep playing until it ends.
    #[cfg(not(target_arch = "wasm32"))]
    outgoing: HashMap<u64, video::VideoPlayer>,
    /// Syphon In connections per layer.
    #[cfg(target_os = "macos")]
    syphon: HashMap<u64, crate::syphon::SyphonClient>,
    /// Seeks waiting for a layer's video to open (crash recovery restores positions this way).
    #[cfg(not(target_arch = "wasm32"))]
    pub pending_seek: HashMap<u64, f64>,
    /// NDI In receivers per layer.
    #[cfg(target_os = "macos")]
    ndi: HashMap<u64, crate::ndi::NdiReceiver>,
    /// Syphon layers whose source isn't there yet (the other app may start later): next retry time.
    #[cfg(target_os = "macos")]
    syphon_waiting: HashMap<u64, f64>,
    /// Zero-copy video frames still on screen or in flight on the GPU, per (layer, outgoing): kept alive a
    /// few frames so the decoder never reuses memory the GPU is reading.
    #[cfg(not(target_arch = "wasm32"))]
    held: Held,
}

/// Per (layer, outgoing): recent zero-copy frames (kept alive while the GPU may read them) and the GPU
/// wrappers made for each decoder buffer (reused when the decoder cycles back to it: no per-frame setup).
#[cfg(target_os = "macos")]
#[derive(Default)]
struct ZeroCopy {
    recent: std::collections::VecDeque<video::surface::PixelBuffer>,
    planes: Vec<(usize, wgpu::Texture, wgpu::Texture)>,
}
#[cfg(target_os = "macos")]
type Held = HashMap<(u64, bool), ZeroCopy>;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
type Held = ();

/// Puts a decoded frame on a layer (or its outgoing side of a transition): uploads pixels or GPU blocks, or
/// wraps a hardware frame's memory with no copy; reusable buffers go back to the player.
#[cfg(not(target_arch = "wasm32"))]
fn show_frame(engine: &mut Engine, held: &mut Held, player: &video::VideoPlayer, id: u64, outgoing: bool, frame: video::VideoFrame) {
    match &frame {
        video::VideoFrame::Nv12 { width, height, luma, luma_stride, chroma, chroma_stride } => {
            if outgoing {
                engine.upload_nv12_outgoing(id, *width, *height, luma, *luma_stride, chroma, *chroma_stride);
            } else {
                engine.upload_nv12(id, *width, *height, luma, *luma_stride, chroma, *chroma_stride);
            }
        }
        video::VideoFrame::Rgba { width, height, data } => {
            if outgoing {
                engine.upload_rgba_outgoing(id, *width, *height, data);
            } else {
                engine.upload_rgba(id, *width, *height, data);
            }
        }
        video::VideoFrame::Compressed { width, height, format, data, alpha } => engine.upload_compressed(id, outgoing, *width, *height, *format, data, alpha.as_deref()),
        #[cfg(target_os = "macos")]
        video::VideoFrame::Surface(_) => {}
    }
    #[cfg(target_os = "macos")]
    if let video::VideoFrame::Surface(pb) = frame {
        let zc = held.entry((id, outgoing)).or_default();
        let key = pb.surface_id();
        let cached = zc.planes.iter().find(|(k, l, _)| *k == key && (l.width(), l.height()) == pb.size()).map(|(_, l, c)| (l.clone(), c.clone()));
        let planes = cached.or_else(|| {
            let (l, c) = pb.planes(engine.device())?;
            if zc.planes.len() >= 12 {
                zc.planes.remove(0);
            }
            zc.planes.push((key, l.clone(), c.clone()));
            Some((l, c))
        });
        if let Some((luma, chroma)) = planes {
            engine.set_layer_nv12_textures(id, outgoing, luma, chroma, pb.full_range());
        }
        zc.recent.push_back(pb);
        while zc.recent.len() > 3 {
            zc.recent.pop_front();
        }
        return;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = held;
    player.recycle(frame);
}

impl MediaManager {
    /// Loads newly assigned media and drops media for layers whose source changed.
    pub fn sync(&mut self, comp: &Composition, engine: &mut Engine) {
        for layer in &comp.layers {
            if self.loaded.get(&layer.id) == Some(&layer.source) {
                continue;
            }
            // A transition keeps the old source alive as the outgoing picture.
            let transitioning = layer.transition.as_ref().is_some_and(|t| self.loaded.get(&layer.id) == Some(&t.from));
            if transitioning {
                engine.begin_transition(layer.id);
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(p) = self.players.remove(&layer.id) {
                    self.outgoing.insert(layer.id, p);
                }
            } else if !(matches!(self.loaded.get(&layer.id), Some(SourceSpec::Text(_))) && matches!(layer.source, SourceSpec::Text(_))) {
                // (Retyped text keeps showing the old words until the new picture is ready: no flash.)
                engine.clear_media(layer.id);
                #[cfg(target_os = "macos")]
                self.syphon.remove(&layer.id);
                #[cfg(target_os = "macos")]
                self.syphon_waiting.remove(&layer.id);
                #[cfg(target_os = "macos")]
                self.ndi.remove(&layer.id);
                #[cfg(not(target_arch = "wasm32"))]
                self.players.remove(&layer.id);
                #[cfg(not(target_arch = "wasm32"))]
                self.outgoing.remove(&layer.id);
            }
            self.errors.remove(&layer.id);
            match &layer.source {
                SourceSpec::Image { path: Some(path), .. } => {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        // Decode off the UI thread so big images never hitch playback.
                        let (tx, _) = self.decoded.get_or_insert_with(std::sync::mpsc::channel);
                        let tx = tx.clone();
                        let (id, spec, path) = (layer.id, layer.source.clone(), path.clone());
                        std::thread::spawn(move || {
                            let result = image::open(&path).map(|i| i.to_rgba8()).map_err(|e| format!("Couldn't open {}: {e}", file_name(&path)));
                            let _ = tx.send((id, spec, result));
                        });
                    }
                    #[cfg(target_arch = "wasm32")]
                    if let Err(e) = load_image_path(engine, layer.id, path) {
                        self.errors.insert(layer.id, e);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                SourceSpec::Movie { path: Some(path), .. } => match video::VideoPlayer::open(path) {
                    Ok(mut player) => {
                        if let Some(t) = self.pending_seek.remove(&layer.id) {
                            player.seek(t);
                        }
                        self.players.insert(layer.id, player);
                    }
                    Err(e) => {
                        self.errors.insert(layer.id, e);
                    }
                },
                SourceSpec::Movie { .. } => {
                    self.errors.insert(layer.id, "Video playback in the browser is coming soon".into());
                }
                SourceSpec::Text(style) => {
                    let (w, h) = (comp.width, comp.height);
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let (tx, _) = self.decoded.get_or_insert_with(std::sync::mpsc::channel);
                        let tx = tx.clone();
                        let (id, spec, style) = (layer.id, layer.source.clone(), style.clone());
                        std::thread::spawn(move || {
                            let _ = tx.send((id, spec, crate::text::render(&style, w, h)));
                        });
                    }
                    #[cfg(target_arch = "wasm32")]
                    match crate::text::render(style, w, h) {
                        Ok(img) => engine.upload_rgba(layer.id, img.width(), img.height(), img.as_raw()),
                        Err(e) => {
                            self.errors.insert(layer.id, e);
                        }
                    }
                }
                #[cfg(target_os = "macos")]
                SourceSpec::Live { format, url, .. } if format == "ndi" => match crate::ndi::NdiReceiver::connect(url) {
                    Ok(r) => {
                        self.ndi.insert(layer.id, r);
                    }
                    Err(e) => {
                        self.errors.insert(layer.id, e);
                    }
                },
                #[cfg(target_os = "macos")]
                SourceSpec::Live { format, .. } if format == "syphon" => {
                    // Connected (with retries) in update_video.
                    self.syphon_waiting.insert(layer.id, 0.0);
                }
                #[cfg(not(target_arch = "wasm32"))]
                SourceSpec::Live { name, format, url } => match live::open(name, format, url) {
                    Ok(player) => {
                        self.players.insert(layer.id, player);
                    }
                    Err(e) => {
                        self.errors.insert(layer.id, e);
                    }
                },
                _ => {}
            }
            self.loaded.insert(layer.id, layer.source.clone());
        }
        let live: std::collections::HashSet<u64> = comp.layers.iter().map(|l| l.id).collect();
        self.loaded.retain(|id, _| live.contains(id));
        #[cfg(not(target_arch = "wasm32"))]
        self.players.retain(|id, _| live.contains(id));
        #[cfg(target_os = "macos")]
        self.held.retain(|(id, _), _| live.contains(id));
        #[cfg(target_os = "macos")]
        self.syphon.retain(|id, _| live.contains(id));
        #[cfg(target_os = "macos")]
        self.ndi.retain(|id, _| live.contains(id));
        #[cfg(target_os = "macos")]
        self.syphon_waiting.retain(|id, _| live.contains(id));
        #[cfg(not(target_arch = "wasm32"))]
        self.outgoing.retain(|id, _| comp.layers.iter().any(|l| l.id == *id && l.transition.is_some()));
    }

    /// Pulls the newest due frame from every movie into the engine. `now` is the frame clock in seconds.
    pub fn update_video(&mut self, comp: &Composition, engine: &mut Engine, now: f64) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((_, rx)) = &self.decoded {
            for (id, spec, result) in rx.try_iter() {
                // Only if the layer still shows that image.
                if !comp.layers.iter().any(|l| l.id == id && l.source == spec) {
                    continue;
                }
                match result {
                    Ok(img) => engine.upload_rgba(id, img.width(), img.height(), img.as_raw()),
                    Err(e) => {
                        self.errors.insert(id, e);
                    }
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        for layer in &comp.layers {
            let Some(player) = self.players.get_mut(&layer.id) else { continue };
            // During a transition the sound crossfades too (the outgoing player gets the rest below).
            let fade_in = layer.transition.as_ref().map_or(1.0, |t| t.progress(now as f32));
            player.set_volume(comp.sound_gain(layer) * fade_in);
            player.set_playing(layer.playing && layer.enabled);
            player.set_rate(layer.play_rate(player.duration(), comp.bpm));
            player.set_looping(layer.looping);
            let range = player.duration().and_then(|d| layer.loop_range(d, comp.bpm));
            player.set_mode(layer.direction, range);
            if !layer.enabled {
                continue;
            }
            if let Some(frame) = player.poll_frame(now) {
                show_frame(engine, &mut self.held, player, layer.id, false, frame);
            }
            if let Some(err) = player.error() {
                self.errors.entry(layer.id).or_insert_with(|| err.to_string());
            }
        }
        #[cfg(target_os = "macos")]
        for (id, next) in self.syphon_waiting.iter_mut() {
            if now < *next {
                continue;
            }
            *next = now + 1.0;
            let Some(SourceSpec::Live { url, .. }) = comp.layers.iter().find(|l| l.id == *id).map(|l| &l.source) else { continue };
            match crate::syphon::SyphonClient::connect(url, engine.device()) {
                Ok(client) => {
                    self.errors.remove(id);
                    self.syphon.insert(*id, client);
                }
                Err(e) => {
                    self.errors.insert(*id, format!("{e} (waiting for it…)"));
                }
            }
        }
        #[cfg(target_os = "macos")]
        self.syphon_waiting.retain(|id, _| !self.syphon.contains_key(id));
        #[cfg(target_os = "macos")]
        for (id, client) in &self.syphon {
            if comp.layers.iter().any(|l| l.id == *id && l.enabled)
                && let Some(tex) = client.new_frame(engine.device())
            {
                engine.set_layer_texture(*id, tex);
            }
        }
        #[cfg(target_os = "macos")]
        for (id, r) in &self.ndi {
            if let Some((w, h, rgba)) = r.take()
                && comp.layers.iter().any(|l| l.id == *id && l.enabled)
            {
                engine.upload_rgba(*id, w, h, &rgba);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        for (id, player) in &mut self.outgoing {
            let gain = comp.layers.iter().find(|l| l.id == *id).map_or(0.0, |l| {
                comp.sound_gain(l) * (1.0 - l.transition.as_ref().map_or(1.0, |t| t.progress(now as f32)))
            });
            player.set_volume(gain);
            player.set_playing(true);
            if let Some(frame) = player.poll_frame(now) {
                show_frame(engine, &mut self.held, player, *id, true, frame);
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (comp, engine, now);
    }
}

impl MediaManager {
    /// (position, duration) in seconds of a layer's video, for the scrub bar.
    pub fn position(&self, layer: u64) -> Option<(f64, f64)> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let p = self.players.get(&layer)?;
            return Some((p.position(), p.duration()?));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = layer;
            None
        }
    }

    /// Jumps a layer's video to `t` seconds (or remembers it until the video has opened).
    pub fn seek(&mut self, layer: u64, t: f64) {
        #[cfg(not(target_arch = "wasm32"))]
        match self.players.get_mut(&layer) {
            Some(p) => p.seek(t),
            None => {
                self.pending_seek.insert(layer, t);
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (layer, t);
    }

    /// Every playing video's position, for crash recovery.
    pub fn positions(&self) -> Vec<(u64, f64)> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return self.players.iter().filter(|(_, p)| p.duration().is_some()).map(|(id, p)| (*id, (p.position() * 10.0).round() / 10.0)).collect();
        }
        #[cfg(target_arch = "wasm32")]
        Vec::new()
    }

    /// How many times a layer's movie has played through (None = not a movie).
    pub fn passes(&self, layer: u64) -> Option<u64> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return self.players.get(&layer).filter(|p| p.duration().is_some()).map(|p| p.passes());
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = layer;
            None
        }
    }

    /// The layer's movie has a sound track.
    /// Any movie playing with an audio track (the analyser can listen to the clips).
    pub fn any_sound(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return self.players.values().any(|p| p.has_sound());
        }
        #[cfg(target_arch = "wasm32")]
        false
    }

    pub fn has_sound(&self, layer: u64) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return self.players.get(&layer).is_some_and(|p| p.has_sound());
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = layer;
            false
        }
    }

    /// Codec and decoder details for a movie layer, for the Inspector.
    pub fn video_info(&self, layer: u64) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let p = self.players.get(&layer)?;
            let s = p.stats();
            let (w, h) = p.size();
            let dur = p.duration().map(|d| format!(" · {d:.1}s")).unwrap_or_default();
            if (w, h) == (0, 0) {
                return Some(format!("sound only{dur}{}", if p.has_sound() { "" } else { " · no sound output" }));
            }
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

#[cfg(not(target_arch = "wasm32"))]
type Decoded = (u64, SourceSpec, Result<image::RgbaImage, String>);

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
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
