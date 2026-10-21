//! Step tracks that fire things: each lit step of a gate track sets its `fires` targets as it starts (clip plays,
//! column fires…) and lets them go as it ends (`StepTrack::edges`). Runs once per frame on the leading machine.

use super::*;

impl State {
    pub(crate) fn run_step_fires(&mut self, now: f64) {
        let beats = self.clock.beats(now) as f32;
        let mut out: Vec<(phosphor_engine::ControlTarget, f32)> = Vec::new();
        for m in &self.comp.modulators {
            let Some(track) = m.sequence.as_ref().filter(|t| t.gate && !t.fires.is_empty() && m.signal.source == phosphor_engine::ModSource::Steps) else {
                self.step_fired.remove(&m.id);
                continue;
            };
            let last = self.step_fired.get(&m.id).copied();
            let (ended, started) = track.edges(last, beats);
            for b in &track.fires {
                if ended {
                    out.push((b.target.clone(), b.range.apply(0.0)));
                }
                if started {
                    out.push((b.target.clone(), b.range.apply(1.0)));
                }
            }
            self.step_fired.insert(m.id, track.step_at(beats));
        }
        for (target, v) in out {
            self.set_control(&target, v, now);
        }
    }
}
