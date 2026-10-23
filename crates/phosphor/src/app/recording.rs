//! Starting and stopping the stream / recording: now, or on the next bar of the clock (`StreamSettings::on_bar`).
use super::State;

impl State {
    /// Starts the stream or recording at the canvas size.
    pub(crate) fn stream_start(&mut self, now: f64) {
        let (w, h) = (self.comp.width, self.comp.height);
        self.notice = self.stream.start(w, h, now).err();
    }

    /// Stops it; a finished recording goes into the media bin, ready to play.
    pub(crate) fn stream_stop(&mut self) {
        self.stream.stop();
        if self.stream.settings.destination == crate::stream::Destination::File {
            let path = self.stream.settings.recorded_path();
            self.recording_saved(&path);
        }
    }

    /// Start (or stop) now, or on the next bar when that's set. Pressing again while waiting cancels.
    pub(crate) fn stream_toggle(&mut self, now: f64) {
        let start = !self.stream.running();
        if !self.stream.settings.on_bar {
            if start { self.stream_start(now) } else { self.stream_stop() }
            return;
        }
        self.stream_bar = match self.stream_bar {
            Some(_) => None,
            None => Some((start, bar_of(self.clock.beats(now)))),
        };
    }

    /// Waiting for the next bar: goes when the clock gets there. Every frame.
    pub(crate) fn run_stream_bar(&mut self, now: f64) {
        let Some((start, bar)) = self.stream_bar else { return };
        if bar_of(self.clock.beats(now)) > bar {
            self.stream_bar = None;
            if start { self.stream_start(now) } else { self.stream_stop() }
        }
    }
}

/// Which bar (of four beats) the clock is in.
fn bar_of(beats: f64) -> i64 {
    (beats / 4.0).floor() as i64
}

#[cfg(test)]
mod tests {
    #[test]
    fn bars_are_four_beats() {
        assert_eq!((super::bar_of(0.0), super::bar_of(3.99), super::bar_of(4.0), super::bar_of(-0.5)), (0, 0, 1, -1));
    }
}
