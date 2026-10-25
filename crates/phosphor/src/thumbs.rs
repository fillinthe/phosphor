//! Thumbnails for the media bin, built on a background thread (images via `image`, movies via
//! an FFmpeg frame grab) and uploaded as egui textures on first use. An `https://` address works too (online
//! libraries): those are fetched on threads of their own so slow sites never hold up the media bin's.

use std::collections::{HashMap, HashSet};

use eframe::egui;

pub const THUMB_W: u32 = 192;

/// Between a path and its size in a `get_sized` key.
const SIZE_SEP: char = '\u{1}';

pub struct Thumbs {
    cache: HashMap<String, Option<egui::TextureHandle>>,
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
        if let Some(entry) = self.cache.get(path) {
            return entry.as_ref().map(|h| h.id());
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
            self.cache.insert(key.to_string(), handle);
        }
        self.cache.get(key).and_then(|h| h.as_ref().map(|h| h.id()))
    }

    /// A loaded thumbnail's width / height (to fit it in a tile without stretching).
    pub fn aspect(&self, path: &str) -> Option<f32> {
        let size = self.cache.get(path)?.as_ref()?.size();
        (size[1] > 0).then(|| size[0] as f32 / size[1] as f32)
    }

    pub fn is_pending(&self, path: &str) -> bool {
        self.pending.contains(path)
    }

    /// Uploads finished thumbnails; call once per frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        for (path, image) in self.worker.finished().into_iter().chain(self.web.as_ref().map(|w| w.finished()).unwrap_or_default()) {
            self.pending.remove(&path);
            let handle = image.map(|img| ctx.load_texture(format!("thumb:{path}"), img, egui::TextureOptions::LINEAR));
            self.cache.insert(path, handle);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = ctx;
    }

    pub fn forget(&mut self, path: &str) {
        self.cache.remove(path);
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
    use std::process::{Command, Stdio};
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
        if let Some(c) = &cached
            && let Ok(img) = image::open(c)
        {
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
            let out = Command::new(ffmpeg)
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
            if rgba.save_with_format(&part, image::ImageFormat::Png).is_ok() {
                let _ = std::fs::rename(&part, &c);
            }
        }
        Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
    }

    /// Where a file's thumbnail at `size` is kept: Phosphor's cache folder, named by a hash of the path, the file's
    /// length and modification time (an edited file gets a new one). None when the file can't be looked at.
    pub(super) fn cache_file(path: &str, size: u32) -> Option<std::path::PathBuf> {
        use std::hash::{Hash, Hasher};
        // (A sequence pattern isn't a file: its first frame stands in.)
        let probe = if crate::media::sequence::is_sequence(path) { crate::media::sequence::members(path).into_iter().next().map(|p| p.to_string_lossy().into_owned())? } else { path.to_string() };
        let meta = std::fs::metadata(&probe).ok()?;
        let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (path, meta.len(), modified, size).hash(&mut h);
        Some(crate::show::storage_dir()?.join("cache").join("thumbs").join(format!("{:016x}.png", h.finish())))
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
