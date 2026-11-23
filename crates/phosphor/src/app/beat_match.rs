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
                Err(e) => self.notice = Some(format!("Match the beat: {e}")),
            }
        }
    }

    /// `path` is the movie that was scanned.
    fn apply_rhythm(&mut self, path: &str, target: MatchTarget, r: &crate::media::rhythm::Rhythm) {
        if !r.steady() {
            self.notice = Some(format!("No steady visual beat in this clip ({:.0}% steady): left as it was", r.confidence * 100.0));
            return;
        }
        match target {
            MatchTarget::Layer(id) => {
                let bpm = self.comp.bpm;
                let on_layer = self.comp.layer(id).and_then(|l| movie_path(&l.source)).as_deref() == Some(path);
                let waiting = if on_layer { None } else { self.comp.media_bin.iter().position(|m| m.path == path) };
                if let Some(i) = waiting {
                    // The movie isn't on the layer yet (its launch waits for the beat, or the scan was ready before the
                    // drop): the match waits in the file's memory, which the launch brings back. Set on the layer now,
                    // it would cut the movie still playing there short, and be remembered as that file's cut.
                    let base = self.match_base(id, path);
                    let s = self.comp.media_bin[i].settings.get_or_insert(base);
                    let (loop_in, beats) = settings_for(r, s.loop_out, bpm);
                    (s.loop_in, s.sync_beats, s.loop_beats) = (loop_in, beats, 0.0);
                    if let Some(l) = self.comp.layer_mut(id) {
                        l.looping = true;
                    }
                } else if on_layer && let Some(l) = self.comp.layer_mut(id) {
                    let (loop_in, beats) = settings_for(r, l.loop_out, bpm);
                    l.loop_in = loop_in;
                    l.sync_beats = beats;
                    l.loop_beats = 0.0;
                    l.looping = true;
                }
            }
            MatchTarget::Clip(id, slot) => {
                let bpm = self.comp.bpm;
                let base = self.match_base(id, path);
                if let Some(l) = self.comp.layer_mut(id) {
                    let k = l.clip_slot_mut(slot);
                    let own = k.own.get_or_insert(base);
                    let (loop_in, beats) = settings_for(r, own.loop_out, bpm);
                    own.loop_in = loop_in;
                    own.sync_beats = beats;
                    own.loop_beats = 0.0;
                }
            }
        }
        let per = crate::media::rhythm::beats_per_hit(r, self.comp.bpm);
        let how = if per == 1.0 { "a hit every beat".to_string() } else if per > 1.0 { format!("a hit every {per:.0} beats") } else { "two hits a beat".to_string() };
        self.notice = Some(format!("Visual beat: {:.0} BPM · {:.0}% steady · {how} at {:.0} BPM", r.bpm, r.confidence * 100.0, self.comp.bpm));
    }

    /// What a match for `path` starts from when the clip or file has no settings yet: the layer's (speed and
    /// direction carry over), with the cut only if the layer is playing that file (another movie's cut isn't this one's).
    fn match_base(&self, layer: u64, path: &str) -> phosphor_engine::ClipSettings {
        let Some(l) = self.comp.layer(layer) else { return phosphor_engine::LayerState::default().clip_settings() };
        let mut s = l.clip_settings();
        if movie_path(&l.source).as_deref() != Some(path) {
            (s.loop_in, s.loop_out, s.loop_beats) = (0.0, 1.0, 0.0);
        }
        s
    }
}

/// The file behind a movie source.
pub fn movie_path(source: &SourceSpec) -> Option<String> {
    match source {
        SourceSpec::Movie { path: Some(p), .. } => Some(p.clone()),
        _ => None,
    }
}
