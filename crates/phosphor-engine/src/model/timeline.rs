//! Keyframe timeline (MadMapper's Conductor, VDMX's timeline): tracks of keys over time, each driving one control
//! (any `ControlTarget`), with markers that stop, loop or jump. The app plays it (`State::run_timeline`) through the
//! same path every control takes.

use serde::{Deserialize, Serialize};

use super::{ControlTarget, SendTo};

/// How the value travels from a key to the next one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ease {
    /// Stays until the next key, then jumps.
    Hold,
    /// A straight line.
    #[default]
    Linear,
    /// Eases out of one key and into the next.
    Smooth,
}

impl Ease {
    pub const ALL: [Ease; 3] = [Ease::Hold, Ease::Linear, Ease::Smooth];

    pub fn name(self) -> &'static str {
        match self {
            Ease::Hold => "Hold",
            Ease::Linear => "Line",
            Ease::Smooth => "Smooth",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    /// Seconds from the start.
    pub t: f64,
    /// The control's value (0..1).
    pub v: f32,
    /// How it goes on to the next key.
    #[serde(default)]
    pub ease: Ease,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub target: ControlTarget,
    /// Kept in time order.
    pub keys: Vec<Key>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

impl Track {
    /// The value at `t`: the first key's before it starts, the last key's after it ends.
    pub fn value_at(&self, t: f64) -> Option<f32> {
        let k = &self.keys;
        let first = k.first()?;
        if t <= first.t {
            return Some(first.v);
        }
        let i = k.partition_point(|key| key.t <= t);
        let a = k[i - 1];
        let Some(b) = k.get(i) else { return Some(a.v) };
        let x = ((t - a.t) / (b.t - a.t).max(1e-9)).clamp(0.0, 1.0) as f32;
        Some(match a.ease {
            Ease::Hold => a.v,
            Ease::Linear => a.v + (b.v - a.v) * x,
            Ease::Smooth => a.v + (b.v - a.v) * x * x * (3.0 - 2.0 * x),
        })
    }

    /// Adds (or replaces, within 1/100 s) a key at `t`, keeping time order. Returns its index.
    pub fn set_key(&mut self, t: f64, v: f32) -> usize {
        let t = t.max(0.0);
        if let Some(i) = self.keys.iter().position(|k| (k.t - t).abs() < 0.01) {
            self.keys[i].v = v;
            return i;
        }
        let i = self.keys.partition_point(|k| k.t < t);
        let ease = self.keys.get(i.wrapping_sub(1)).map_or(Ease::Linear, |k| k.ease);
        self.keys.insert(i, Key { t, v, ease });
        i
    }

    /// Moves key `i` to `t` (stays in time order); returns its new index.
    pub fn move_key(&mut self, i: usize, t: f64) -> usize {
        if i >= self.keys.len() {
            return i;
        }
        let mut key = self.keys.remove(i);
        key.t = t.max(0.0);
        let j = self.keys.partition_point(|k| k.t < key.t);
        self.keys.insert(j, key);
        j
    }
}

/// What a marker does when the playhead reaches it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkerKind {
    /// Just a name on the ruler.
    #[default]
    Label,
    /// Stops there (press Play to go on).
    Pause,
    /// Jumps back to the marker before it (a loop between the two).
    LoopBack,
}

impl MarkerKind {
    pub const ALL: [MarkerKind; 3] = [MarkerKind::Label, MarkerKind::Pause, MarkerKind::LoopBack];

    pub fn name(self) -> &'static str {
        match self {
            MarkerKind::Label => "Label",
            MarkerKind::Pause => "Pause here",
            MarkerKind::LoopBack => "Loop back",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub t: f64,
    pub name: String,
    #[serde(default)]
    pub kind: MarkerKind,
}

/// Something that happens as the playhead passes a moment: a control set to `value` (play a clip, fire a column, go
/// to a cue, any setting) and / or `value` sent out (OSC, MIDI).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Event {
    pub t: f64,
    pub name: String,
    pub target: Option<ControlTarget>,
    pub send: Option<SendTo>,
    pub value: f32,
}

impl Default for Event {
    fn default() -> Self {
        Self { t: 0.0, name: String::new(), target: None, send: None, value: 1.0 }
    }
}

/// A laser projector switching to a look as the playhead reaches `t` (the Timeline's laser lanes). Looks are kept by
/// id, so adding or removing looks never moves what plays when.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LookChange {
    pub t: f64,
    pub projector: u64,
    pub look: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundTrack {
    pub path: String,
    pub start: f64,
    pub volume: f32,
}

impl Default for SoundTrack {
    fn default() -> Self {
        Self { path: String::new(), start: 0.0, volume: 1.0 }
    }
}

impl SoundTrack {
    /// Where in the sound the playhead at `t` is (None before it starts).
    pub fn position(&self, t: f64) -> Option<f64> {
        (t >= self.start).then(|| t - self.start)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Timeline {
    pub tracks: Vec<Track>,
    pub markers: Vec<Marker>,
    /// Events, in no particular order.
    pub events: Vec<Event>,
    /// A sound that plays along (music, a voice-over): the file, where it starts on the timeline (seconds) and how
    /// loud (1 = as it is).
    pub sound: Option<SoundTrack>,
    /// Seconds.
    pub length: f64,
    /// Back to the start at the end.
    pub looping: bool,
    /// Laser looks over time, per projector (kept in time order).
    pub laser_looks: Vec<LookChange>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self { tracks: Vec::new(), markers: Vec::new(), events: Vec::new(), sound: None, length: 60.0, looping: false, laser_looks: Vec::new() }
    }
}

/// What happens to the playhead going from `from` to `to` (seconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// Carry on at this time.
    At(f64),
    /// Stop here (a Pause marker, or the end without looping).
    Stop(f64),
}

impl Timeline {
    /// The look projector `projector` shows at `t`: the last change at or before it (None before its first).
    pub fn look_at(&self, projector: u64, t: f64) -> Option<u64> {
        self.laser_looks.iter().filter(|c| c.projector == projector && c.t <= t + 1e-9).max_by(|a, b| a.t.total_cmp(&b.t)).map(|c| c.look)
    }

    /// A look change at `t` (replacing one at the same moment), kept in time order; its index.
    pub fn set_look(&mut self, projector: u64, t: f64, look: u64) -> usize {
        self.laser_looks.retain(|c| !(c.projector == projector && (c.t - t).abs() < 1e-6));
        self.laser_looks.push(LookChange { t, projector, look });
        self.sort_looks();
        self.laser_looks.iter().position(|c| c.projector == projector && (c.t - t).abs() < 1e-6).expect("just added")
    }

    pub fn sort_looks(&mut self) {
        self.laser_looks.sort_by(|a, b| a.t.total_cmp(&b.t));
    }

    /// The track for `target` (made if there isn't one).
    pub fn track_mut(&mut self, target: &ControlTarget) -> &mut Track {
        if let Some(i) = self.tracks.iter().position(|t| t.target == *target) {
            return &mut self.tracks[i];
        }
        self.tracks.push(Track { target: target.clone(), keys: Vec::new(), enabled: true });
        self.tracks.last_mut().expect("just pushed")
    }

    /// Every enabled track's value at `t`.
    pub fn values_at(&self, t: f64) -> Vec<(&ControlTarget, f32)> {
        self.tracks.iter().filter(|tr| tr.enabled).filter_map(|tr| Some((&tr.target, tr.value_at(t)?))).collect()
    }

    /// Moves the playhead from `from` to `to`, minding markers and the end.
    pub fn advance(&self, from: f64, to: f64) -> Step {
        let mut passed: Vec<&Marker> = self.markers.iter().filter(|m| m.t > from && m.t <= to && m.kind != MarkerKind::Label).collect();
        passed.sort_by(|a, b| a.t.total_cmp(&b.t));
        if let Some(m) = passed.first() {
            match m.kind {
                MarkerKind::Pause => return Step::Stop(m.t),
                MarkerKind::LoopBack => {
                    let back = self.markers.iter().filter(|o| o.t < m.t).map(|o| o.t).fold(0.0, f64::max);
                    return Step::At(back + (to - m.t));
                }
                MarkerKind::Label => {}
            }
        }
        if to >= self.length {
            return if self.looping { Step::At((to - self.length).rem_euclid(self.length.max(0.01))) } else { Step::Stop(self.length) };
        }
        Step::At(to)
    }
}

impl Timeline {
    /// The events the playhead passes going from `from` towards `want` (as `advance` moves it: up to a Pause marker,
    /// back from a Loop marker, round from the end), in the order it meets them. `from` itself counts only when
    /// `inclusive` (the moment it starts playing).
    pub fn passed_events(&self, from: f64, want: f64, inclusive: bool) -> Vec<&Event> {
        let between = |a: f64, b: f64, with_a: bool| {
            let mut v: Vec<&Event> = self.events.iter().filter(|e| (e.t > a || (with_a && e.t >= a)) && e.t <= b).collect();
            v.sort_by(|x, y| x.t.total_cmp(&y.t));
            v
        };
        if want < from {
            return Vec::new();
        }
        let first = self.markers.iter().filter(|m| m.t > from && m.t <= want && m.kind != MarkerKind::Label).min_by(|a, b| a.t.total_cmp(&b.t));
        let mut out = match first {
            Some(m) if m.kind == MarkerKind::Pause => between(from, m.t, inclusive),
            Some(m) => {
                let back = self.markers.iter().filter(|o| o.t < m.t).map(|o| o.t).fold(0.0, f64::max);
                let mut v = between(from, m.t, inclusive);
                v.extend(between(back, back + (want - m.t), true));
                v
            }
            None if want >= self.length => {
                let mut v = between(from, self.length, inclusive);
                if self.looping {
                    v.extend(between(0.0, (want - self.length).rem_euclid(self.length.max(0.01)), true));
                }
                v
            }
            None => between(from, want, inclusive),
        };
        out.dedup_by(|a, b| std::ptr::eq(*a, *b));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Laser looks: each projector shows its last change at or before the playhead; nothing before the first; one
    /// change per moment; old timelines have none.
    #[test]
    fn laser_looks_follow_the_playhead() {
        let mut tl = Timeline::default();
        tl.set_look(1, 10.0, 200);
        tl.set_look(1, 0.0, 100);
        tl.set_look(2, 5.0, 300);
        assert_eq!(tl.look_at(1, 0.0), Some(100));
        assert_eq!(tl.look_at(1, 9.9), Some(100));
        assert_eq!(tl.look_at(1, 10.0), Some(200));
        assert_eq!(tl.look_at(2, 4.0), None);
        tl.set_look(1, 10.0, 400);
        assert_eq!((tl.look_at(1, 30.0), tl.laser_looks.len()), (Some(400), 3), "replaced, not added");
        assert!(tl.laser_looks.windows(2).all(|w| w[0].t <= w[1].t));
        let old: Timeline = serde_json::from_str(r#"{"length": 30.0}"#).unwrap();
        assert!(old.laser_looks.is_empty());
    }

    #[test]
    fn events_fire_as_the_playhead_passes() {
        let ev = |t: f64, name: &str| Event { t, name: name.into(), ..Default::default() };
        let mut tl = Timeline { length: 10.0, events: vec![ev(5.0, "b"), ev(0.0, "start"), ev(2.0, "a"), ev(9.5, "end")], ..Default::default() };
        let names = |v: Vec<&Event>| v.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(tl.passed_events(0.0, 2.0, true)), ["start", "a"], "from the start: its own moment too");
        assert_eq!(names(tl.passed_events(2.0, 2.5, false)), Vec::<String>::new(), "each fires once");
        assert_eq!(names(tl.passed_events(1.0, 6.0, false)), ["a", "b"], "in order");
        tl.looping = true;
        assert_eq!(names(tl.passed_events(9.0, 10.2, false)), ["end", "start"], "round the loop");
        tl.markers.push(Marker { t: 6.0, name: "Pause".into(), kind: MarkerKind::Pause });
        assert_eq!(names(tl.passed_events(4.0, 9.8, false)), ["b"], "not past a pause");
        tl.markers[0].kind = MarkerKind::LoopBack;
        tl.markers.push(Marker { t: 1.0, name: "Back here".into(), kind: MarkerKind::Label });
        assert_eq!(names(tl.passed_events(4.0, 7.5, false)), ["b", "a"], "back to the marker before, and on");
    }

    #[test]
    fn keys_ease_and_markers_steer() {
        let mut tr = Track { target: ControlTarget::MasterFade, keys: Vec::new(), enabled: true };
        tr.set_key(0.0, 0.0);
        tr.set_key(2.0, 1.0);
        assert_eq!(tr.value_at(1.0), Some(0.5), "linear half way");
        tr.keys[0].ease = Ease::Hold;
        assert_eq!(tr.value_at(1.9), Some(0.0), "held");
        tr.keys[0].ease = Ease::Smooth;
        assert!((tr.value_at(0.5).unwrap() - 0.15625).abs() < 1e-6, "eases in");
        assert_eq!(tr.value_at(5.0), Some(1.0), "after the end: the last key");
        assert_eq!(tr.set_key(2.004, 0.7), 1, "replaces the key at the same time");
        assert_eq!(tr.move_key(0, 3.0), 1, "moved past the other key");
        let mut tl = Timeline { length: 10.0, ..Default::default() };
        tl.markers.push(Marker { t: 4.0, name: "A".into(), kind: MarkerKind::Label });
        tl.markers.push(Marker { t: 6.0, name: "Loop".into(), kind: MarkerKind::LoopBack });
        assert_eq!(tl.advance(5.9, 6.1), Step::At(4.1), "loops back to the marker before");
        tl.markers.push(Marker { t: 5.0, name: "Wait".into(), kind: MarkerKind::Pause });
        assert_eq!(tl.advance(4.9, 5.2), Step::Stop(5.0));
        assert_eq!(tl.advance(9.9, 10.1), Step::Stop(10.0), "the end");
        tl.looping = true;
        assert!(matches!(tl.advance(9.9, 10.1), Step::At(t) if (t - 0.1).abs() < 1e-9));
        tl.track_mut(&ControlTarget::MasterFade).set_key(1.0, 0.3);
        assert_eq!(tl.values_at(1.0), vec![(&ControlTarget::MasterFade, 0.3)]);
    }
}
