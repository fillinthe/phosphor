//! Match the beat: scan a movie's motion (see `media::rhythm`) and set the clip so its visual hits land on
//! the music's beats (loop starts at the first hit, BPM sync stretches it to one beat per hit).

use phosphor_engine::SourceSpec;

use super::State;
use crate::media::rhythm::{MatchTarget, settings_for};

impl State {
    /// Starts a scan for a layer's movie (its current source) or one of its clips.
    pub fn match_beat(&mut self, target: MatchTarget) {
        let path = match target {
            MatchTarget::Layer(id) => self.comp.layers.iter().find(|l| l.id == id).and_then(|l| movie_path(&l.source)),
            MatchTarget::Clip(id, slot) => self.comp.layers.iter().find(|l| l.id == id).and_then(|l| l.clips.get(slot)).and_then(|c| movie_path(&c.source)),
        };
        let Some(path) = path else {
            self.notice = Some("Match the beat works on movies.".into());
            return;
        };
        // Already scanned: apply right away.
        if let Some(Ok(r)) = self.rhythm.results.get(&path).cloned() {
            self.apply_rhythm(target, &r);
            return;
        }
        if !self.rhythm.is_pending(&path) {
            self.rhythm.request(&path, target);
        }
    }

    /// Every movie clip on a layer, one after the other.
    pub fn match_beat_all(&mut self, layer: u64) {
        let slots: Vec<usize> = self
            .comp
            .layers
            .iter()
            .find(|l| l.id == layer)
            .map(|l| l.clips.iter().enumerate().filter(|(_, c)| movie_path(&c.source).is_some()).map(|(i, _)| i).collect())
            .unwrap_or_default();
        for slot in slots {
            self.match_beat(MatchTarget::Clip(layer, slot));
        }
    }

    /// Movies that just landed in a row (`place_clips`): scanned when the launch setting asks for it.
    pub fn auto_match_new(&mut self, layer: u64, first_slot: usize, count: usize) {
        if !self.comp.launch.auto_beat_match {
            return;
        }
        for slot in first_slot..first_slot + count {
            self.match_beat(MatchTarget::Clip(layer, slot));
        }
    }

    /// A movie about to become a layer's source: scanned now (the source itself may land later, on the beat).
    pub fn auto_match_source(&mut self, layer: u64, source: &SourceSpec) {
        if !self.comp.launch.auto_beat_match {
            return;
        }
        if let Some(path) = movie_path(source) {
            if let Some(Ok(r)) = self.rhythm.results.get(&path).cloned() {
                self.apply_rhythm(MatchTarget::Layer(layer), &r);
            } else if !self.rhythm.is_pending(&path) {
                self.rhythm.request(&path, MatchTarget::Layer(layer));
            }
        }
    }

    /// Call every frame: finished scans set their clip or layer.
    pub(crate) fn poll_rhythm(&mut self) {
        for (_, target, r) in self.rhythm.poll() {
            match r {
                Ok(r) => self.apply_rhythm(target, &r),
                Err(e) => self.notice = Some(format!("Match the beat: {e}")),
            }
        }
    }

    fn apply_rhythm(&mut self, target: MatchTarget, r: &crate::media::rhythm::Rhythm) {
        match target {
            MatchTarget::Layer(id) => {
                if let Some(l) = self.comp.layer_mut(id) {
                    let (loop_in, beats) = settings_for(r, l.loop_out);
                    l.loop_in = loop_in;
                    l.sync_beats = beats;
                    l.loop_beats = 0.0;
                    l.looping = true;
                }
            }
            MatchTarget::Clip(id, slot) => {
                if let Some(l) = self.comp.layer_mut(id) {
                    let base = l.clip_settings();
                    let k = l.clip_slot_mut(slot);
                    let own = k.own.get_or_insert(base);
                    let (loop_in, beats) = settings_for(r, own.loop_out);
                    own.loop_in = loop_in;
                    own.sync_beats = beats;
                    own.loop_beats = 0.0;
                }
            }
        }
        self.notice = Some(format!("Visual beat: {:.0} BPM, {} hits · {:.0}% steady", r.bpm, settings_for(r, 1.0).1, r.confidence * 100.0));
    }
}

/// The file behind a movie source.
pub fn movie_path(source: &SourceSpec) -> Option<String> {
    match source {
        SourceSpec::Movie { path: Some(p), .. } => Some(p.clone()),
        _ => None,
    }
}
