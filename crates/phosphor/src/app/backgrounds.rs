//! Output background pictures (`OutputConfig::background`): decoded off the UI thread when an output's picture
//! changes, then handed to the engine.

use super::*;

#[derive(Default)]
pub struct Backgrounds {
    /// The picture each output has (or is getting), by output id.
    loaded: HashMap<u64, String>,
    done: Option<(std::sync::mpsc::Sender<(u64, String, Option<image::RgbaImage>)>, std::sync::mpsc::Receiver<(u64, String, Option<image::RgbaImage>)>)>,
}

impl State {
    /// Starts loading any output's new background picture, and hands finished ones to the engine. Every frame.
    pub(crate) fn sync_backgrounds(&mut self) {
        let mut core = self.core.lock();
        let Some(engine) = core.engine.as_mut() else { return };
        let bg = &mut self.backgrounds;
        let (tx, rx) = bg.done.get_or_insert_with(std::sync::mpsc::channel);
        for (id, path, img) in rx.try_iter() {
            if self.comp.outputs.iter().any(|o| o.id == id && o.background == path) {
                engine.set_output_background(id, img.as_ref().map(|i| (i.width(), i.height(), i.as_raw().as_slice())));
            }
        }
        for o in &self.comp.outputs {
            if bg.loaded.get(&o.id) == Some(&o.background) {
                continue;
            }
            bg.loaded.insert(o.id, o.background.clone());
            if o.background.is_empty() {
                engine.set_output_background(o.id, None);
                continue;
            }
            let (tx, id, path) = (tx.clone(), o.id, o.background.clone());
            let max_side = crate::media::still_size::longest_side((o.width.max(self.comp.width), o.height.max(self.comp.height)), engine.device().limits().max_texture_dimension_2d);
            std::thread::spawn(move || {
                let img = crate::media::decode_still(&path).ok().map(|i| crate::media::still_size::fit(i, max_side).to_rgba8());
                let _ = tx.send((id, path, img));
            });
        }
        bg.loaded.retain(|id, _| self.comp.outputs.iter().any(|o| o.id == *id));
    }
}
