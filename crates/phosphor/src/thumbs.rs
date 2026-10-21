//! Thumbnails for the media bin, built on a background thread (images via `image`, movies via
//! an FFmpeg frame grab) and uploaded as egui textures on first use.

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
}

impl Thumbs {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            pending: HashSet::new(),
            #[cfg(not(target_arch = "wasm32"))]
            worker: native::Worker::new(),
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
            self.worker.request(path.to_string());
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
        for (path, image) in self.worker.finished() {
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

    use eframe::egui;

    use super::{SIZE_SEP, THUMB_W};

    pub struct Worker {
        req: Sender<String>,
        done: Receiver<(String, Option<egui::ColorImage>)>,
    }

    impl Worker {
        pub fn new() -> Self {
            let (req, req_rx) = channel::<String>();
            let (done_tx, done) = channel();
            std::thread::Builder::new()
                .name("phosphor-thumbs".into())
                .spawn(move || {
                    for path in req_rx {
                        let _ = done_tx.send((path.clone(), build(&path)));
                    }
                })
                .expect("thumbnail thread");
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
        let img = if crate::media::is_movie(&name) {
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
            image::open(path).ok()?.thumbnail(size, size)
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
