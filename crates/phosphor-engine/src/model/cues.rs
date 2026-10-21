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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cue {
    pub id: u64,
    pub name: String,
    pub layers: Vec<CueLayer>,
    /// Mapping slices on every output (old cues have none: the mapping stays as it is).
    pub slices: Vec<CueSlice>,
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
}

impl Default for Cue {
    fn default() -> Self {
        Self { id: 0, name: "Cue".into(), layers: Vec::new(), slices: Vec::new(), master_fade: 1.0, fade: 2.0, follow: 0.0, at: None, daily: None, days: 0 }
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
}

impl CueFade {
    pub fn done(&self, now: f32) -> bool {
        now - self.start >= self.secs
    }
}

impl Composition {
    /// The state of every layer now, as a cue.
    pub fn record_cue(&mut self, name: String) -> Cue {
        let layers = self
            .layers
            .iter()
            .map(|l| CueLayer {
                id: l.id,
                enabled: l.enabled,
                source: l.source.clone(),
                generator_values: l.generator_values.clone(),
                opacity: l.opacity,
                blend: l.blend,
                effects: l.effects.iter().map(|e| (e.id, e.enabled, e.mix, e.values.clone())).collect(),
            })
            .collect();
        let slices = self
            .outputs
            .iter()
            .flat_map(|o| o.mapping.surfaces.iter())
            .map(|s| CueSlice { id: s.id, enabled: s.enabled, opacity: s.opacity, corners: s.corners, mesh: s.mesh.as_ref().map_or(Vec::new(), |m| m.points.clone()) })
            .collect();
        Cue { id: self.new_id(), name, layers, slices, master_fade: self.master_fade, ..Default::default() }
    }

    /// Starts going to cue `index`: sources change now (through each layer's transition), numbers glide
    /// with `step_cue`. Returns None for a missing cue.
    pub fn go_to_cue(&mut self, index: usize, now: f32) -> Option<CueFade> {
        let cue = self.cues.cues.get(index)?.clone();
        let secs = cue.fade.max(0.0);
        let mut fade = CueFade { cue: index, start: now, secs, master_from: self.master_fade, layers: Vec::new(), turn_off: Vec::new(), slices: Vec::new(), slices_off: Vec::new() };
        for c in &cue.layers {
            let Some(l) = self.layers.iter_mut().find(|l| l.id == c.id) else { continue };
            let switching = l.source != c.source;
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
        for c in &cue.slices {
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
        self.master_fade = lerp(fade.master_from, cue.master_fade);
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

#[cfg(test)]
mod tests {
    use super::*;

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
