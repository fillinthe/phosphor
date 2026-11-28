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
            self.apply_rhythm(&path, target, &r);
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
                self.apply_rhythm(&path, MatchTarget::Layer(layer), &r);
            } else if !self.rhythm.is_pending(&path) {
                self.rhythm.request(&path, MatchTarget::Layer(layer));
            }
        }
    }

    /// Call every frame: finished scans set their clip or layer.
    pub(crate) fn poll_rhythm(&mut self) {
        for (path, target, r) in self.rhythm.poll() {
            match r {
                Ok(r) => self.apply_rhythm(&path, target, &r),
                Err(e) => self.notice = Some(crate::ui::notice::failure(format!("Match the beat: {e}"))),
            }
        }
    }

    /// A scan of movie `path` came back: its clip, or its layer (or, while the layer plays another movie, the file's
    /// memory: `Composition::match_layer_beat`).
    fn apply_rhythm(&mut self, path: &str, target: MatchTarget, r: &crate::media::rhythm::Rhythm) {
        if !r.steady() {
            self.notice = Some(format!("No steady visual beat in this clip ({:.0}% steady): left as it was", r.confidence * 100.0));
            return;
        }
        let bpm = self.comp.bpm;
        let set = |loop_out: f32| settings_for(r, loop_out, bpm);
        let done = match target {
            MatchTarget::Layer(id) => self.comp.match_layer_beat(id, path, set),
            MatchTarget::Clip(id, slot) => self.comp.match_clip_beat(id, slot, path, set),
        };
        if !done {
            self.notice = Some("Match the beat: that movie isn't on its layer any more, so it was left as it was".into());
            return;
        }
        let per = crate::media::rhythm::beats_per_hit(r, self.comp.bpm);
        let how = if per == 1.0 { "a hit every beat".to_string() } else if per > 1.0 { format!("a hit every {per:.0} beats") } else { "two hits a beat".to_string() };
        self.notice = Some(format!("Visual beat: {:.0} BPM · {:.0}% steady · {how} at {:.0} BPM", r.bpm, r.confidence * 100.0, self.comp.bpm));
    }
}

/// The file behind a movie source.
pub fn movie_path(source: &SourceSpec) -> Option<String> {
    phosphor_engine::movie_file(source).map(str::to_owned)
}
