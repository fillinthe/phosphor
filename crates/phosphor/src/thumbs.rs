//! Thumbnails for the media bin, built on a background thread (images via `image`, movies via
//! an FFmpeg frame grab) and uploaded as egui textures on first use.

use std::collections::{HashMap, HashSet};

use eframe::egui;

pub const THUMB_W: u32 = 192;

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

    use super::THUMB_W;

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

    fn build(path: &str) -> Option<egui::ColorImage> {
        let name = path.to_ascii_lowercase();
        let img = if crate::media::is_movie(&name) {
            let ffmpeg = crate::stream::ffmpeg_path()?;
            let out = Command::new(ffmpeg)
                .args(["-v", "error", "-ss", "0.5", "-i", path, "-frames:v", "1", "-vf", &format!("scale={THUMB_W}:-2"), "-f", "image2pipe", "-vcodec", "png", "-"])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            image::load_from_memory(&out.stdout).ok()?
        } else {
            image::open(path).ok()?.thumbnail(THUMB_W, THUMB_W)
        };
        let rgba = img.to_rgba8();
        Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
    }
}
