//! Render the show to a movie offline: the show's clock steps one frame at a time and each frame waits for every
//! movie's picture, so a show too heavy to play smoothly still comes out smooth (no dropped or repeated frames).
//! It never runs ahead of real time, so the app's clock just carries on afterwards. Picture only (no sound yet).

use super::State;
use crate::stream::{Destination, FileFormat, StreamSettings};

/// What to render: where, in which format, how many frames a second, how long, and whether the timeline starts
/// from its beginning.
#[derive(Clone, Debug)]
pub(crate) struct OfflineSetup {
    pub path: String,
    pub format: FileFormat,
    pub fps: u32,
    pub secs: f64,
    pub from_start: bool,
}

impl Default for OfflineSetup {
    fn default() -> Self {
        Self { path: String::new(), format: FileFormat::Auto, fps: 30, secs: 0.0, from_start: true }
    }
}

/// A render in progress.
pub(crate) struct Offline {
    pub fps: u32,
    pub frames: u64,
    /// Frames taken so far; each one moved the show's clock on by 1 / fps.
    pub done: u64,
    /// Frames that waited too long for a movie and went out anyway.
    pub late: u32,
    start: f64,
    /// The recording settings it borrowed, put back afterwards.
    saved: (StreamSettings, bool),
    /// When this frame started waiting for its movies (real time), and when the last frame was taken.
    waiting: Option<std::time::Instant>,
    finished: Option<std::time::Instant>,
}

/// How long one frame waits for a movie before going out anyway.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(3);

impl Offline {
    /// The show's time now.
    pub fn now(&self) -> f64 {
        self.start + self.done as f64 / self.fps as f64
    }

    pub fn progress(&self) -> f32 {
        (self.done as f64 / self.frames.max(1) as f64) as f32
    }

    /// Should this frame go into the movie? Yes once every movie has its picture (`ready`), or after waiting too
    /// long. Never past the end.
    pub fn take(&mut self, ready: bool) -> bool {
        if self.done >= self.frames {
            return false;
        }
        if ready {
            self.waiting = None;
            return true;
        }
        if self.waiting.get_or_insert_with(std::time::Instant::now).elapsed() > PATIENCE {
            self.waiting = None;
            self.late += 1;
            return true;
        }
        false
    }
}

impl State {
    /// Starts rendering `setup` (the timeline's length when it says 0 seconds).
    pub(crate) fn offline_start(&mut self, setup: &OfflineSetup, now: f64) {
        if self.stream.running() {
            self.notice = Some("Stop the stream or recording first".into());
            return;
        }
        if setup.path.trim().is_empty() {
            self.notice = Some("Choose a file first".into());
            return;
        }
        let secs = if setup.secs > 0.0 { setup.secs } else { self.comp.timeline.length };
        let fps = setup.fps.clamp(1, 120);
        let saved = (self.stream.settings.clone(), self.stream.mute_audio);
        self.stream.settings = StreamSettings {
            destination: Destination::File,
            file_path: setup.path.trim().into(),
            file_format: setup.format,
            height: 0,
            fps,
            kbps: saved.0.kbps.max(20_000),
            ..Default::default()
        };
        self.stream.mute_audio = true;
        self.stream.offline = true;
        if setup.from_start {
            self.timeline_seek(0.0, now);
            if self.timeline_play.is_none() {
                self.timeline_toggle(now);
            }
        }
        let (w, h) = (self.comp.width, self.comp.height);
        if let Err(e) = self.stream.start(w, h, now) {
            self.notice = Some(e);
            (self.stream.settings, self.stream.mute_audio) = saved;
            self.stream.offline = false;
            return;
        }
        self.offline = Some(Offline { fps, frames: (secs * fps as f64).round().max(1.0) as u64, done: 0, late: 0, start: now, saved, waiting: None, finished: None });
    }

    /// Every frame: once the last frame is in (and the encoder has it all), finishes the file and puts it in the
    /// media bin.
    pub(crate) fn run_offline(&mut self) {
        let Some(o) = &mut self.offline else { return };
        if o.done < o.frames && self.stream.running() {
            return;
        }
        // Frames still being read back from the GPU arrive a few frames later.
        let at = *o.finished.get_or_insert_with(std::time::Instant::now);
        if self.stream.running() && self.stream.sent() < o.frames && at.elapsed().as_secs_f32() < 2.0 {
            return;
        }
        self.offline_stop();
    }

    /// Stops a render (finished or not); what's rendered so far is kept.
    pub(crate) fn offline_stop(&mut self) {
        let Some(o) = self.offline.take() else { return };
        let ok = self.stream.running();
        self.stream.stop();
        let path = self.stream.settings.recorded_path();
        (self.stream.settings, self.stream.mute_audio) = o.saved;
        self.stream.offline = false;
        if !ok {
            self.notice = Some(format!("Rendering stopped: {}", self.stream.status().0));
            return;
        }
        self.recording_saved(&path);
        let late = if o.late > 0 { format!(" ({} frames didn't wait for a slow movie)", o.late) } else { String::new() };
        self.notice = Some(format!("Rendered {} frames to {path} (in the media bin){late}", o.done));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(frames: u64) -> Offline {
        Offline { fps: 25, frames, done: 0, late: 0, start: 10.0, saved: (Default::default(), false), waiting: None, finished: None }
    }

    #[test]
    fn the_clock_moves_a_frame_at_a_time_and_waits_for_movies() {
        let mut o = render(2);
        assert_eq!(o.now(), 10.0);
        assert!(!o.take(false), "a movie isn't ready: wait");
        assert!(o.take(true));
        o.done += 1;
        assert_eq!(o.now(), 10.04);
        assert!(o.take(true));
        o.done += 1;
        assert!(!o.take(true), "past the end");
        assert_eq!(o.progress(), 1.0);
    }
}
