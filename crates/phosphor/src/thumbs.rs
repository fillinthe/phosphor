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
        let img = if path.starts_with("https://") {
            image::load_from_memory(&crate::online::get_bytes(path).ok()?).ok()?.thumbnail(size, size)
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
        Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
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
}

#[cfg(all(test, not(target_arch = "wasm32")))]
fn native_build(key: &str) -> Option<egui::ColorImage> {
    native::build(key)
}
