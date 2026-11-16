//! Undo / redo for the project. A snapshot is taken once an edit settles (mouse up and nothing
//! changed for a moment), so one drag of a slider is one undo step.

use phosphor_engine::Composition;

const LIMIT: usize = 100;
/// Seconds an edit has to be still before it becomes an undo step.
const SETTLE: f64 = 0.4;

#[derive(Default)]
pub struct History {
    undo: Vec<Composition>,
    redo: Vec<Composition>,
    /// The project as of the last undo step.
    committed: Option<Composition>,
    changed_at: Option<f64>,
    /// When the project was last compared with `committed`.
    checked_at: f64,
}

/// Where two saved projects differ, as paths ("layers.0.opacity: 1.0 -> 0.5"), at most a handful.
fn differences(path: &str, a: &serde_json::Value, b: &serde_json::Value, out: &mut Vec<String>) {
    use serde_json::Value::{Array, Object};
    if a == b || out.len() >= 6 {
        return;
    }
    match (a, b) {
        (Object(x), Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                differences(&format!("{path}.{k}"), x.get(k).unwrap_or(&serde_json::Value::Null), y.get(k).unwrap_or(&serde_json::Value::Null), out);
            }
        }
        (Array(x), Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                differences(&format!("{path}.{i}"), p, q, out);
            }
        }
        _ => {
            let short = |v: &serde_json::Value| v.to_string().chars().take(40).collect::<String>();
            out.push(format!("{}: {} -> {}", path.trim_start_matches('.'), short(a), short(b)));
        }
    }
}

/// Fields that change on their own (tempo following, transitions in flight, live text) aren't edits.
fn settle_volatile(from: &Composition, into: &mut Composition) {
    into.bpm = from.bpm;
    keep_lasers_live(from, into);
    // Blackout is a live switch with a clock that ticks every frame, what tracking last read comes from the camera,
    // and a moving fader's position moves by itself: none of them is an edit. (The blackout clock alone made a new
    // undo step twice a second in every project, so Undo stepped through copies that looked the same and the
    // real edits fell off the end: owner, 2026-11-16, "undo not working".)
    (into.blackout_on, into.blackout, into.blackout_clock) = (from.blackout_on, from.blackout, from.blackout_clock);
    into.seen_code.clone_from(&from.seen_code);
    into.seen_text.clone_from(&from.seen_text);
    for w in &mut into.surface.widgets {
        if let Some(src) = from.surface.widgets.iter().find(|s| s.id == w.id) {
            w.live = src.live;
        }
    }
    // A modulator's knob position, its data reading and its looper take move by themselves (sound bands, tracking,
    // live data, a recording looper): with those counted as edits, Undo filled with a copy of the project twice a
    // second while music played and the real edits fell off the end.
    for m in &mut into.modulators {
        if let Some(src) = from.modulators.iter().find(|s| s.id == m.id) {
            m.value = src.value;
            m.live.clone_from(&src.live);
            m.looper.clone_from(&src.looper);
        }
    }
    for l in &mut into.layers {
        if let Some(src) = from.layers.iter().find(|s| s.id == l.id) {
            l.transition = src.transition.clone();
            l.voice_turn = src.voice_turn;
            // Live text (a clock, a countdown) changes its words by itself.
            if let (phosphor_engine::SourceSpec::Text(t), phosphor_engine::SourceSpec::Text(st)) = (&mut l.source, &src.source)
                && st.live != phosphor_engine::LiveText::Off
                && t.live == st.live
            {
                t.text.clone_from(&st.text);
                t.live_start = st.live_start;
            }
        }
    }
}

// "Wherefore let him that thinketh he standeth take heed lest he fall." 1 Corinthians 10:12 (KJV)
/// Arming the lasers and their test patterns are live switches, not edits: never part of an undo step, and Undo /
/// Redo keep them as they are now (an old snapshot must never switch beams back on).
fn keep_lasers_live(from: &Composition, into: &mut Composition) {
    into.lasers.armed = from.lasers.armed;
    for p in &mut into.lasers.projectors {
        p.test_pattern = from.lasers.projectors.iter().find(|f| f.id == p.id).is_some_and(|f| f.test_pattern);
    }
}

impl History {
    /// Call once per frame. `busy` = a mouse button is down (mid-drag).
    pub fn track(&mut self, comp: &Composition, now: f64, busy: bool) {
        let Some(committed) = self.committed.as_mut() else {
            self.committed = Some(comp.clone());
            return;
        };
        // Comparing two whole projects every frame adds up; ten times a second is plenty (SETTLE is longer).
        if now - self.checked_at < 0.1 {
            return;
        }
        self.checked_at = now;
        settle_volatile(comp, committed);
        if committed == comp {
            self.changed_at = None;
            return;
        }
        let since = *self.changed_at.get_or_insert(now);
        if busy || now - since < SETTLE {
            return;
        }
        let prev = std::mem::replace(committed, comp.clone());
        // PHOSPHOR_UNDO_DEBUG=1: say what changed in each step (finding values that move by themselves).
        if std::env::var_os("PHOSPHOR_UNDO_DEBUG").is_some()
            && let (Ok(a), Ok(b)) = (serde_json::to_value(&prev), serde_json::to_value(comp))
        {
            let mut out = Vec::new();
            differences("", &a, &b, &mut out);
            println!("UNDO_STEP {}", out.join(" | "));
        }
        self.undo.push(prev);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.changed_at = None;
    }

    /// A different project opened: nothing to undo or redo in it yet (Undo never brings the previous project back).
    pub fn start_over(&mut self, comp: &Composition) {
        self.undo.clear();
        self.redo.clear();
        self.committed = Some(comp.clone());
        self.changed_at = None;
    }

    /// Steps back; returns the project to load.
    pub fn undo(&mut self, current: &Composition) -> Option<Composition> {
        // An edit that hasn't settled yet is undone first.
        let pending = self.committed.as_ref().is_some_and(|c| c != current);
        let mut target = if pending { self.committed.clone()? } else { self.undo.pop()? };
        keep_lasers_live(current, &mut target);
        self.redo.push(current.clone());
        self.committed = Some(target.clone());
        self.changed_at = None;
        Some(target)
    }

    pub fn redo(&mut self, current: &Composition) -> Option<Composition> {
        let mut target = self.redo.pop()?;
        keep_lasers_live(current, &mut target);
        self.undo.push(current.clone());
        self.committed = Some(target.clone());
        self.changed_at = None;
        Some(target)
    }

    pub fn can_undo(&self, current: &Composition) -> bool {
        !self.undo.is_empty() || self.committed.as_ref().is_some_and(|c| c != current)
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// How many steps back there are (the bench prints it: a project left alone must not grow any).
    pub fn steps(&self) -> usize {
        self.undo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modulator_moving_by_itself_is_not_an_edit() {
        let mut h = History::default();
        let mut comp = Composition::default();
        comp.set_hand("Bass", 0.0);
        h.track(&comp, 0.0, false);
        // A sound band (or tracking, or live data) moves the knob every frame for a minute.
        for k in 1..600 {
            comp.modulators[0].value = (k as f32 * 0.37).fract();
            h.track(&comp, k as f64 * 0.1, false);
        }
        assert!(!h.can_undo(&comp), "nothing was edited");
        // A real edit in the middle of that is still one step, and Undo brings it back.
        comp.master_fade = 0.5;
        for k in 600..640 {
            comp.modulators[0].value = (k as f32 * 0.37).fract();
            h.track(&comp, k as f64 * 0.1, false);
        }
        assert_eq!(h.undo.len(), 1);
        assert_eq!(h.undo(&comp).unwrap().master_fade, 1.0);
    }

    /// A project left to play makes no undo steps: the blackout clock ticks, tracking reads, a fader moves by
    /// itself, and none of that is an edit. Then one real edit is one step.
    #[test]
    fn a_project_playing_by_itself_makes_no_undo_steps() {
        let mut h = History::default();
        let mut comp = Composition::starter();
        h.track(&comp, 0.0, false);
        for k in 1..600 {
            let now = k as f64 * 0.1;
            comp.step_blackout(now);
            comp.seen_text = format!("frame {k}");
            comp.seen_code = format!("code {k}");
            h.track(&comp, now, false);
        }
        assert_eq!(h.steps(), 0, "nothing was edited");
        assert!(!h.can_undo(&comp));
        comp.master_fade = 0.3;
        for k in 600..620 {
            let now = k as f64 * 0.1;
            comp.step_blackout(now);
            h.track(&comp, now, false);
        }
        assert_eq!(h.steps(), 1);
        assert_eq!(h.undo(&comp).unwrap().master_fade, 1.0);
    }

    #[test]
    fn opening_a_project_leaves_nothing_to_undo() {
        let mut h = History::default();
        let mut comp = Composition::default();
        h.track(&comp, 0.0, false);
        comp.master_fade = 0.5;
        h.track(&comp, 1.0, false);
        h.track(&comp, 2.0, false);
        assert!(h.can_undo(&comp));
        let opened = Composition { master_fade: 0.2, ..Composition::default() };
        h.start_over(&opened);
        assert!(!h.can_undo(&opened) && !h.can_redo());
        assert!(h.undo(&opened).is_none(), "Undo must not bring the previous project back");
        // Edits in the new project are steps again.
        let mut edited = opened.clone();
        edited.master_fade = 0.9;
        h.track(&edited, 3.0, false);
        h.track(&edited, 4.0, false);
        assert_eq!(h.undo(&edited).unwrap().master_fade, 0.2);
    }

    #[test]
    fn undo_never_arms_the_lasers() {
        let mut h = History::default();
        let mut comp = Composition::default();
        h.track(&comp, 0.0, false);
        comp.lasers.armed = true; // Arm: not an edit
        h.track(&comp, 1.0, false);
        h.track(&comp, 2.0, false);
        assert!(!h.can_undo(&comp), "arming made an undo step");
        comp.master_fade = 0.5; // a real edit while armed
        h.track(&comp, 3.0, false);
        h.track(&comp, 4.0, false);
        comp.lasers.armed = false; // Stop lasers
        let back = h.undo(&comp).unwrap();
        assert!(!back.lasers.armed, "Undo switched the lasers back on");
        assert!(!h.redo(&back).unwrap().lasers.armed, "Redo switched the lasers back on");
    }

    #[test]
    fn edits_settle_into_steps() {
        let mut h = History::default();
        let mut comp = Composition::default();
        h.track(&comp, 0.0, false);
        comp.master_fade = 0.5;
        h.track(&comp, 1.0, true); // dragging
        comp.master_fade = 0.3;
        h.track(&comp, 1.2, true);
        h.track(&comp, 2.0, false); // released and still
        comp.bpm = 133.0; // tempo following isn't an edit
        h.track(&comp, 3.0, false);
        h.track(&comp, 4.0, false);
        let back = h.undo(&comp).unwrap();
        assert_eq!(back.master_fade, 1.0);
        assert!(h.undo(&back).is_none());
        let fwd = h.redo(&back).unwrap();
        assert_eq!(fwd.master_fade, 0.3);
    }
}
