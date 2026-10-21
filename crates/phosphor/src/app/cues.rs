//! Running the cue list: GO / back / fire, fades (`Composition::step_cue`), follow-on cues, and cues on
//! timecode (Phosphor's show clock or MIDI timecode).

use phosphor_engine::{CueFade, TimecodeSource};

use super::*;

/// Phosphor's own show clock for timecoded cues: play, pause, back to zero.
#[derive(Default)]
pub struct ShowClock {
    running: Option<f64>,
    offset: f64,
}

impl ShowClock {
    pub fn time(&self, now: f64) -> f64 {
        self.offset + self.running.map_or(0.0, |start| now - start)
    }

    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    pub fn toggle(&mut self, now: f64) {
        match self.running.take() {
            Some(start) => self.offset += now - start,
            None => self.running = Some(now),
        }
    }

    pub fn reset(&mut self, now: f64) {
        self.offset = 0.0;
        if self.running.is_some() {
            self.running = Some(now);
        }
    }
}

/// The cue list while the show runs.
#[derive(Default)]
pub struct CueRun {
    /// The cue last gone to.
    pub current: Option<usize>,
    pub fade: Option<CueFade>,
    /// When the current cue finished fading in (for follow-on cues).
    landed: Option<f64>,
    tc_prev: Option<f32>,
    /// Local time of day when scheduled cues were last checked.
    sched_prev: Option<u32>,
    pub clock: ShowClock,
}

impl State {
    /// Goes to cue `index` (fading as the cue says).
    pub fn go_cue(&mut self, index: usize, now: f64) {
        if let Some(fade) = self.comp.go_to_cue(index, now as f32) {
            self.cue_run.current = Some(index);
            self.cue_run.fade = Some(fade);
            self.cue_run.landed = None;
        }
    }

    /// GO: the next cue (the first one if none has run).
    pub fn cue_next(&mut self, now: f64) {
        let next = self.cue_run.current.map_or(0, |c| c + 1);
        if next < self.comp.cues.cues.len() {
            self.go_cue(next, now);
        }
    }

    pub fn cue_back(&mut self, now: f64) {
        if let Some(c) = self.cue_run.current.filter(|c| *c > 0) {
            self.go_cue(c - 1, now);
        }
    }

    /// The cue list's timecode now (seconds), if it follows one.
    pub fn timecode(&self, now: f64) -> Option<f64> {
        match self.comp.cues.timecode {
            TimecodeSource::Off => None,
            TimecodeSource::ShowClock => Some(self.cue_run.clock.time(now)),
            TimecodeSource::Midi => self.control.mtc.time(now),
            TimecodeSource::Ltc => self.audio.ltc_time(),
        }
    }

    /// Fades, follow-on cues and timecode. Call once per frame.
    pub(crate) fn run_cues(&mut self, now: f64) {
        self.audio.set_ltc(self.comp.cues.timecode == TimecodeSource::Ltc);
        if let Some(fade) = self.cue_run.fade.take() {
            self.comp.step_cue(&fade, now as f32);
            if fade.done(now as f32) {
                self.cue_run.landed = Some(now);
            } else {
                self.cue_run.fade = Some(fade);
            }
        }
        if let (Some(cur), Some(landed)) = (self.cue_run.current, self.cue_run.landed)
            && let Some(cue) = self.comp.cues.cues.get(cur)
            && cue.follow > 0.0
            && now - landed >= cue.follow as f64
        {
            self.cue_run.landed = None;
            self.cue_next(now);
        }
        // Daily schedules (installations), checked against the wall clock.
        if self.comp.cues.cues.iter().any(|c| c.daily.is_some()) {
            let (secs, weekday) = crate::clock::local_time();
            if let Some(prev) = self.cue_run.sched_prev
                && prev != secs
                && let Some(i) = self.comp.scheduled_cue(prev, secs, weekday)
            {
                self.go_cue(i, now);
            }
            self.cue_run.sched_prev = Some(secs);
        }
        match self.timecode(now).map(|t| t as f32) {
            Some(tc) => {
                let prev = self.cue_run.tc_prev.unwrap_or(tc - 1e-3);
                if let Some(i) = self.comp.timecoded_cue(prev, tc, self.cue_run.current) {
                    self.go_cue(i, now);
                }
                self.cue_run.tc_prev = Some(tc);
            }
            None => self.cue_run.tc_prev = None,
        }
    }
}

/// "01:02:03.4" for timecodes.
pub fn format_tc(secs: f64) -> String {
    let s = secs.max(0.0);
    format!("{:02}:{:02}:{:04.1}", (s / 3600.0) as u32, ((s / 60.0) as u32) % 60, s % 60.0)
}
