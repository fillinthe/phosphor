//! Turns files (paths natively, bytes in the browser) into engine textures.

use std::collections::HashMap;

use phosphor_engine::{Composition, Engine, SourceSpec};

#[cfg(not(target_arch = "wasm32"))]
pub mod live;
#[cfg(not(target_arch = "wasm32"))]
pub mod preload;
#[cfg(not(target_arch = "wasm32"))]
pub mod rhythm;
pub mod sequence;
#[cfg(not(target_arch = "wasm32"))]
pub mod video;

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "exr", "hdr", "tga", "qoi", "psd", "heic", "heif", "svg"];

/// SVG drawings are drawn this many pixels on their longer side (sharp at 4K, small enough to decode fast).
#[cfg(not(target_arch = "wasm32"))]
const SVG_SIZE: f32 = 3840.0;

/// An SVG drawing as a picture, `SVG_SIZE` on its longer side, transparent where nothing is drawn.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_svg(data: &[u8]) -> Result<image::RgbaImage, String> {
    render_svg_sized(data, SVG_SIZE)
}

/// An SVG drawing as a picture `longest` pixels on its longer side.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_svg_sized(data: &[u8], longest: f32) -> Result<image::RgbaImage, String> {
    use resvg::{tiny_skia, usvg};
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(data, &opt).map_err(|e| e.to_string())?;
    let size = tree.size();
    let scale = longest.max(1.0) / size.width().max(size.height()).max(1.0);
    let (w, h) = (((size.width() * scale).round() as u32).max(1), ((size.height() * scale).round() as u32).max(1));
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("the drawing is empty")?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia keeps premultiplied colour; pictures here are straight.
    let rgba: Vec<u8> = pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect();
    image::RgbaImage::from_raw(w, h, rgba).ok_or_else(|| "bad size".to_string())
}
/// Pictures the image library can't read: FFmpeg turns them into a PNG first (`decode_still`).
#[cfg(not(target_arch = "wasm32"))]
const FFMPEG_STILLS: &[&str] = &["psd", "heic", "heif"];
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
    MOVIE_EXTS.contains(&extension(name).as_str()) || sequence::is_sequence(name)
}

pub fn is_picture(name: &str) -> bool {
    IMAGE_EXTS.contains(&extension(name).as_str())
}

/// Opens any picture Phosphor takes: the image library for most, FFmpeg for Photoshop and HEIC files (and as a
/// second try when the library can't read a file). Off the UI thread: it can take a moment.
#[cfg(not(target_arch = "wasm32"))]
pub fn decode_still(path: &str) -> Result<image::DynamicImage, String> {
    let ext = extension(path);
    if ext == "svg" {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        return render_svg(&data).map(image::DynamicImage::ImageRgba8);
    }
    let direct = if FFMPEG_STILLS.contains(&ext.as_str()) { None } else { Some(image::open(path)) };
    match direct {
        Some(Ok(img)) => Ok(img),
        other => {
            let first = other.and_then(|r| r.err()).map(|e| e.to_string());
            let ffmpeg = crate::stream::ffmpeg_path().ok_or_else(|| first.clone().unwrap_or_else(|| "FFmpeg not found".into()))?;
            let out = std::process::Command::new(ffmpeg)
                .args(["-v", "error", "-i", path, "-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"])
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .map_err(|e| e.to_string())?;
            image::load_from_memory(&out.stdout).map_err(|e| first.unwrap_or_else(|| e.to_string()))
        }
    }
}

pub fn is_supported(name: &str) -> bool {
    // "._clip.mov": the metadata macOS writes next to every file on drives formatted for Windows, not a movie.
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if file.starts_with("._") {
        return false;
    }
    let ext = extension(name);
    IMAGE_EXTS.contains(&ext.as_str()) || MOVIE_EXTS.contains(&ext.as_str())
}

/// A clip's label for a file: its name without the extension.
pub fn clip_name(file: &str) -> String {
    if sequence::is_sequence(file) {
        return sequence::label(file);
    }
    std::path::Path::new(file).file_stem().map_or_else(|| file.to_string(), |s| s.to_string_lossy().into_owned())
}

pub fn spec_for_path(path: &str) -> SourceSpec {
    spec_for(&file_name(path), Some(path.to_string()))
}

pub fn spec_for(name: &str, path: Option<String>) -> SourceSpec {
    if is_movie(name) || is_sound_file(name) {
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
    #[cfg(any(target_os = "macos", windows))]
    ndi: HashMap<u64, crate::ndi::NdiReceiver>,
    /// Syphon layers whose source isn't there yet (the other app may start later): next retry time.
    #[cfg(target_os = "macos")]
    syphon_waiting: HashMap<u64, f64>,
    /// Display / window capture layers.
    #[cfg(target_os = "macos")]
    captures: HashMap<u64, crate::screen_capture::ScreenCapture>,
    /// Zero-copy video frames still on screen or in flight on the GPU, per (layer, outgoing): kept alive a
    /// few frames so the decoder never reuses memory the GPU is reading.
    #[cfg(not(target_arch = "wasm32"))]
    held: Held,
    /// Movies opening in the background, and clips kept ready to start at once.
    #[cfg(not(target_arch = "wasm32"))]
    pub preload: preload::Preloader,
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
        show_surface(engine, held, id, outgoing, pb);
        return;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = held;
    player.recycle(frame);
}

/// Puts a hardware frame (an NV12 IOSurface) on a layer with no copy: the texture wrappers are made once per
/// IOSurface and the last few frames are kept alive while the GPU may still read them. Video and screen capture.
#[cfg(target_os = "macos")]
fn show_surface(engine: &mut Engine, held: &mut Held, id: u64, outgoing: bool, pb: video::surface::PixelBuffer) {
    {
        let zc = held.entry((id, outgoing)).or_default();
        let key = pb.surface_id();
        let cached = zc.planes.iter().find(|(k, l, _)| *k == key && (l.width(), l.height()) == pb.size()).map(|(_, l, c)| (l.clone(), c.clone()));
        // An AYUV frame is one texture (kept in both slots of the cache entry).
        let planes = cached.or_else(|| {
            let (l, c) = if pb.is_ayuv() { pb.ayuv_texture(engine.device()).map(|t| (t.clone(), t))? } else { pb.planes(engine.device())? };
            if zc.planes.len() >= 12 {
                zc.planes.remove(0);
            }
            zc.planes.push((key, l.clone(), c.clone()));
            Some((l, c))
        });
        match planes {
            Some((tex, _)) if pb.is_ayuv() => engine.set_layer_ayuv_texture(id, outgoing, tex),
            Some((luma, chroma)) => engine.set_layer_nv12_textures(id, outgoing, luma, chroma, pb.full_range()),
            None => {}
        }
        zc.recent.push_back(pb);
        while zc.recent.len() > 3 {
            zc.recent.pop_front();
        }
    }
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
                self.captures.remove(&layer.id);
                #[cfg(any(target_os = "macos", windows))]
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
                            let result = decode_still(&path).map(|i| i.to_rgba8()).map_err(|e| format!("Couldn't open {}: {e}", file_name(&path)));
                            let _ = tx.send((id, spec, result));
                        });
                    }
                    #[cfg(target_arch = "wasm32")]
                    if let Err(e) = load_image_path(engine, layer.id, path) {
                        self.errors.insert(layer.id, e);
                    }
                }
                // A clip kept ready starts at once; anything else opens in the background (never on this thread).
                #[cfg(not(target_arch = "wasm32"))]
                SourceSpec::Movie { path: Some(path), .. } => match self.preload.take(path) {
                    Some(player) => self.add_player(layer.id, player),
                    None => self.preload.open_for(layer.id, layer.source.clone(), path.clone()),
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
                // Scope layers: the scopes put their picture on the layer (`Scopes::take_layer_pictures`).
                SourceSpec::Live { format, .. } if format == crate::scopes::SCOPE_FORMAT => {}
                #[cfg(any(target_os = "macos", windows))]
                SourceSpec::Live { format, url, .. } if format == "ndi" => match crate::ndi::NdiReceiver::connect(url) {
                    Ok(r) => {
                        self.ndi.insert(layer.id, r);
                    }
                    Err(e) => {
                        self.errors.insert(layer.id, e);
                    }
                },
                #[cfg(target_os = "macos")]
                SourceSpec::Live { format, url, .. } if format == "screen" => {
                    self.captures.insert(layer.id, crate::screen_capture::ScreenCapture::start(url));
                }
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
        self.captures.retain(|id, _| live.contains(id));
        #[cfg(any(target_os = "macos", windows))]
        self.ndi.retain(|id, _| live.contains(id));
        #[cfg(target_os = "macos")]
        self.syphon_waiting.retain(|id, _| live.contains(id));
        #[cfg(not(target_arch = "wasm32"))]
        self.outgoing.retain(|id, _| comp.layers.iter().any(|l| l.id == *id && l.transition.is_some()));
    }

    /// Pulls the newest due frame from every movie into the engine. `now` is the frame clock in seconds.
    #[cfg(not(target_arch = "wasm32"))]
    fn add_player(&mut self, layer: u64, mut player: video::VideoPlayer) {
        player.mark_started();
        if let Some(t) = self.pending_seek.remove(&layer) {
            player.seek(t);
        }
        self.players.insert(layer, player);
    }

    /// Keeps the open bank's next movie clips ready (`page` = the bank on screen), or none with preload off.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn preload_clips(&mut self, comp: &Composition, page: usize, per_page: usize) {
        let list = if comp.launch.preload { preload::candidates(comp, page, per_page) } else { Vec::new() };
        self.preload.want(&list);
    }

    pub fn update_video(&mut self, comp: &Composition, engine: &mut Engine, now: f64) {
        // Movies opened in the background: in place if the layer still wants them.
        #[cfg(not(target_arch = "wasm32"))]
        for (id, spec, result) in self.preload.opened() {
            if !comp.layers.iter().any(|l| l.id == id && l.source == spec) {
                continue;
            }
            match result {
                Ok(player) => self.add_player(id, player),
                Err(e) => {
                    self.errors.insert(id, e);
                }
            }
        }
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
            player.set_rate(layer.play_rate(player.duration(), comp.bpm) * comp.master_speed.clamp(0.25, 4.0));
            player.set_looping(layer.looping);
            let range = player.duration().and_then(|d| layer.loop_range(d, comp.bpm));
            player.set_mode(layer.direction, range);
            if !layer.enabled {
                continue;
            }
            if let Some(frame) = player.poll_frame(now) {
                show_frame(engine, &mut self.held, player, layer.id, false, frame);
            }
            // "Black at the end": the last frame goes (the next frame shown, after a seek or restart, brings it back).
            if layer.black_at_end && player.finished() {
                engine.clear_media(layer.id);
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
        for (id, c) in &self.captures {
            if let Some(e) = c.error() {
                self.errors.insert(*id, e);
            } else if let Some(pb) = c.take()
                && comp.layers.iter().any(|l| l.id == *id && l.enabled)
            {
                show_surface(engine, &mut self.held, *id, false, pb);
            }
        }
        #[cfg(any(target_os = "macos", windows))]
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
    /// Opens `path` again from disk on every layer showing it (after it was edited), at the same place in a movie.
    pub fn reload(&mut self, comp: &Composition, path: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        drop(self.preload.take(path));
        for l in &comp.layers {
            if let SourceSpec::Image { path: Some(p), .. } | SourceSpec::Movie { path: Some(p), .. } = &l.source
                && p == path
            {
                if let Some((pos, _)) = self.position(l.id) {
                    self.pending_seek.insert(l.id, pos);
                }
                self.loaded.remove(&l.id);
            }
        }
    }

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

    /// A movie layer's chapters (start in seconds, title); empty when the file has none.
    pub fn chapters(&self, layer: u64) -> Vec<(f64, String)> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return self.players.get(&layer).map(|p| p.chapters().to_vec()).unwrap_or_default();
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = layer;
            Vec::new()
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
            let skipped = if s.reverse_slow { format!("{skipped} · slow backwards (few keyframes): Media › right-click › Convert… makes it smooth") } else { skipped };
            let about = if p.about().is_empty() { String::new() } else { format!(" · {}", p.about()) };
            Some(format!("{} · {w}×{h}{dur}{about} · {decode} · {} shown, {} decoded{skipped}", s.codec, s.frames_shown, s.frames_decoded))
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

#[cfg(test)]
mod appledouble_tests {
    /// macOS metadata files next to footage on Windows-formatted drives aren't media.
    #[test]
    fn metadata_files_are_not_media() {
        assert!(super::is_supported("/Volumes/X/clip.mov"));
        assert!(!super::is_supported("/Volumes/X/._clip.mov"));
        assert!(!super::is_supported("._clip.mp4"));
    }
}

/// Photoshop, HEIC, EXR, HDR, TIFF and BMP pictures open (made here with macOS's `sips` and FFmpeg).
#[cfg(all(test, target_os = "macos"))]
#[test]
fn every_picture_format_opens() {
    let Some(ffmpeg) = crate::stream::ffmpeg_path() else { return };
    let dir = std::env::temp_dir().join(format!("phosphor-pictures-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("a.png");
    image::RgbaImage::from_pixel(48, 32, image::Rgba([200, 40, 10, 255])).save(&png).unwrap();
    let mut made = Vec::new();
    for fmt in ["psd", "heic", "tiff", "bmp"] {
        let out = dir.join(format!("a.{fmt}"));
        let ok = std::process::Command::new("sips").args(["-s", "format", fmt]).arg(&png).arg("--out").arg(&out).output().is_ok_and(|o| o.status.success());
        if ok {
            made.push(out);
        }
    }
    for fmt in ["exr", "hdr"] {
        let out = dir.join(format!("a.{fmt}"));
        let pix = if fmt == "exr" { "gbrapf32le" } else { "rgb48le" };
        let ok = std::process::Command::new(&ffmpeg).args(["-v", "error", "-y", "-i"]).arg(&png).args(["-pix_fmt", pix]).arg(&out).output().is_ok_and(|o| o.status.success());
        if ok {
            made.push(out);
        }
    }
    assert!(made.len() >= 5, "only made {made:?}");
    for p in &made {
        let img = decode_still(&p.to_string_lossy()).unwrap_or_else(|e| panic!("{}: {e}", p.display())).to_rgba8();
        assert_eq!(img.dimensions(), (48, 32), "{}", p.display());
        assert!(img.get_pixel(10, 10)[0] > 150, "{}: wrong colour {:?}", p.display(), img.get_pixel(10, 10));
        assert!(is_picture(&p.to_string_lossy()));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod svg_tests {
    #[test]
    fn svg_drawings_become_pictures() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect x="100" width="100" height="100" fill="#ff0000"/></svg>"##;
        let img = super::render_svg(svg).unwrap();
        assert_eq!((img.width(), img.height()), (3840, 1920), "sharp: 4K on the long side");
        assert_eq!(img.get_pixel(3000, 960).0, [255, 0, 0, 255], "the red half");
        assert_eq!(img.get_pixel(500, 960).0[3], 0, "transparent where nothing's drawn");
        assert!(super::is_supported("logo.svg"));
        assert!(super::render_svg(b"not svg").is_err());
    }
}
