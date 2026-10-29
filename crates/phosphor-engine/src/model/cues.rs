//! Cue list (show control, MadMapper / Resolume Arena style): each cue remembers every layer's source, on/off,
//! opacity, blend, generator and effect values, the master fader and every mapping slice (on/off, opacity, corners,
//! mesh points). Going to a cue changes sources through
//! the layer's transition and glides every number over the cue's fade time. Cues can follow on by themselves
//! or fire at a timecode.

use serde::{Deserialize, Serialize};

use super::*;

/// One layer as a cue remembers it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CueLayer {
    pub id: u64,
    pub enabled: bool,
    pub source: SourceSpec,
    pub generator_values: Vec<f32>,
    pub opacity: f32,
    pub blend: BlendMode,
    /// (effect id, on, mix, values) for the effects on the layer when the cue was recorded.
    pub effects: Vec<(u64, bool, f32, Vec<f32>)>,
}

impl CueLayer {
    /// A layer as it is now.
    pub fn of(l: &LayerState) -> Self {
        Self {
            id: l.id,
            enabled: l.enabled,
            source: l.source.clone(),
            generator_values: l.generator_values.clone(),
            opacity: l.opacity,
            blend: l.blend,
            effects: l.effects.iter().map(|e| (e.id, e.enabled, e.mix, e.values.clone())).collect(),
        }
    }
}

/// One mapping slice as a cue remembers it (MadMapper scenes: switch slice layouts between songs).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CueSlice {
    pub id: u64,
    pub enabled: bool,
    pub opacity: f32,
    pub corners: [[f32; 2]; 4],
    /// Its mesh points (empty without a mesh warp).
    pub mesh: Vec<[f32; 2]>,
}

/// A laser projector as a cue remembers it: its look and brightness. (Arming isn't kept: a cue never turns lasers
/// on.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CueLaser {
    pub id: u64,
    pub look: u64,
    pub brightness: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cue {
    pub id: u64,
    pub name: String,
    pub layers: Vec<CueLayer>,
    /// Mapping slices on every output (old cues have none: the mapping stays as it is).
    pub slices: Vec<CueSlice>,
    /// Laser projectors (old cues have none: the lasers stay as they are) and the lasers' master brightness.
    pub lasers: Vec<CueLaser>,
    pub laser_master: f32,
    pub master_fade: f32,
    /// Seconds to glide into this cue.
    pub fade: f32,
    /// Go to the next cue this many seconds after this one finished fading in (0 = wait for GO).
    pub follow: f32,
    /// Fire at this timecode (seconds), when the cue list follows a timecode.
    pub at: Option<f32>,
    /// Fire every day at this local time (seconds since midnight), for installations: lights up at 18:00, off at 02:00.
    #[serde(default)]
    pub daily: Option<u32>,
    /// Which days `daily` fires on: bit 0 = Sunday … bit 6 = Saturday (0 = every day).
    #[serde(default)]
    pub days: u8,
    /// What it changes (MadMapper's cues vs scenes): the layers' sources (off = the media keeps playing, only the
    /// numbers glide), the master fade, the mapping slices. Which layers: the ones in `layers`.
    pub sets_sources: bool,
    pub sets_master: bool,
    pub sets_slices: bool,
    /// Laser looks and brightness.
    pub sets_lasers: bool,
    /// A cue group (a song, a scene): cues with the same group name in a row are one group (empty = none).
    pub group: String,
}

impl Default for Cue {
    fn default() -> Self {
        Self { id: 0, name: "Cue".into(), layers: Vec::new(), slices: Vec::new(), lasers: Vec::new(), laser_master: 1.0, sets_lasers: true, master_fade: 1.0, fade: 2.0, follow: 0.0, at: None, daily: None, days: 0, sets_sources: true, sets_master: true, sets_slices: true, group: String::new() }
    }
}

/// What drives timecoded cues.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimecodeSource {
    #[default]
    Off,
    /// Phosphor's own show clock (play / pause / reset in the Cues panel).
    ShowClock,
    /// MIDI timecode (MTC) from any MIDI input.
    Midi,
    /// Audio timecode (LTC / SMPTE) on the audio input (Audio panel), either of the first two channels.
    Ltc,
}

impl TimecodeSource {
    pub const ALL: [TimecodeSource; 4] = [TimecodeSource::Off, TimecodeSource::ShowClock, TimecodeSource::Midi, TimecodeSource::Ltc];

    pub fn name(self) -> &'static str {
        match self {
            TimecodeSource::Off => "No timecode",
            TimecodeSource::ShowClock => "Show clock",
            TimecodeSource::Midi => "MIDI timecode",
            TimecodeSource::Ltc => "Audio timecode (LTC)",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CueList {
    pub cues: Vec<Cue>,
    pub timecode: TimecodeSource,
    /// Installations: after this many minutes with nobody touching anything (mouse, keys, controllers), go to the
    /// cue `idle_cue` (its id). 0 = off.
    pub idle_minutes: f32,
    pub idle_cue: u64,
    /// Go to this cue (its id) when the project opens (0 = none).
    pub open_cue: u64,
    /// A calendar file (.ics): its events start the cues with the same name ("" = none).
    pub calendar: String,
}

impl CueList {
    /// The groups in the list, in order: (name, the index of its first cue). A name used again further down starts
    /// another group there.
    pub fn groups(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        let mut prev: Option<&str> = None;
        for (i, c) in self.cues.iter().enumerate() {
            if !c.group.is_empty() && prev != Some(c.group.as_str()) {
                out.push((c.group.clone(), i));
            }
            prev = Some(c.group.as_str());
        }
        out
    }

    /// The cue to go to after `quiet` seconds with nobody touching anything (when idle cues are on and it's time).
    pub fn idle_cue_index(&self, quiet: f64) -> Option<usize> {
        (self.idle_minutes > 0.0 && quiet >= self.idle_minutes as f64 * 60.0).then(|| self.cues.iter().position(|c| c.id == self.idle_cue)).flatten()
    }

    /// The cue to go to when the project opens.
    pub fn open_cue_index(&self) -> Option<usize> {
        (self.open_cue != 0).then(|| self.cues.iter().position(|c| c.id == self.open_cue)).flatten()
    }
}

/// A cue being faded into: where every number started.
#[derive(Clone, Debug)]
pub struct CueFade {
    pub cue: usize,
    pub start: f32,
    pub secs: f32,
    master_from: f32,
    /// (layer id, opacity from, generator values from, effect values from (id, mix, values)).
    layers: Vec<(u64, f32, Vec<f32>, Vec<(u64, f32, Vec<f32>)>)>,
    /// Layers the cue turns off: switched off when the fade ends.
    turn_off: Vec<u64>,
    /// (slice id, opacity, corners, mesh points) at the start, and slices switched off when the fade ends.
    slices: Vec<(u64, f32, [[f32; 2]; 4], Vec<[f32; 2]>)>,
    slices_off: Vec<u64>,
    /// (laser projector id, brightness at the start), and the lasers' master at the start.
    lasers: Vec<(u64, f32)>,
    laser_master_from: f32,
}

impl CueFade {
    pub fn done(&self, now: f32) -> bool {
        now - self.start >= self.secs
    }
}

impl Composition {
    /// The state of every layer now, as a cue.
    pub fn record_cue(&mut self, name: String) -> Cue {
        let layers = self.layers.iter().map(CueLayer::of).collect();
        let slices = self
            .outputs
            .iter()
            .flat_map(|o| o.mapping.surfaces.iter())
            .map(|s| CueSlice { id: s.id, enabled: s.enabled, opacity: s.opacity, corners: s.corners, mesh: s.mesh.as_ref().map_or(Vec::new(), |m| m.points.clone()) })
            .collect();
        let lasers = self.lasers.projectors.iter().map(|p| CueLaser { id: p.id, look: p.look, brightness: p.brightness }).collect();
        Cue { id: self.new_id(), name, layers, slices, lasers, laser_master: self.lasers.master, master_fade: self.master_fade, ..Default::default() }
    }

    /// Starts going to cue `index`: sources change now (through each layer's transition), numbers glide
    /// with `step_cue`. Returns None for a missing cue.
    pub fn go_to_cue(&mut self, index: usize, now: f32) -> Option<CueFade> {
        let cue = self.cues.cues.get(index)?.clone();
        let secs = cue.fade.max(0.0);
        let mut fade = CueFade {
            cue: index,
            start: now,
            secs,
            master_from: self.master_fade,
            layers: Vec::new(),
            turn_off: Vec::new(),
            slices: Vec::new(),
            slices_off: Vec::new(),
            lasers: Vec::new(),
            laser_master_from: self.lasers.master,
        };
        // Lasers: the look switches now, the brightness glides.
        for c in cue.lasers.iter().filter(|_| cue.sets_lasers) {
            let Some(p) = self.lasers.projector_mut(c.id) else { continue };
            p.look = c.look;
            fade.lasers.push((p.id, p.brightness));
        }
        for c in &cue.layers {
            let Some(l) = self.layers.iter_mut().find(|l| l.id == c.id) else { continue };
            let switching = cue.sets_sources && l.source != c.source;
            if switching {
                l.start_transition(now, secs);
                l.source = c.source.clone();
                l.generator_values = c.generator_values.clone();
                l.generator_mods.clear();
                l.active_clip = None;
                l.playing = true;
            }
            l.blend = c.blend;
            let mut from_opacity = l.opacity;
            if c.enabled && !l.enabled {
                // Coming on: fade up from nothing.
                l.enabled = true;
                from_opacity = 0.0;
            } else if !c.enabled && l.enabled {
                fade.turn_off.push(l.id);
            }
            let fx_from = l
                .effects
                .iter_mut()
                .filter_map(|e| {
                    let (_, on, _, _) = c.effects.iter().find(|(id, ..)| *id == e.id)?;
                    e.enabled = *on;
                    Some((e.id, e.mix, e.values.clone()))
                })
                .collect();
            let values_from = if switching { c.generator_values.clone() } else { l.generator_values.clone() };
            fade.layers.push((l.id, from_opacity, values_from, fx_from));
        }
        for c in cue.slices.iter().filter(|_| cue.sets_slices) {
            let Some(s) = self.slice_mut(c.id) else { continue };
            let mut from = s.opacity;
            if c.enabled && !s.enabled {
                s.enabled = true;
                from = 0.0;
            } else if !c.enabled && s.enabled {
                fade.slices_off.push(s.id);
            }
            let mesh = s.mesh.as_ref().map_or(Vec::new(), |m| m.points.clone());
            fade.slices.push((s.id, from, s.corners, mesh));
        }
        self.step_cue(&fade, now);
        Some(fade)
    }

    /// Glides every number towards the cue (call each frame until `fade.done`).
    pub fn step_cue(&mut self, fade: &CueFade, now: f32) {
        let Some(cue) = self.cues.cues.get(fade.cue).cloned() else { return };
        let t = if fade.secs <= 0.0 { 1.0 } else { ((now - fade.start) / fade.secs).clamp(0.0, 1.0) };
        // Ease in and out.
        let k = t * t * (3.0 - 2.0 * t);
        let lerp = |a: f32, b: f32| a + (b - a) * k;
        if cue.sets_master {
            self.master_fade = lerp(fade.master_from, cue.master_fade);
        }
        for (id, op_from, vals_from, fx_from) in &fade.layers {
            let (Some(c), Some(l)) = (cue.layers.iter().find(|c| c.id == *id), self.layers.iter_mut().find(|l| l.id == *id)) else { continue };
            let off = fade.turn_off.contains(id);
            l.opacity = lerp(*op_from, if off { 0.0 } else { c.opacity });
            for (i, to) in c.generator_values.iter().enumerate() {
                if let (Some(v), Some(from)) = (l.generator_values.get_mut(i), vals_from.get(i)) {
                    *v = lerp(*from, *to);
                }
            }
            for (eid, mix_from, values_from) in fx_from {
                let (Some((_, _, mix_to, values_to)), Some(e)) = (c.effects.iter().find(|(i, ..)| i == eid), l.effects.iter_mut().find(|e| e.id == *eid)) else { continue };
                e.mix = lerp(*mix_from, *mix_to);
                for (i, to) in values_to.iter().enumerate() {
                    if let (Some(v), Some(from)) = (e.values.get_mut(i), values_from.get(i)) {
                        *v = lerp(*from, *to);
                    }
                }
            }
            if t >= 1.0 && off {
                l.enabled = false;
                l.opacity = c.opacity;
            }
        }
        if cue.sets_lasers && !cue.lasers.is_empty() {
            self.lasers.master = lerp(fade.laser_master_from, cue.laser_master);
            for (id, from) in &fade.lasers {
                if let (Some(c), Some(p)) = (cue.lasers.iter().find(|c| c.id == *id), self.lasers.projector_mut(*id)) {
                    p.brightness = lerp(*from, c.brightness);
                }
            }
        }
        let lerp2 = |a: [f32; 2], b: [f32; 2]| [lerp(a[0], b[0]), lerp(a[1], b[1])];
        for (id, op_from, corners_from, mesh_from) in &fade.slices {
            let Some(c) = cue.slices.iter().find(|c| c.id == *id) else { continue };
            let Some(s) = self.slice_mut(*id) else { continue };
            let off = fade.slices_off.contains(id);
            s.opacity = lerp(*op_from, if off { 0.0 } else { c.opacity });
            for k in 0..4 {
                s.corners[k] = lerp2(corners_from[k], c.corners[k]);
            }
            // Mesh points glide when the warp has the same points as when the cue was recorded.
            if let Some(m) = &mut s.mesh
                && m.points.len() == c.mesh.len()
                && mesh_from.len() == c.mesh.len()
            {
                for (p, (from, to)) in m.points.iter_mut().zip(mesh_from.iter().zip(&c.mesh)) {
                    *p = lerp2(*from, *to);
                }
            }
            if t >= 1.0 && off {
                s.enabled = false;
                s.opacity = c.opacity;
            }
        }
    }

    /// With a timecode at `tc` seconds (after `prev`): the cue that should fire now, if any. A jump back
    /// (or forward past several cues) lands on the last cue at or before `tc`.
    /// The scheduled cue whose time of day came between `prev` and `now` (local seconds since midnight; the window
    /// wraps at midnight) on `weekday` (0 = Sunday). The latest one wins.
    pub fn scheduled_cue(&self, prev: u32, now: u32, weekday: u8) -> Option<usize> {
        // Backwards by less than half a day = the clock was set back (time sync, clocks going back): nothing fires.
        // Further back = it went past midnight.
        if now < prev && prev - now < 43_200 {
            return None;
        }
        let passed = |t: u32| if prev <= now { t > prev && t <= now } else { t > prev || t <= now };
        self.cues
            .cues
            .iter()
            .enumerate()
            .filter(|(_, c)| c.days == 0 || c.days & (1 << weekday.min(6)) != 0)
            .filter_map(|(i, c)| c.daily.filter(|t| passed(*t)).map(|t| (i, t)))
            .max_by_key(|(_, t)| if *t > now { *t as i64 - 86_400 } else { *t as i64 })
            .map(|(i, _)| i)
    }

    pub fn timecoded_cue(&self, prev: f32, tc: f32, current: Option<usize>) -> Option<usize> {
        let last_at = |t: f32| self.cues.cues.iter().enumerate().filter(|(_, c)| c.at.is_some_and(|a| a <= t)).max_by(|a, b| a.1.at.partial_cmp(&b.1.at).unwrap()).map(|(i, _)| i);
        let now = last_at(tc)?;
        // Starting (nothing run yet) or a jump: catch up with the cue that should be on now.
        let jumped = current.is_none() || tc < prev || tc - prev > 1.0;
        let crossed = self.cues.cues[now].at.is_some_and(|a| a > prev);
        ((crossed || jumped) && Some(now) != current).then_some(now)
    }
}

impl CueList {
    /// The cues as a table (one row each): number, name, fade, then-next seconds, timecode, daily time, days.
    /// For a running order in a spreadsheet; the looks themselves stay in the project.
    pub fn to_csv(&self) -> String {
        let mut out = String::from("cue,name,fade,follow,timecode,daily,days\n");
        for (i, c) in self.cues.iter().enumerate() {
            let daily = c.daily.map_or(String::new(), |t| format!("{:02}:{:02}", t / 3600, t / 60 % 60));
            let days = if c.daily.is_none() || c.days == 0 { String::new() } else { (0..7).filter(|b| c.days & (1 << b) != 0).map(|b| ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"][b]).collect::<Vec<_>>().join(" ") };
            let at = c.at.map_or(String::new(), |a| format!("{a}"));
            out.push_str(&format!("{},{},{},{},{at},{daily},{days}\n", i + 1, csv_field(&c.name), c.fade, c.follow));
        }
        out
    }

    /// Names and timings from a table like `to_csv`'s, onto the cues in order (row 1 → cue 1). Rows past the last
    /// cue are left out (a cue is a recorded look). Returns how many cues changed.
    pub fn apply_csv(&mut self, text: &str) -> Result<usize, String> {
        let mut rows = text.lines().filter(|l| !l.trim().is_empty());
        let header: Vec<String> = csv_row(rows.next().ok_or("empty file")?).iter().map(|h| h.trim().to_lowercase()).collect();
        let col = |name: &str| header.iter().position(|h| h == name);
        let (name, fade, follow, tc, daily, days) = (col("name"), col("fade"), col("follow"), col("timecode"), col("daily"), col("days"));
        if name.is_none() && fade.is_none() {
            return Err("no \"name\" or \"fade\" column".into());
        }
        let mut changed = 0;
        for (cue, row) in self.cues.iter_mut().zip(rows) {
            let f = csv_row(row);
            let get = |c: Option<usize>| c.and_then(|c| f.get(c)).map(|v| v.trim().to_string());
            if let Some(n) = get(name).filter(|n| !n.is_empty()) {
                cue.name = n;
            }
            if let Some(v) = get(fade).and_then(|v| v.parse::<f32>().ok()) {
                cue.fade = v.clamp(0.0, 600.0);
            }
            if let Some(v) = get(follow).and_then(|v| v.parse::<f32>().ok()) {
                cue.follow = v.max(0.0);
            }
            if let Some(v) = get(tc) {
                cue.at = v.parse::<f32>().ok();
            }
            if let Some(v) = get(daily) {
                cue.daily = v.split_once(':').and_then(|(h, m)| Some(h.trim().parse::<u32>().ok()?.min(23) * 3600 + m.trim().parse::<u32>().ok()?.min(59) * 60));
            }
            if let Some(v) = get(days) {
                cue.days = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"].iter().enumerate().filter(|(_, d)| v.split_whitespace().any(|w| w.eq_ignore_ascii_case(d))).fold(0, |acc, (b, _)| acc | (1 << b));
            }
            changed += 1;
        }
        Ok(changed)
    }
}

/// A CSV field, quoted when it has to be.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// One CSV line's fields (quotes and doubled quotes understood).
fn csv_row(line: &str) -> Vec<String> {
    let (mut fields, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cue keeps each laser's look and brightness and the lasers' master: going to it switches the look at once,
    /// glides the brightness, never arms the lasers; old cues (no lasers) leave them alone.
    #[test]
    fn cues_keep_laser_looks() {
        let mut comp = Composition::default();
        comp.lasers.projectors.push(crate::LaserProjector { id: 7, look: 1, brightness: 0.2, ..Default::default() });
        comp.lasers.master = 0.5;
        let cue = comp.record_cue("Song".into());
        comp.cues.cues.push(Cue { fade: 2.0, ..cue });
        comp.lasers.projectors[0].look = 2;
        comp.lasers.projectors[0].brightness = 1.0;
        comp.lasers.master = 1.0;
        let fade = comp.go_to_cue(0, 10.0).unwrap();
        assert_eq!(comp.lasers.projectors[0].look, 1, "the look switches at once");
        comp.step_cue(&fade, 11.0);
        assert!((comp.lasers.projectors[0].brightness - 0.6).abs() < 1e-4, "half way: {}", comp.lasers.projectors[0].brightness);
        comp.step_cue(&fade, 12.0);
        assert!((comp.lasers.projectors[0].brightness - 0.2).abs() < 1e-5 && (comp.lasers.master - 0.5).abs() < 1e-5);
        assert!(!comp.lasers.armed);
        let old: Cue = serde_json::from_str(r#"{"name": "Old"}"#).unwrap();
        comp.cues.cues.push(old);
        comp.lasers.projectors[0].look = 3;
        let fade = comp.go_to_cue(1, 20.0).unwrap();
        comp.step_cue(&fade, 30.0);
        assert!(comp.lasers.projectors[0].look == 3 && (comp.lasers.master - 0.5).abs() < 1e-5, "an old cue leaves the lasers be");
    }

    #[test]
    fn cue_groups_follow_the_list() {
        let mut list = CueList::default();
        for g in ["", "Intro", "Intro", "Song 1", "Song 1", "", "Intro"] {
            list.cues.push(Cue { group: g.into(), ..Default::default() });
        }
        assert_eq!(list.groups(), vec![("Intro".into(), 1), ("Song 1".into(), 3), ("Intro".into(), 6)]);
    }

    #[test]
    fn idle_and_opening_cues() {
        let mut list = CueList { cues: vec![Cue { id: 5, ..Default::default() }, Cue { id: 9, ..Default::default() }], ..Default::default() };
        assert_eq!((list.idle_cue_index(1e9), list.open_cue_index()), (None, None), "off");
        (list.idle_minutes, list.idle_cue, list.open_cue) = (2.0, 9, 5);
        assert_eq!(list.idle_cue_index(119.0), None);
        assert_eq!(list.idle_cue_index(120.0), Some(1));
        assert_eq!(list.open_cue_index(), Some(0));
    }

    #[test]
    fn a_cue_changes_only_what_it_says() {
        let mut comp = Composition::default();
        let a = comp.add_layer().id;
        let b = comp.add_layer().id;
        let movie = SourceSpec::Movie { name: "m".into(), path: Some("/x/m.mp4".into()) };
        for l in &mut comp.layers {
            l.source = movie.clone();
            l.opacity = 1.0;
        }
        let mut cue = comp.record_cue("half".into());
        cue.fade = 0.0;
        for c in &mut cue.layers {
            c.opacity = 0.5;
            c.source = SourceSpec::Generator("plasma".into());
        }
        cue.layers.retain(|c| c.id == a);
        cue.master_fade = 0.1;
        (cue.sets_sources, cue.sets_master) = (false, false);
        comp.cues.cues.push(cue);
        comp.go_to_cue(0, 0.0);
        let la = comp.layers.iter().find(|l| l.id == a).unwrap();
        assert_eq!((la.opacity, &la.source), (0.5, &movie), "the number glides, the movie keeps playing");
        assert_eq!(comp.layers.iter().find(|l| l.id == b).unwrap().opacity, 1.0, "a layer left out of the cue stays");
        assert_eq!(comp.master_fade, 1.0, "master left alone");
    }

    #[test]
    fn cues_go_out_and_come_back_as_csv() {
        let mut list = CueList::default();
        list.cues = vec![Cue { name: "Intro, slow".into(), fade: 3.0, ..Default::default() }, Cue { name: "Drop".into(), follow: 8.0, at: Some(61.5), daily: Some(18 * 3600), days: 0b100_0001, ..Default::default() }];
        let csv = list.to_csv();
        assert!(csv.contains("\"Intro, slow\"") && csv.contains("18:00") && csv.contains("Su Sa"), "{csv}");
        let mut other = CueList::default();
        other.cues = vec![Cue::default(), Cue::default(), Cue::default()];
        assert_eq!(other.apply_csv(&csv), Ok(2));
        assert_eq!(other.cues[0].name, "Intro, slow");
        assert_eq!((other.cues[1].follow, other.cues[1].at, other.cues[1].daily, other.cues[1].days), (8.0, Some(61.5), Some(18 * 3600), 0b100_0001));
        assert_eq!(other.cues[2].name, "Cue", "no row for it: unchanged");
        assert!(other.apply_csv("what,ever\n1,2").is_err());
    }

    #[test]
    fn scheduled_cues_fire_once_at_their_time() {
        let mut comp = Composition::default();
        comp.cues.cues = vec![
            Cue { daily: Some(18 * 3600), ..Default::default() },
            Cue { daily: Some(2 * 3600), days: 0b011_1110, ..Default::default() }, // weekdays only
        ];
        assert_eq!(comp.scheduled_cue(18 * 3600 - 1, 18 * 3600, 3), Some(0));
        assert_eq!(comp.scheduled_cue(18 * 3600, 18 * 3600 + 1, 3), None, "once");
        assert_eq!(comp.scheduled_cue(86_399, 2 * 3600 + 5, 1), Some(1), "across midnight (asleep a while)");
        assert_eq!(comp.scheduled_cue(2 * 3600 - 1, 2 * 3600, 0), None, "not on Sunday");
        assert_eq!(comp.scheduled_cue(18 * 3600 + 30, 18 * 3600 + 10, 3), None, "the clock went back a little: nothing again");
    }

    #[test]
    fn cues_glide_and_switch() {
        let mut comp = Composition::default();
        let id = comp.add_layer().id;
        comp.layers[0].source = SourceSpec::Generator("plasma".into());
        comp.layers[0].opacity = 0.2;
        let a = comp.record_cue("A".into());
        comp.layers[0].opacity = 1.0;
        comp.layers[0].source = SourceSpec::Generator("rings".into());
        comp.cues.cues.push(a);
        let f = comp.go_to_cue(0, 10.0).unwrap();
        assert_eq!(comp.layers[0].source, SourceSpec::Generator("plasma".into()), "source switches through the transition");
        comp.step_cue(&f, 11.0);
        assert!((comp.layers[0].opacity - 0.6).abs() < 1e-3, "half way: {}", comp.layers[0].opacity);
        comp.step_cue(&f, 12.0);
        assert!((comp.layers[0].opacity - 0.2).abs() < 1e-6);
        assert!(comp.layers.iter().any(|l| l.id == id));
    }

    #[test]
    fn cues_move_slices() {
        let mut comp = Composition::default();
        let out = comp.add_output();
        let sid = comp.new_id();
        comp.outputs.iter_mut().find(|o| o.id == out).unwrap().mapping.add_surface(sid);
        comp.slice_mut(sid).unwrap().corners[0] = [0.0, 0.0];
        let a = comp.record_cue("A".into());
        let s = comp.slice_mut(sid).unwrap();
        s.corners[0] = [0.4, 0.2];
        s.enabled = false;
        comp.cues.cues.push(a);
        let f = comp.go_to_cue(0, 0.0).unwrap();
        assert!(comp.slice(sid).unwrap().enabled, "comes on at the start of the fade");
        comp.step_cue(&f, 1.0);
        let c = comp.slice(sid).unwrap().corners[0];
        assert!((c[0] - 0.2).abs() < 1e-3 && (c[1] - 0.1).abs() < 1e-3, "half way: {c:?}");
        comp.step_cue(&f, 2.0);
        assert_eq!(comp.slice(sid).unwrap().corners[0], [0.0, 0.0]);
    }

    #[test]
    fn timecode_fires_crossed_cues_and_jumps() {
        let mut comp = Composition::default();
        for (i, at) in [Some(0.0), Some(10.0), None, Some(20.0)].into_iter().enumerate() {
            comp.cues.cues.push(Cue { id: i as u64, at, ..Default::default() });
        }
        assert_eq!(comp.timecoded_cue(9.9, 10.1, Some(0)), Some(1));
        assert_eq!(comp.timecoded_cue(10.1, 10.2, Some(1)), None);
        assert_eq!(comp.timecoded_cue(30.0, 15.0, Some(3)), Some(1), "rewind lands on the cue for that time");
        assert_eq!(comp.timecoded_cue(4.0, 4.1, None), Some(0), "starting mid-show catches up");
    }
}
