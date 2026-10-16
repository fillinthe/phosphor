//! Ableton Link: tempo and beat shared with Ableton Live, Traktor, Resolume, phones and anything else on the
//! network that speaks Link. Phosphor's clock follows the session (tempo, and beat 1 on Link's bar); tempo
//! changes made in Phosphor (tap, typing, a controller) go out to the session.

use rusty_link::{AblLink, SessionState};

use crate::clock::Clock;

/// Beats per bar Link aligns to.
const QUANTUM: f64 = 4.0;

pub struct LinkSync {
    link: AblLink,
    state: SessionState,
    /// The tempo we last agreed with the session (a different `clock.bpm` means it was changed here).
    agreed: f32,
}

impl LinkSync {
    pub fn new(bpm: f32) -> Self {
        let link = AblLink::new(bpm as f64);
        link.enable(true);
        Self { link, state: SessionState::new(), agreed: bpm }
    }

    pub fn peers(&self) -> u64 {
        self.link.num_peers()
    }

    /// Follows the session, or tells it about a tempo change made here. Call once per frame.
    pub fn sync(&mut self, clock: &mut Clock, now: f64) {
        self.link.capture_app_session_state(&mut self.state);
        let micros = self.link.clock_micros();
        if (clock.bpm - self.agreed).abs() > 0.01 {
            // Changed here: the session takes our tempo.
            self.state.set_tempo(clock.bpm as f64, micros);
            self.link.commit_app_session_state(&self.state);
            self.agreed = clock.bpm;
        } else {
            let bpm = self.state.tempo() as f32;
            clock.bpm = bpm;
            self.agreed = bpm;
        }
        // Beat 1 of our bar sits on Link's bar.
        let beats = self.state.beat_at_time(micros, QUANTUM);
        clock.set_beats(now, beats);
    }

    /// "Resync" with Link on: beat 1 is now, for everyone in the session.
    pub fn resync(&mut self) {
        self.link.capture_app_session_state(&mut self.state);
        self.state.request_beat_at_time(0.0, self.link.clock_micros(), QUANTUM);
        self.link.commit_app_session_state(&self.state);
    }
}

impl Drop for LinkSync {
    fn drop(&mut self) {
        self.link.enable(false);
    }
}
