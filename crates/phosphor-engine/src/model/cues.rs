//! Cue list (show control, MadMapper / Resolume Arena style): each cue remembers every layer's source, on/off,
//! opacity, blend, generator and effect values and the master fader. Going to a cue changes sources through
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cue {
    pub id: u64,
    pub name: String,
    pub layers: Vec<CueLayer>,
    pub master_fade: f32,
    /// Seconds to glide into this cue.
    pub fade: f32,
    /// Go to the next cue this many seconds after this one finished fading in (0 = wait for GO).
    pub follow: f32,
    /// Fire at this timecode (seconds), when the cue list follows a timecode.
    pub at: Option<f32>,
}

impl Default for Cue {
    fn default() -> Self {
        Self { id: 0, name: "Cue".into(), layers: Vec::new(), master_fade: 1.0, fade: 2.0, follow: 0.0, at: None }
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
        Cue { id: self.new_id(), name, layers, master_fade: self.master_fade, ..Default::default() }
    }

    /// Starts going to cue `index`: sources change now (through each layer's transition), numbers glide
    /// with `step_cue`. Returns None for a missing cue.
    pub fn go_to_cue(&mut self, index: usize, now: f32) -> Option<CueFade> {
        let cue = self.cues.cues.get(index)?.clone();
        let secs = cue.fade.max(0.0);
        let mut fade = CueFade { cue: index, start: now, secs, master_from: self.master_fade, layers: Vec::new(), turn_off: Vec::new() };
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
    }

    /// With a timecode at `tc` seconds (after `prev`): the cue that should fire now, if any. A jump back
    /// (or forward past several cues) lands on the last cue at or before `tc`.
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
