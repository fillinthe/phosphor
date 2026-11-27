//! Thumbnails for the media bin, built on a background thread (images via `image`, movies via
//! an FFmpeg frame grab) and uploaded as egui textures on first use. An `https://` address works too (online
//! libraries): those are fetched on threads of their own so slow sites never hold up the media bin's.

use std::collections::{HashMap, HashSet};

use eframe::egui;

pub const THUMB_W: u32 = 192;

/// Between a path and its size in a `get_sized` key.
const SIZE_SEP: char = '\u{1}';

/// Pictures kept on the graphics card at most (bytes, 4 a pixel). Past it the ones unused longest go (made again from
/// the disk cache when they're shown again). A long show browsing a big library kept every thumbnail it ever showed.
const CACHE_BYTES: usize = 128 << 20;

/// One kept picture: its texture (None = couldn't be made), its size in bytes and the frame it was last asked for.
struct Kept {
    handle: Option<egui::TextureHandle>,
    bytes: usize,
    used: u64,
}

impl Kept {
    fn new(handle: Option<egui::TextureHandle>, used: u64) -> Self {
        // (A picture that couldn't be made still takes a little: the key.)
        let bytes = handle.as_ref().map_or(64, |h| h.size()[0] * h.size()[1] * 4);
        Self { handle, bytes, used }
    }
}

/// Keys to drop (unused longest first) so `entries` (key, bytes, last used) fit in `cap`; never one used in frame
/// `now` (it's on screen).
fn to_evict(mut entries: Vec<(&str, usize, u64)>, cap: usize, now: u64) -> Vec<String> {
    let mut total: usize = entries.iter().map(|e| e.1).sum();
    if total <= cap {
        return Vec::new();
    }
    entries.sort_by_key(|e| e.2);
    let mut out = Vec::new();
    // Down to three quarters, so a cache at its cap doesn't evict a little every frame.
    for (key, bytes, used) in entries {
        if total <= cap / 4 * 3 || used >= now {
            break;
        }
        total -= bytes;
        out.push(key.to_string());
    }
    out
}

pub struct Thumbs {
    cache: HashMap<String, Kept>,
    /// Bytes of every kept picture, and the frame count (`poll`): what "last used" is measured in.
    bytes: usize,
    frame: u64,
    pending: HashSet<String>,
    #[cfg(not(target_arch = "wasm32"))]
    worker: native::Worker,
    /// Started on the first web picture.
    #[cfg(not(target_arch = "wasm32"))]
    web: Option<native::Worker>,
}

impl Thumbs {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            bytes: 0,
            frame: 0,
            pending: HashSet::new(),
            #[cfg(not(target_arch = "wasm32"))]
            worker: native::Worker::new(1),
            #[cfg(not(target_arch = "wasm32"))]
            web: None,
        }
    }

    /// The thumbnail texture for `path`, requesting it on first call. `None` while loading or if
    /// it couldn't be made.
    pub fn get(&mut self, path: &str) -> Option<egui::TextureId> {
        if let Some(entry) = self.cache.get_mut(path) {
            entry.used = self.frame;
            return entry.handle.as_ref().map(|h| h.id());
        }
        if self.pending.insert(path.to_string()) {
            #[cfg(not(target_arch = "wasm32"))]
            if path.starts_with("https://") {
                self.web.get_or_insert_with(|| native::Worker::new(4)).request(path.to_string());
            } else {
                self.worker.request(path.to_string());
            }
        }
        None
    }

    /// A picture of `path` up to `max` pixels on its long side (stage photos), made on the same background thread.
    pub fn get_sized(&mut self, path: &str, max: u32) -> Option<egui::TextureId> {
        if max == THUMB_W {
            return self.get(path);
        }
        self.get(&format!("{path}{SIZE_SEP}{max}"))
    }

    /// A thumbnail made right here (text clips): `build` runs once per key and the picture is kept.
    pub fn get_or_build(&mut self, ctx: &egui::Context, key: &str, build: impl FnOnce() -> Option<egui::ColorImage>) -> Option<egui::TextureId> {
        if !self.cache.contains_key(key) {
            let handle = build().map(|img| ctx.load_texture(format!("thumb:{key}"), img, egui::TextureOptions::LINEAR));
            self.keep(key.to_string(), handle);
        }
        let entry = self.cache.get_mut(key)?;
        entry.used = self.frame;
        entry.handle.as_ref().map(|h| h.id())
    }

    /// A loaded thumbnail's width / height (to fit it in a tile without stretching).
    pub fn aspect(&self, path: &str) -> Option<f32> {
        let size = self.cache.get(path)?.handle.as_ref()?.size();
        (size[1] > 0).then(|| size[0] as f32 / size[1] as f32)
    }

    /// `aspect` of a `get_sized` picture.
    pub fn aspect_sized(&self, path: &str, max: u32) -> Option<f32> {
        if max == THUMB_W { self.aspect(path) } else { self.aspect(&format!("{path}{SIZE_SEP}{max}")) }
    }

    pub fn is_pending(&self, path: &str) -> bool {
        self.pending.contains(path)
    }

    /// Uploads finished thumbnails, and lets the ones unused longest go past `CACHE_BYTES` (only then: a sum the
    /// cache keeps as it goes, no walk). Call once per frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        #[cfg(not(target_arch = "wasm32"))]
        for (path, image) in self.worker.finished().into_iter().chain(self.web.as_ref().map(|w| w.finished()).unwrap_or_default()) {
            self.pending.remove(&path);
            let handle = image.map(|img| ctx.load_texture(format!("thumb:{path}"), img, egui::TextureOptions::LINEAR));
            self.keep(path, handle);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = ctx;
        if self.bytes > CACHE_BYTES {
            // (Last frame's pictures are the ones on screen now: this frame's haven't been asked for yet.)
            let gone = to_evict(self.cache.iter().map(|(k, e)| (k.as_str(), e.bytes, e.used)).collect(), CACHE_BYTES, self.frame - 1);
            for key in gone {
                self.forget(&key);
            }
        }
    }

    fn keep(&mut self, key: String, handle: Option<egui::TextureHandle>) {
        let kept = Kept::new(handle, self.frame);
        self.bytes += kept.bytes;
        if let Some(old) = self.cache.insert(key, kept) {
            self.bytes -= old.bytes;
        }
    }

    pub fn forget(&mut self, path: &str) {
        if let Some(old) = self.cache.remove(path) {
            self.bytes -= old.bytes;
        }
        self.pending.remove(path);
    }

    /// `forget` for a `get_sized` picture.
    pub fn forget_sized(&mut self, path: &str, max: u32) {
        if max == THUMB_W {
            self.forget(path);
        } else {
            self.forget(&format!("{path}{SIZE_SEP}{max}"));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::process::Stdio;
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::sync::{Arc, Mutex};

    use eframe::egui;

    use super::{SIZE_SEP, THUMB_W};

    pub struct Worker {
        req: Sender<String>,
        done: Receiver<(String, Option<egui::ColorImage>)>,
    }

    impl Worker {
        /// `threads` share the queue (web pictures: several at once).
        pub fn new(threads: usize) -> Self {
            let (req, req_rx) = channel::<String>();
            let (done_tx, done) = channel();
            let queue = Arc::new(Mutex::new(req_rx));
            for _ in 0..threads.max(1) {
                let (queue, done_tx) = (queue.clone(), done_tx.clone());
                std::thread::Builder::new()
                    .name("phosphor-thumbs".into())
                    .spawn(move || {
                        loop {
                            let Ok(path) = queue.lock().map_err(|_| ()).and_then(|q| q.recv().map_err(|_| ())) else { return };
                            let _ = done_tx.send((path.clone(), build(&path)));
                        }
                    })
                    .expect("thumbnail thread");
            }
            Self { req, done }
        }

        pub fn request(&self, path: String) {
            let _ = self.req.send(path);
        }

        pub fn finished(&self) -> Vec<(String, Option<egui::ColorImage>)> {
            self.done.try_iter().collect()
        }
    }

    pub(super) fn build(key: &str) -> Option<egui::ColorImage> {
        let (path, size) = match key.split_once(SIZE_SEP) {
            Some((p, s)) => (p, s.parse().unwrap_or(THUMB_W)),
            None => (key, THUMB_W),
        };
        let name = path.to_ascii_lowercase();
        // A file's thumbnail is kept on this Mac's own disk (keyed by path, size and modification time): the next
        // launch reads that instead of decoding the file again from an external drive while a show plays from it.
        let cached = (!path.starts_with("https://")).then(|| cache_file(path, size)).flatten();
        // The file can't be looked at (its drive is away): the last thumbnail made of it still shows.
        let last = if cached.is_none() && !path.starts_with("https://") { last_known(path, size) } else { None };
        if let Some(c) = cached.as_ref().or(last.as_ref())
            && let Ok(img) = image::open(c)
        {
            // Used today: the cache's clean-up goes by when a thumbnail was last used, not when it was made.
            if let Ok(f) = std::fs::File::options().write(true).open(c) {
                let _ = f.set_modified(std::time::SystemTime::now());
            }
            if cached.is_some() {
                remember(path, size, c);
            }
            let rgba = img.to_rgba8();
            return Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()));
        }
        let img = if path.starts_with("https://") {
            let bytes = crate::online::get_bytes(path).ok()?;
            if path.split('?').next().is_some_and(|p| p.ends_with(".svg")) {
                image::DynamicImage::ImageRgba8(crate::media::render_svg_sized(&bytes, size as f32).ok()?)
            } else {
                image::load_from_memory(&bytes).ok()?.thumbnail(size, size)
            }
        } else if crate::media::is_movie(&name) {
            let ffmpeg = crate::stream::ffmpeg_path()?;
            // An image sequence: where its numbers start and its frame rate, before the input.
            let start = crate::media::sequence::first_number(path).unwrap_or(0).to_string();
            let sequence: &[&str] = if crate::media::sequence::is_sequence(path) { &["-start_number", &start, "-framerate", crate::media::sequence::RATE] } else { &[] };
            let out = crate::os::command(ffmpeg)
                .args(["-v", "error"])
                .args(sequence)
                .args(["-ss", "0.5", "-i", path, "-frames:v", "1", "-vf", &format!("scale={size}:-2"), "-f", "image2pipe", "-vcodec", "png", "-"])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            image::load_from_memory(&out.stdout).ok()?
        } else {
            crate::media::decode_still(path).ok()?.thumbnail(size, size)
        };
        let rgba = img.to_rgba8();
        if let Some(c) = cached {
            if let Some(dir) = c.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            // Written beside, then renamed: a half-written file is never read back.
            let part = c.with_extension("part");
            if rgba.save_with_format(&part, image::ImageFormat::Png).is_ok() && std::fs::rename(&part, &c).is_ok() {
                remember(path, size, &c);
            }
        }
        Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
    }

    /// The folder of thumbnails kept on this computer.
    pub(crate) fn cache_dir() -> Option<std::path::PathBuf> {
        crate::cache_store::folder("thumbs")
    }

    /// The note that says which thumbnail was last made of `path` (by the path alone: it's read when the file itself
    /// can't be, with its drive away).
    fn last_note(dir: &std::path::Path, path: &str, size: u32) -> std::path::PathBuf {
        let h = phosphor_engine::stable_hash(&[path.as_bytes(), &size.to_le_bytes()]);
        dir.join("last").join(format!("{h:016x}"))
    }

    fn remember(path: &str, size: u32, thumb: &std::path::Path) {
        if let (Some(dir), Some(name)) = (thumb.parent(), thumb.file_name()) {
            remember_in(dir, path, size, &name.to_string_lossy());
        }
    }

    pub(super) fn remember_in(dir: &std::path::Path, path: &str, size: u32, name: &str) {
        let note = last_note(dir, path, size);
        if std::fs::read_to_string(&note).is_ok_and(|n| n == name) {
            return;
        }
        if let Some(d) = note.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(note, name);
    }

    fn last_known(path: &str, size: u32) -> Option<std::path::PathBuf> {
        last_known_in(&cache_dir()?, path, size)
    }

    pub(super) fn last_known_in(dir: &std::path::Path, path: &str, size: u32) -> Option<std::path::PathBuf> {
        let name = std::fs::read_to_string(last_note(dir, path, size)).ok()?;
        let thumb = dir.join(name.trim());
        thumb.is_file().then_some(thumb)
    }

    /// Where a file's thumbnail at `size` is kept: Phosphor's cache folder, named by a hash of the path, the file's
    /// length and modification time (an edited file gets a new one). None when the file can't be looked at.
    pub(super) fn cache_file(path: &str, size: u32) -> Option<std::path::PathBuf> {
        // (A sequence pattern isn't a file: its first frame stands in.)
        let probe = if crate::media::sequence::is_sequence(path) { crate::media::sequence::members(path).into_iter().next().map(|p| p.to_string_lossy().into_owned())? } else { path.to_string() };
        let meta = std::fs::metadata(&probe).ok()?;
        let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        let h = phosphor_engine::stable_hash(&[path.as_bytes(), &meta.len().to_le_bytes(), &modified.to_le_bytes(), &size.to_le_bytes()]);
        Some(cache_dir()?.join(format!("{h:016x}.png")))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::cache_dir;

#[cfg(test)]
mod evict_tests {
    #[test]
    fn the_pictures_unused_longest_go_first_never_those_on_screen() {
        let mb = 1 << 20;
        let entries = vec![("a", 40 * mb, 5), ("b", 40 * mb, 1), ("c", 40 * mb, 9), ("d", 40 * mb, 3)];
        assert!(super::to_evict(entries.clone(), 200 * mb, 9).is_empty(), "under the cap: nothing");
        assert_eq!(super::to_evict(entries.clone(), 100 * mb, 9), vec!["b", "d", "a"], "oldest first, down to 3/4 of the cap");
        assert_eq!(super::to_evict(entries, 10 * mb, 5), vec!["b", "d"], "never one used in the frame on screen");
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn sized_pictures_are_bigger_than_thumbnails() {
        let icon = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/icon-1024.png");
        let small = super::native_build(icon).expect("thumbnail");
        let big = super::native_build(&format!("{icon}{}{}", super::SIZE_SEP, 800)).expect("sized");
        assert_eq!(small.size[0].max(small.size[1]), super::THUMB_W as usize);
        assert_eq!(big.size[0].max(big.size[1]), 800);
    }

    /// With its drive away a file can't be looked at: its last thumbnail is found by the path alone.
    #[test]
    fn the_last_thumbnail_of_a_file_is_found_without_the_file() {
        let dir = std::env::temp_dir().join(format!("phosphor-thumblast-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let clip = "/Volumes/SHOW/clip.mov";
        assert_eq!(super::native::last_known_in(&dir, clip, 192), None);
        std::fs::write(dir.join("abc.png"), b"x").unwrap();
        super::native::remember_in(&dir, clip, 192, "abc.png");
        assert_eq!(super::native::last_known_in(&dir, clip, 192), Some(dir.join("abc.png")));
        // Another size, another file: their own.
        assert_eq!(super::native::last_known_in(&dir, clip, 480), None);
        assert_eq!(super::native::last_known_in(&dir, "/Volumes/SHOW/other.mov", 192), None);
        // The thumbnail was cleared: nothing to show.
        std::fs::remove_file(dir.join("abc.png")).unwrap();
        assert_eq!(super::native::last_known_in(&dir, clip, 192), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A thumbnail made once is read back from the cache; an edited file (new length / time) gets a new one.
    #[test]
    fn thumbnails_are_cached_by_path_length_and_time() {
        let dir = std::env::temp_dir().join(format!("phosphor-thumbcache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.png");
        image::RgbaImage::from_pixel(400, 200, image::Rgba([10, 200, 30, 255])).save(&file).unwrap();
        let p = file.to_string_lossy().into_owned();
        let Some(c) = super::native::cache_file(&p, super::THUMB_W) else { return };  // (no settings folder in this test run)
        let _ = std::fs::remove_file(&c);
        let first = super::native_build(&p).expect("built");
        assert!(c.exists(), "kept on disk");
        let again = super::native_build(&p).expect("read back");
        assert_eq!((first.size, first.pixels.len()), (again.size, again.pixels.len()));
        // An edited file: another cache entry.
        image::RgbaImage::from_pixel(300, 300, image::Rgba([0, 0, 0, 255])).save(&file).unwrap();
        assert_ne!(super::native::cache_file(&p, super::THUMB_W), Some(c.clone()));
        let _ = std::fs::remove_file(&c);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
fn native_build(key: &str) -> Option<egui::ColorImage> {
    native::build(key)
}
