//! More recorders: besides the main stream / recording, up to `MAX` file recorders at once, each with its own
//! source (the program, or a layer, group or comp), file and format. Each reads its own monitor slot
//! (`FIRST_KEY + i`) at its own frame rate.

use super::State;
use crate::stream::{Destination, StreamSettings, Streamer};

pub(crate) const MAX: usize = 8;
/// Monitor keys for recorder `i`: `FIRST_KEY + i` (clear of preview windows and the tools at 250 and up).
pub(crate) const FIRST_KEY: u8 = 230;

/// A new recorder's settings: a file recording of the program at the canvas size.
pub(crate) fn new_settings() -> StreamSettings {
    StreamSettings { destination: Destination::File, height: 0, ..Default::default() }
}

impl State {
    pub(crate) fn recorder_add(&mut self) {
        if self.recorders.len() < MAX {
            self.recorders.push(Streamer::new(new_settings()));
        }
    }

    /// Starts or stops recorder `i`; a finished recording goes into the media bin.
    pub(crate) fn recorder_toggle(&mut self, i: usize, now: f64) {
        let (w, h) = (self.comp.width, self.comp.height);
        let Some(r) = self.recorders.get_mut(i) else { return };
        if !r.running() {
            r.settings.destination = Destination::File;
            r.mute_audio = true;
            self.notice = r.start(w, h, now).err();
            return;
        }
        r.stop();
        let path = r.settings.recorded_path();
        self.recording_saved(&path);
    }

    /// A recording that just finished: into the media bin, and say where it is.
    pub(crate) fn recording_saved(&mut self, path: &str) {
        let name = std::path::Path::new(path).file_name().map_or(path.to_string(), |n| n.to_string_lossy().to_string());
        self.comp.add_media(&name, path);
        self.notice = Some(format!("Recording saved: {path} (in the media bin)"));
    }

    /// The recorders' settings, to remember between runs.
    pub(crate) fn recorder_settings(&self) -> Vec<StreamSettings> {
        self.recorders.iter().map(|r| r.settings.clone()).collect()
    }
}
