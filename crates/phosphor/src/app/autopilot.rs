//! Autopilot: layers that play their next / previous / a random clip by themselves, when the clip ends
//! or after a number of beats (rules: `LayerState::autopilot_next`).

use super::*;

/// What autopilot remembers about a layer's current clip.
#[derive(Default)]
pub struct AutoRun {
    clip: Option<u64>,
    /// Collection layers: the file playing.
    file: Option<String>,
    /// Collection layers: the file the collection itself last put on (anything else = you picked something).
    launched: Option<String>,
    started_beats: f64,
    passes: Option<u64>,
}

/// The file a layer is showing, if it's a movie or picture.
fn source_file(layer: &phosphor_engine::LayerState) -> Option<&str> {
    match &layer.source {
        phosphor_engine::SourceSpec::Movie { path, .. } | phosphor_engine::SourceSpec::Image { path, .. } => path.as_deref(),
        _ => None,
    }
}

impl State {
    /// Moves autopilot layers on when their clip is done. Call once per frame.
    pub(crate) fn run_autopilot(&mut self, now: f64) {
        let beats = self.clock.beats(now);
        let mut fire: Vec<(u64, usize)> = Vec::new();
        let mut clear: Vec<u64> = Vec::new();
        self.run_collections(now, beats);
        for layer in self.comp.layers.iter().filter(|l| l.autopilot != phosphor_engine::Autopilot::Off && l.collection.is_none()) {
            let passes = self.media.passes(layer.id);
            let run = self.autopilot.entry(layer.id).or_default();
            if run.clip != layer.active_clip {
                *run = AutoRun { clip: layer.active_clip, file: None, launched: None, started_beats: beats, passes };
            }
            // Only moves on from a clip it (or you) started; an empty layer waits for you.
            if layer.active_clip.is_none() || !layer.enabled {
                continue;
            }
            let played = beats - run.started_beats;
            let due = if layer.autopilot_beats > 0.0 {
                played >= layer.autopilot_beats as f64
            } else if let (Some(now_passes), Some(at_start)) = (passes, run.passes) {
                now_passes > at_start
            } else {
                played >= phosphor_engine::AUTOPILOT_DEFAULT_BEATS as f64
            };
            if !due {
                continue;
            }
            if layer.autopilot == phosphor_engine::Autopilot::Clear {
                clear.push(layer.id);
                continue;
            }
            // A cheap random number is plenty for "pick another clip".
            let pick = ((now * 7919.0).fract() * 0.618_034 + (beats * 0.137).fract()).fract() as f32;
            if let Some(slot) = layer.autopilot_next(pick) {
                fire.push((layer.id, slot));
            }
            // Wait for the new clip (or, if the same one is picked again, a fresh count).
            *run = AutoRun { clip: layer.active_clip, file: None, launched: None, started_beats: beats, passes: self.media.passes(layer.id) };
        }
        for (layer, slot) in fire {
            self.comp.trigger_clip(layer, slot, now as f32);
        }
        // Played once: empty the layer (with the launch fade, if Fade is on).
        let secs = if self.comp.launch.fade { self.comp.launch.fade_secs } else { 0.0 };
        for id in clear {
            if let Some(l) = self.comp.layer_mut(id) {
                l.stop(now as f32, secs);
            }
        }
        let live: std::collections::HashSet<u64> = self.comp.layers.iter().map(|l| l.id).collect();
        self.autopilot.retain(|id, _| live.contains(id));
    }

    /// Layers playing a collection: start on its first file, then step through it (next / previous / random) when
    /// the file is done (end of the movie, or the autopilot's beats; pictures after 16 beats). Autopilot off on such
    /// a layer means "next".
    fn run_collections(&mut self, now: f64, beats: f64) {
        use phosphor_engine::{Autopilot, CollectionStep};
        let mut launch: Vec<(u64, usize)> = Vec::new();
        let mut stop: Vec<u64> = Vec::new();
        for layer in self.comp.layers.iter().filter(|l| l.collection.is_some() && l.enabled) {
            let Some(name) = layer.collection.as_deref() else { continue };
            let file = source_file(layer).map(str::to_string);
            let passes = self.media.passes(layer.id);
            let run = self.autopilot.entry(layer.id).or_default();
            // You put something else on this layer (a clip, a file): it stops following the collection.
            if run.launched.is_some() && file != run.launched {
                stop.push(layer.id);
                continue;
            }
            if run.file != file {
                *run = AutoRun { clip: None, file: file.clone(), launched: run.launched.clone(), started_beats: beats, passes };
            }
            let in_it = file.as_deref().is_some_and(|f| self.comp.collection_items(name).iter().any(|&i| self.comp.media_bin[i].path == f));
            let due = if !in_it {
                // Not showing one of its files yet (just picked, or the collection changed): start now.
                true
            } else if layer.autopilot_beats > 0.0 {
                beats - run.started_beats >= layer.autopilot_beats as f64
            } else if let (Some(now_passes), Some(at_start)) = (passes, run.passes) {
                now_passes > at_start
            } else {
                beats - run.started_beats >= phosphor_engine::AUTOPILOT_DEFAULT_BEATS as f64
            };
            if !due {
                continue;
            }
            let step = match layer.autopilot {
                Autopilot::Random => CollectionStep::Random,
                Autopilot::Previous => CollectionStep::Previous,
                _ => CollectionStep::Next,
            };
            let pick = ((now * 7919.0).fract() * 0.618_034 + (beats * 0.137).fract()).fract() as f32;
            if let Some(i) = self.comp.collection_next(name, if in_it { file.as_deref() } else { None }, step, pick) {
                launch.push((layer.id, i));
                run.launched = Some(self.comp.media_bin[i].path.clone());
            }
            run.file = None;
            run.started_beats = beats;
            run.passes = None;
        }
        for id in stop {
            if let Some(l) = self.comp.layer_mut(id) {
                l.collection = None;
            }
            self.autopilot.remove(&id);
            self.notice = Some("You took over that layer: it stopped playing its collection".into());
        }
        for (layer, i) in launch {
            let Some(item) = self.comp.media_bin.get(i).cloned() else { continue };
            let Some(keep) = self.comp.layer_mut(layer).map(|l| {
                l.active_clip = None;
                l.name.clone()
            }) else {
                continue;
            };
            self.launch_source(layer, crate::media::spec_for_path(&item.path), Vec::new(), item.name, now);
            // The layer is the collection's (named after it), not each file's.
            if let Some(l) = self.comp.layer_mut(layer) {
                l.name = keep;
            }
        }
    }
}
