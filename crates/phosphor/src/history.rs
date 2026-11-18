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

/// Whether launching a clip (or putting anything else on a layer) is an undo step. Off by default (owner,
/// 2026-11-16): what plays on the layers is the show, not an edit, and Undo leaves it alone; settings, effects and
/// values are what it steps through. The brightness menu's "Undo includes clip launches" turns it on.
static UNDO_PLAYBACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn undo_playback() -> bool {
    UNDO_PLAYBACK.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_undo_playback(on: bool) {
    UNDO_PLAYBACK.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// A different thing is on this layer (another clip, another source).
fn playing_changed(a: &phosphor_engine::LayerState, b: &phosphor_engine::LayerState) -> bool {
    // (Whose effects ride on the layer changes a moment after the launch, when its fade is done: the same launch.)
    a.active_clip != b.active_clip || !same_source(&a.source, &b.source) || a.clip_effects_from != b.clip_effects_from
}

/// The same thing on the layer, live text's own words aside: a clock or a counter rewrites its text by itself,
/// and that is not something else playing.
fn same_source(a: &phosphor_engine::SourceSpec, b: &phosphor_engine::SourceSpec) -> bool {
    use phosphor_engine::{LiveText, SourceSpec::Text};
    match (a, b) {
        (Text(x), Text(y)) if x.live != LiveText::Off && x.live == y.live => {
            let mut y = y.clone();
            y.text.clone_from(&x.text);
            y.live_start = x.live_start;
            *x == y
        }
        _ => a == b,
    }
}

/// What's playing on `from`, put onto `into`: the clip, its source and its own settings and effects (what a launch
/// brings with it). Everything else about the layer (opacity, blend, its own effects, its clip grid) is left.
fn carry_playback(from: &phosphor_engine::LayerState, into: &mut phosphor_engine::LayerState) {
    into.source.clone_from(&from.source);
    into.active_clip = from.active_clip;
    into.generator_values.clone_from(&from.generator_values);
    into.generator_mods.clone_from(&from.generator_mods);
    into.clip_effects.clone_from(&from.clip_effects);
    into.clip_effects_from.clone_from(&from.clip_effects_from);
    into.flash_return.clone_from(&from.flash_return);
    into.transition.clone_from(&from.transition);
    into.voice_turn = from.voice_turn;
    (into.rate, into.direction, into.playing) = (from.rate, from.direction, from.playing);
    (into.loop_in, into.loop_out, into.loop_beats, into.sync_beats) = (from.loop_in, from.loop_out, from.loop_beats, from.sync_beats);
    into.collection.clone_from(&from.collection);
    // A clip that carries its own effects puts them on the layer when it's launched.
    let brings_effects = from.active_clip.and_then(|id| from.clips.iter().find(|c| c.id == id)).and_then(|c| c.own.as_ref()).is_some_and(|o| o.with_effects);
    if brings_effects {
        into.effects.clone_from(&from.effects);
    }
    // And the clip that was playing gets its settings written back as it leaves.
    for c in &mut into.clips {
        if let Some(src) = from.clips.iter().find(|s| s.id == c.id) {
            c.own.clone_from(&src.own);
        }
    }
}

/// A step about to be shown (Undo / Redo) keeps everything live as it is now: what moves by itself, the lasers'
/// switches, and (unless clip launches are undo steps) what's playing on each layer.
fn keep_live(current: &Composition, step: &mut Composition) {
    settle_volatile(current, step);
    if !undo_playback() {
        for l in &mut step.layers {
            if let Some(now) = current.layers.iter().find(|n| n.id == l.id).filter(|n| playing_changed(n, l)) {
                carry_playback(now, l);
            }
        }
    }
}

/// Fields that change on their own (tempo following, transitions in flight, live text) aren't edits.
fn settle_volatile(from: &Composition, into: &mut Composition) {
    into.bpm = from.bpm;
    // How the project is being looked at is not an edit either, and Undo must never move the view (owner,
    // 2026-11-16: "make sure there isn't screens shifting"): which comp is open, what's folded, the Control
    // Surface's page, which screen an output fills. A step used to fold a group back open or switch the comp tab
    // along with the edit it undid, and folding something was a step of its own.
    into.open_comp = from.open_comp;
    into.surface.page = from.surface.page;
    for c in &mut into.comps {
        if let Some(src) = from.comps.iter().find(|s| s.id == c.id) {
            c.collapsed = src.collapsed;
        }
    }
    for g in &mut into.groups {
        if let Some(src) = from.groups.iter().find(|s| s.id == g.id) {
            g.collapsed = src.collapsed;
        }
    }
    for o in &mut into.outputs {
        if let Some(src) = from.outputs.iter().find(|s| s.id == o.id) {
            o.screen.clone_from(&src.screen);
            o.frozen = src.frozen;
            for g in &mut o.mapping.groups {
                if let Some(sg) = src.mapping.groups.iter().find(|s| s.id == g.id) {
                    g.collapsed = sg.collapsed;
                }
            }
        }
    }
    // (The id counter only goes up: a launch hands ids out, and a step shown by Undo must never hand one out twice.)
    into.next_id = into.next_id.max(from.next_id);
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
            // (A Designer Value box's own number is a setting the user typed: that one is an edit. Every other
            // value is moved by something: a sound band, data, tracking, a mapped knob, a preview pad.)
            let own_setting = m.designer && m.band.is_none() && m.data.is_none();
            if !own_setting {
                m.value = src.value;
            }
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
        // A launch isn't an edit: the last step takes on what's playing now, with what the launch brought (the
        // clip's settings, its effects, what it wrote back to the clip it replaced). Only that: an edit to the
        // same layer that hasn't settled yet (a fader held while autopilot moves on) stays an edit.
        if !undo_playback() {
            for l in &mut committed.layers {
                if let Some(src) = comp.layers.iter().find(|s| s.id == l.id).filter(|s| playing_changed(s, l)) {
                    carry_playback(src, l);
                }
            }
        }
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
        // An edit that hasn't settled yet is undone first. Then back through the steps to the first one that
        // differs from what's here now: a step that only held a clip launch (kept as it is now) or a value that
        // moves by itself would be a Cmd-Z that does nothing.
        let mut target = self.unsettled(current);
        while target.is_none() {
            let mut step = self.undo.pop()?;
            keep_live(current, &mut step);
            target = (step != *current).then_some(step);
        }
        let target = target?;
        self.redo.push(current.clone());
        self.committed = Some(target.clone());
        self.changed_at = None;
        Some(target)
    }

    pub fn redo(&mut self, current: &Composition) -> Option<Composition> {
        let mut target = None;
        while target.is_none() {
            let mut step = self.redo.pop()?;
            keep_live(current, &mut step);
            target = (step != *current).then_some(step);
        }
        let target = target?;
        self.undo.push(current.clone());
        self.committed = Some(target.clone());
        self.changed_at = None;
        Some(target)
    }

    /// The last step, if the project has an edit since it that hasn't settled into a step of its own yet
    /// (compared with everything that moves by itself brought up to date: the blackout clock alone made every
    /// first Cmd-Z a step that changed nothing).
    fn unsettled(&self, current: &Composition) -> Option<Composition> {
        let mut c = self.committed.clone()?;
        keep_live(current, &mut c);
        (c != *current).then_some(c)
    }

    /// Is there anything to take back? (Cheap enough to ask every frame: `changed_at` is set by `track` while an
    /// edit is waiting to settle, so no project is copied or compared here.)
    pub fn can_undo(&self, _current: &Composition) -> bool {
        !self.undo.is_empty() || self.changed_at.is_some()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// How many steps back there are (the bench prints it: a project left alone must not grow any).
    pub fn steps(&self) -> usize {
        self.undo.len()
    }
}

/// Media brought in stays in the Media bin when a step is undone or redone: undo takes back edits, not imports
/// (owner, 2026-11-18). `restored` is the step about to replace `now`.
pub fn keep_imports(now: &Composition, restored: &mut Composition) {
    let kept: Vec<_> = now.media_bin.iter().filter(|m| !restored.media_bin.iter().any(|o| o.path == m.path)).cloned().collect();
    restored.media_bin.extend(kept);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_keeps_imported_media() {
        let mut h = History::default();
        let mut comp = Composition::default();
        h.track(&comp, 0.0, false);
        comp.media_bin.push(phosphor_engine::MediaItem { name: "a".into(), path: "/m/a.mov".into(), ..Default::default() });
        h.track(&comp, 1.0, false);
        h.track(&comp, 2.0, false);
        let mut back = h.undo(&comp).unwrap();
        assert!(back.media_bin.is_empty());
        keep_imports(&comp, &mut back);
        assert_eq!(back.media_bin.len(), 1, "Undo took the import out of Media");
        keep_imports(&comp, &mut back);
        assert_eq!(back.media_bin.len(), 1, "kept twice");
    }

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

    /// Launching clips is not an undo step, and Undo leaves what's playing alone: an effect value changed
    /// between two launches is what Cmd-Z takes back, with the clip that's on now still on.
    #[test]
    fn clip_launches_stay_out_of_undo() {
        set_undo_playback(false);
        let mut h = History::default();
        let mut comp = Composition::starter();
        let id = comp.layers[0].id;
        let t = std::cell::Cell::new(0.0);
        let tick = || {
            t.set(t.get() + 0.1);
            t.get()
        };
        let settle = |h: &mut History, comp: &Composition| {
            for _ in 0..8 {
                h.track(comp, tick(), false);
            }
        };
        settle(&mut h, &comp);
        let plays = |comp: &mut Composition, name: &str| {
            let l = comp.layer_mut(id).unwrap();
            l.source = phosphor_engine::SourceSpec::Generator(name.into());
            l.active_clip = Some(name.len() as u64);
        };
        plays(&mut comp, "noise");
        settle(&mut h, &comp);
        assert_eq!(h.steps(), 0, "a launch is not a step");
        assert!(!h.can_undo(&comp));
        // An edit still waiting to settle when a launch comes (a fader held while autopilot moves on) stays an edit.
        comp.layer_mut(id).unwrap().opacity = 0.7;
        h.track(&comp, tick(), true);
        plays(&mut comp, "rings");
        h.track(&comp, tick(), true);
        settle(&mut h, &comp);
        assert_eq!(h.steps(), 1, "the held fader became a step, the launch didn't swallow it");
        let back = h.undo(&comp).unwrap();
        assert_eq!(back.layer(id).unwrap().opacity, 1.0);
        assert_eq!(back.layer(id).unwrap().source, phosphor_engine::SourceSpec::Generator("rings".into()));
        // (Start over for the rest.)
        comp = back;
        h = History::default();
        settle(&mut h, &comp);
        comp.layer_mut(id).unwrap().opacity = 0.4;
        settle(&mut h, &comp);
        assert_eq!(h.steps(), 1, "an edit is");
        plays(&mut comp, "plasma2");
        settle(&mut h, &comp);
        assert_eq!(h.steps(), 1);
        let back = h.undo(&comp).unwrap();
        let l = back.layer(id).unwrap();
        assert_eq!(l.opacity, 1.0, "the edit is undone");
        assert_eq!(l.source, phosphor_engine::SourceSpec::Generator("plasma2".into()), "what's playing stays");
        assert!(h.undo(&back).is_none(), "nothing more: the launches were never steps");
        // With the setting on, a launch is a step like any other.
        set_undo_playback(true);
        let mut h = History::default();
        let mut comp = Composition::starter();
        settle(&mut h, &comp);
        plays(&mut comp, "noise");
        settle(&mut h, &comp);
        assert_eq!(h.steps(), 1);
        set_undo_playback(false);
    }

    /// Undo never moves the view: folding a group or switching the open comp is not a step, and a step taken back
    /// leaves what's folded and which comp is open as they are now.
    #[test]
    fn undo_never_moves_the_view() {
        let mut h = History::default();
        let mut comp = Composition::starter();
        comp.ensure_outputs();
        let a = comp.layers[0].id;
        let b = comp.add_layer().id;
        let g = comp.create_group(&[a, b]).unwrap();
        let mut t = 0.0;
        let mut settle = |h: &mut History, comp: &Composition| {
            for _ in 0..8 {
                t += 0.1;
                h.track(comp, t, false);
            }
        };
        settle(&mut h, &comp);
        comp.master_fade = 0.5;
        settle(&mut h, &comp);
        let steps = h.steps();
        comp.groups.iter_mut().find(|x| x.id == g).unwrap().collapsed = true;
        comp.open_comp = Some(77);
        comp.outputs.iter_mut().for_each(|o| o.screen = "Projector".into());
        settle(&mut h, &comp);
        assert_eq!(h.steps(), steps, "looking around is not a step");
        let back = h.undo(&comp).unwrap();
        assert_eq!(back.master_fade, 1.0, "the edit is undone");
        assert!(back.groups.iter().find(|x| x.id == g).unwrap().collapsed, "still folded");
        assert_eq!(back.open_comp, Some(77), "the same comp is open");
        assert!(back.outputs.iter().all(|o| o.screen == "Projector"), "outputs stay on their screens");
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
