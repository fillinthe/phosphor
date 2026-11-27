//! Pictures read back from the GPU once, for the app: the eyedropper's pixel, a snapshot, a clip's picture, a layer's
//! palette or colors to match. Asked with `State::read_back`, they come back a few frames later and are used by
//! `PhosphorApp::run_readbacks`. These used `Engine::read_output`, which waited on the UI thread for the GPU with no
//! time limit (a freeze sample from a released build stopped there).

use phosphor_engine::{CapturedFrame, ReadSource};

use super::*;

/// What a read picture is for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReadFor {
    /// The pixel goes to the waiting color button (`ui::eyedropper`).
    Eyedropper,
    /// The whole picture is saved as a PNG (the save dialog is open meanwhile).
    #[cfg(not(target_arch = "wasm32"))]
    Snapshot,
    /// The layer's thumbnail becomes this clip's picture in the grid.
    #[cfg(not(target_arch = "wasm32"))]
    ClipPicture { clip: u64 },
    /// A Palette effect with the layer's six main colors.
    Palette(u64),
    /// A Color Match effect taking the layer's colors to `to` (a reference picture's average and spread).
    #[cfg(not(target_arch = "wasm32"))]
    ColorMatch { layer: u64, to: [(f32, f32); 3] },
}

/// Reads asked for and not back yet.
#[derive(Default)]
pub(crate) struct Readbacks {
    next_tag: u64,
    waiting: Vec<(u64, ReadFor, web_time::Instant)>,
}

/// A read not back by then is given up (the GPU is stuck or the picture went away).
const GIVE_UP: std::time::Duration = std::time::Duration::from_secs(3);

impl Readbacks {
    fn add(&mut self, what: ReadFor) -> u64 {
        self.next_tag += 1;
        self.waiting.push((self.next_tag, what, web_time::Instant::now()));
        self.next_tag
    }

    /// A read for `what` is on its way.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn waiting_for(&self, what: &ReadFor) -> bool {
        self.waiting.iter().any(|(_, w, _)| w == what)
    }

    /// The reads that came back (with what each is for), and those given up.
    fn collect(&mut self, frames: Vec<CapturedFrame>) -> (Vec<(ReadFor, CapturedFrame)>, Vec<ReadFor>) {
        let mut back = Vec::new();
        for f in frames {
            if let Some(i) = self.waiting.iter().position(|(tag, _, _)| *tag == f.tag) {
                back.push((self.waiting.remove(i).1, f));
            }
        }
        let (late, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting).into_iter().partition(|(_, _, at)| at.elapsed() > GIVE_UP);
        self.waiting = keep;
        (back, late.into_iter().map(|(_, w, _)| w).collect())
    }
}

/// What to say when a read for `what` couldn't be made or never came.
fn failed(what: &ReadFor) -> Option<String> {
    match what {
        ReadFor::Eyedropper => None,
        #[cfg(not(target_arch = "wasm32"))]
        ReadFor::Snapshot => Some("Couldn't read the picture for the snapshot (try again)".into()),
        #[cfg(not(target_arch = "wasm32"))]
        ReadFor::ClipPicture { .. } => Some("Couldn't keep the picture: the layer has no picture (is it on?)".into()),
        ReadFor::Palette(_) => Some("The layer has no picture to take colors from (is it on?)".into()),
        #[cfg(not(target_arch = "wasm32"))]
        ReadFor::ColorMatch { .. } => Some("The layer has no picture to match (is it on?)".into()),
    }
}

impl State {
    /// Asks for `source` as it is now; it's used for `what` when it comes back (`PhosphorApp::run_readbacks`). Says
    /// so at once when there's no such picture.
    pub(crate) fn read_back(&mut self, source: ReadSource, what: ReadFor) {
        let tag = self.readbacks.add(what.clone());
        let asked = self.core.lock().engine.as_mut().is_some_and(|e| e.request_read(source, tag));
        if !asked {
            self.readbacks.waiting.retain(|(t, _, _)| *t != tag);
            if let Some(said) = failed(&what) {
                self.notice = Some(said);
            }
        }
    }
}

impl PhosphorApp {
    /// Uses the reads that came back. Nothing to do (and the GPU isn't asked) while none are waiting.
    pub(crate) fn run_readbacks(&mut self, ctx: &egui::Context) {
        if self.state.readbacks.waiting.is_empty() {
            return;
        }
        let frames = self.state.core.lock().engine.as_mut().map(|e| e.take_reads()).unwrap_or_default();
        let (back, late) = self.state.readbacks.collect(frames);
        for what in late {
            if let Some(said) = failed(&what) {
                self.state.notice = Some(said);
            }
        }
        for (what, f) in back {
            self.use_read(ctx, what, f);
        }
    }

    fn use_read(&mut self, ctx: &egui::Context, what: ReadFor, f: CapturedFrame) {
        let st = &mut self.state;
        match what {
            ReadFor::Eyedropper => {
                if let Some(rgb) = crate::ui::eyedropper::pixel_at(f.width, f.height, &f.rgba, [0.0, 0.0]) {
                    crate::ui::eyedropper::deliver(ctx, rgb);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            ReadFor::Snapshot => match self.snapshot_path.take() {
                // The file was picked before the picture came: it's written now.
                Some(p) => save_png(p, f.width, f.height, f.rgba),
                None => self.pending_snapshot = Some((f.width, f.height, f.rgba)),
            },
            #[cfg(not(target_arch = "wasm32"))]
            ReadFor::ClipPicture { clip } => {
                st.notice = Some(match keep_clip_picture(st, clip, &f) {
                    Ok(()) => "The clip shows this picture in the grid now".into(),
                    Err(e) => format!("Couldn't keep the picture: {e}"),
                });
            }
            ReadFor::Palette(layer) => {
                let pal = phosphor_engine::palette_of(&f.rgba, 6);
                let added = !pal.is_empty() && st.comp.add_palette(layer, &pal).is_some();
                st.notice = Some(if added { "Palette added with this layer's six main colors".into() } else { "The layer has no picture to take colors from (is it on?)".into() });
            }
            #[cfg(not(target_arch = "wasm32"))]
            ReadFor::ColorMatch { layer, to } => {
                let from = phosphor_engine::color_stats(&f.rgba);
                st.notice = Some(match st.comp.add_color_match(layer, from, to) {
                    Some(_) => "Color Match added: the layer's colors follow the picture (tweak it in its effects)".into(),
                    None => "That layer is gone".into(),
                });
            }
        }
    }
}

/// Writes a picture as a PNG on a thread of its own (a 4K PNG takes a moment to make).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_png(path: std::path::PathBuf, w: u32, h: u32, rgba: Vec<u8>) {
    std::thread::spawn(move || {
        if let Err(e) = image::save_buffer(&path, &rgba, w, h, image::ColorType::Rgba8) {
            log::warn!("snapshot {}: {e}", path.display());
        }
    });
}

/// The layer's thumbnail (`f`), saved next to the settings (clip thumbnails/<clip id>.png) as the clip's picture.
#[cfg(not(target_arch = "wasm32"))]
fn keep_clip_picture(st: &mut State, clip: u64, f: &CapturedFrame) -> Result<(), String> {
    let dir = crate::show::storage_dir().ok_or("no settings folder")?.join("clip thumbnails");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let c = st.comp.layers.iter_mut().flat_map(|l| l.clips.iter_mut()).find(|c| c.id == clip).ok_or("that clip is gone")?;
    let path = dir.join(format!("{}.png", c.id));
    // (A thumbnail is small: a millisecond or two to make.)
    image::save_buffer(&path, &f.rgba, f.width, f.height, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
    c.thumb = path.to_string_lossy().to_string();
    // The same file gets a new picture: drop the old one from the cache.
    st.thumbs.forget(&c.thumb);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(tag: u64) -> CapturedFrame {
        CapturedFrame { tag, width: 1, height: 1, rgba: vec![255, 0, 0, 255] }
    }

    #[test]
    fn reads_find_what_they_were_for_and_late_ones_are_given_up() {
        let mut r = Readbacks::default();
        let a = r.add(ReadFor::Eyedropper);
        let b = r.add(ReadFor::Palette(7));
        let (back, late) = r.collect(vec![frame(b), frame(999)]);
        assert_eq!(back.iter().map(|(w, f)| (w.clone(), f.tag)).collect::<Vec<_>>(), vec![(ReadFor::Palette(7), b)]);
        assert!(late.is_empty());
        assert_eq!(r.waiting.len(), 1, "the eyedropper's is still on its way");
        r.waiting[0].2 = web_time::Instant::now() - GIVE_UP - std::time::Duration::from_millis(1);
        let (back, late) = r.collect(Vec::new());
        assert!(back.is_empty());
        assert_eq!(late, vec![ReadFor::Eyedropper]);
        assert!(r.waiting.is_empty());
        assert_ne!(a, b);
    }
}
