//! The timeline's sound track: a sound-only player that follows the playhead (plays while it plays, jumps when it
//! jumps), and the sound's waveform, read on a worker thread for drawing.

use std::sync::{Arc, Mutex};

use super::State;
use crate::media::video::VideoPlayer;

pub(crate) struct TimelineSound {
    path: String,
    player: Option<VideoPlayer>,
    /// Loudness peaks (`sound::PEAKS_PER_SEC` a second); None while they're being read.
    pub peaks: Arc<Mutex<Option<Vec<f32>>>>,
}

impl TimelineSound {
    /// Where the sound is playing (seconds into the file), once it's open.
    pub fn position(&self) -> Option<f64> {
        self.player.as_ref().map(|p| p.position())
    }
}

impl State {
    /// Every frame: keeps the sound track where the playhead is. Nothing runs without one.
    pub(crate) fn run_timeline_sound(&mut self, now: f64) {
        let Some(track) = self.comp.timeline.sound.clone().filter(|s| !s.path.is_empty()) else {
            self.timeline_sound = None;
            return;
        };
        if self.timeline_sound.as_ref().is_none_or(|s| s.path != track.path) {
            let peaks = Arc::new(Mutex::new(None));
            let (p, path) = (peaks.clone(), track.path.clone());
            let _ = std::thread::Builder::new().name("timeline waveform".into()).spawn(move || {
                let read = crate::media::video::sound::peaks(&path).unwrap_or_default();
                if let Ok(mut slot) = p.lock() {
                    *slot = Some(read);
                }
            });
            let player = match VideoPlayer::open(&track.path) {
                Ok(p) => Some(p),
                Err(e) => {
                    self.notice = Some(format!("The timeline's sound: {e}"));
                    None
                }
            };
            self.timeline_sound = Some(TimelineSound { path: track.path.clone(), player, peaks });
        }
        let Some(p) = self.timeline_sound.as_mut().and_then(|s| s.player.as_mut()) else { return };
        let at = track.position(self.timeline_pos);
        p.set_looping(false);
        p.set_volume(track.volume);
        // Follow jumps (dragging the playhead, loops, markers).
        if let Some(x) = at
            && (p.position() - x).abs() > 0.15
        {
            p.seek(x);
        }
        p.set_playing(self.timeline_play.is_some() && at.is_some());
        let _ = p.poll_frame(now);
    }
}
