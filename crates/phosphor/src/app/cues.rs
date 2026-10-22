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
    /// When someone last touched anything (mouse, keys, controllers), and whether the idle cue has gone since.
    pub(crate) last_input: f64,
    idle_done: bool,
    /// The project's opening cue has been gone to (cleared when a project loads).
    pub(crate) opened: bool,
    /// The calendar file's events, which file and when it changed (re-read when it does), and the last local time
    /// checked (local wall-clock seconds).
    #[cfg(not(target_arch = "wasm32"))]
    calendar: Option<(String, Option<std::time::SystemTime>, Vec<crate::calendar::Event>)>,
    calendar_prev: i64,
    calendar_checked: f64,
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

    /// A random cue, never the one on now.
    pub fn cue_random(&mut self, now: f64) {
        let n = self.comp.cues.cues.len();
        if n == 0 {
            return;
        }
        let mut pick = super::quick::dice().below(n);
        if n > 1 && Some(pick) == self.cue_run.current {
            pick = (pick + 1) % n;
        }
        self.go_cue(pick, now);
    }

    /// The next cue that will fire by itself, and in how many seconds: a "then next" after the current cue, or the
    /// next timecoded cue while the timecode runs.
    pub fn next_cue_in(&self, now: f64) -> Option<(usize, f64)> {
        let follow = self.cue_run.current.and_then(|c| {
            let cue = self.comp.cues.cues.get(c)?;
            (cue.follow > 0.0 && c + 1 < self.comp.cues.cues.len()).then_some(())?;
            // Still fading in: the follow time starts once it lands.
            let left = match (self.cue_run.landed, &self.cue_run.fade) {
                (Some(landed), _) => cue.follow as f64 - (now - landed),
                (None, Some(f)) => (f.secs - (now as f32 - f.start)).max(0.0) as f64 + cue.follow as f64,
                (None, None) => return None,
            };
            Some((c + 1, left.max(0.0)))
        });
        let timed = self.timecode(now).and_then(|tc| {
            self.comp.cues.cues.iter().enumerate().filter_map(|(i, c)| c.at.filter(|a| *a as f64 > tc).map(|a| (i, a as f64 - tc))).min_by(|a, b| a.1.total_cmp(&b.1))
        });
        match (follow, timed) {
            (Some(a), Some(b)) => Some(if a.1 <= b.1 { a } else { b }),
            (a, b) => a.or(b),
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

    /// Calendar events start the cues with their names (checked once a second; the file re-read when it changes).
    #[cfg(not(target_arch = "wasm32"))]
    fn run_calendar(&mut self, now: f64) {
        let path = self.comp.cues.calendar.clone();
        if path.is_empty() {
            self.cue_run.calendar = None;
            return;
        }
        if now - self.cue_run.calendar_checked < 1.0 {
            return;
        }
        self.cue_run.calendar_checked = now;
        let (local, offset) = crate::calendar::local_now();
        let changed = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if self.cue_run.calendar.as_ref().is_none_or(|(p, m, _)| *p != path || *m != changed) {
            let events = std::fs::read_to_string(&path).map(|t| crate::calendar::parse(&t, offset)).unwrap_or_default();
            self.cue_run.calendar = Some((path, changed, events));
            self.cue_run.calendar_prev = local;
            return;
        }
        let prev = std::mem::replace(&mut self.cue_run.calendar_prev, local);
        // A jump (the Mac slept, the clock was set): nothing fires for the time in between.
        if local < prev || local - prev > 120 {
            return;
        }
        let due: Vec<String> = self.cue_run.calendar.as_ref().map_or(Vec::new(), |(_, _, ev)| ev.iter().filter(|e| e.starts_between(prev, local)).map(|e| e.title.to_lowercase()).collect());
        for title in due {
            if let Some(i) = self.comp.cues.cues.iter().position(|c| c.name.trim().to_lowercase() == title) {
                self.go_cue(i, now);
            }
        }
    }

    /// Live text layers (clock, countdown, stopwatch): their words follow the time. Every frame; a layer only
    /// redraws when its words change (once a second).
    pub(crate) fn run_live_text(&mut self, now: f64) {
        let (tod, _) = crate::clock::local_time();
        for l in &mut self.comp.layers {
            if let phosphor_engine::SourceSpec::Text(style) = &mut l.source
                && let Some(words) = style.live.words(tod, now - style.live_start)
                && style.text != words
            {
                style.text = words;
            }
        }
    }

    /// Several layers selected: whatever changed on the selected one this frame goes to the others too. Costs a
    /// comparison per frame only while more than one layer is selected.
    pub(crate) fn run_multi_edit(&mut self) {
        self.also_selected.retain(|id| self.comp.layers.iter().any(|l| l.id == *id));
        self.run_clip_multi_edit();
        let Some(sel) = self.selected.filter(|_| !self.also_selected.is_empty()) else {
            self.multi_before = None;
            return;
        };
        let Some(now) = self.comp.layer(sel) else { return };
        match &self.multi_before {
            Some(before) if before.id == sel => {
                if before != now {
                    let before = self.multi_before.take().expect("checked");
                    let to = self.also_selected.clone();
                    self.comp.copy_layer_changes(&before, &to);
                    self.multi_before = self.comp.layer(sel).cloned();
                }
            }
            _ => self.multi_before = Some(now.clone()),
        }
    }

    /// Several clips selected: what changed in how the selected clip plays goes to the others too.
    fn run_clip_multi_edit(&mut self) {
        let Some(from) = self.selected_clip.filter(|_| !self.also_clips.is_empty()) else {
            self.clip_before = None;
            return;
        };
        let Some(now) = self.comp.layer(from.0).and_then(|l| l.clips.get(from.1)).cloned() else { return };
        match &self.clip_before {
            Some(before) if before.id == now.id => {
                if *before != now {
                    let (before, to) = (before.clone(), self.also_clips.clone());
                    self.comp.copy_clip_changes(&before, from, &to);
                    self.clip_before = Some(now);
                }
            }
            _ => self.clip_before = Some(now),
        }
    }

    /// Someone touched something (the idle cue waits again).
    pub(crate) fn touched(&mut self, now: f64) {
        self.cue_run.last_input = now;
        self.cue_run.idle_done = false;
    }

    /// Fades, follow-on cues and timecode. Call once per frame.
    pub(crate) fn run_cues(&mut self, now: f64) {
        // The opening cue, once per project.
        if !self.cue_run.opened {
            self.cue_run.opened = true;
            self.cue_run.last_input = now;
            if let Some(i) = self.comp.cues.open_cue_index() {
                self.go_cue(i, now);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.run_calendar(now);
        // The idle cue, once each time things go quiet.
        if !self.cue_run.idle_done
            && let Some(i) = self.comp.cues.idle_cue_index(now - self.cue_run.last_input)
        {
            self.cue_run.idle_done = true;
            self.go_cue(i, now);
        }
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
