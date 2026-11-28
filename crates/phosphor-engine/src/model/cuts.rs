//! A movie's cut (In, Out and loop length) belongs to its file. A launch that puts another movie on a layer gives it
//! that file's cut (what the media list remembers for it, or all of it) unless the clip brings settings of its own;
//! the movies playing remember their settings first. Left on the layer, one movie's cut (a 1-beat loop, say) cut
//! short every movie played after it, and `remember_media_settings` saved it as each of those files' own cut (and
//! the app's `media::cuts` for every project). Speed, direction and BPM sync still carry over from movie to movie.
//!
//! Match the beat follows the same rule: a match for a movie that isn't on its layer yet waits in the file's memory.

use super::*;

/// Each layer's movie file just before a launch (`Composition::before_launch`).
pub(crate) struct MoviesBefore(Vec<(u64, Option<String>)>);

/// The whole movie: no In, no Out, no loop length.
const WHOLE: (f32, f32, f32) = (0.0, 1.0, 0.0);

impl Composition {
    /// Before a launch: the playing movies remember their settings (otherwise done twice a second, so a cut made
    /// just before the launch stays with its file), and which movie each layer plays, for `settle_cuts`.
    pub(crate) fn before_launch(&mut self) -> MoviesBefore {
        self.remember_media_settings();
        MoviesBefore(self.layers.iter().map(|l| (l.id, movie_file(&l.source).map(str::to_owned))).collect())
    }

    /// After a launch: a layer now playing another movie, brought by a clip without settings of its own (or put
    /// straight onto the layer, or by a cue), takes that file's cut. The same file again keeps the cut it has.
    pub(crate) fn settle_cuts(&mut self, before: &MoviesBefore) {
        for (id, was) in &before.0 {
            let Some(l) = self.layer(*id) else { continue };
            let Some(file) = movie_file(&l.source) else { continue };
            if was.as_deref() == Some(file) {
                continue;
            }
            // The clip that brought it, from any row (a row with voices plays its clips on other layers).
            if l.active_clip.is_some_and(|c| self.layers.iter().flat_map(|r| &r.clips).any(|k| k.id == c && k.own.is_some())) {
                continue;
            }
            let cut = self.file_cut(file);
            if let Some(l) = self.layer_mut(*id) {
                (l.loop_in, l.loop_out, l.loop_beats) = cut;
            }
        }
    }

    /// The cut movie file `path` plays with when nothing else says: what the media list remembers, or all of it.
    fn file_cut(&self, path: &str) -> (f32, f32, f32) {
        self.media_bin.iter().find(|m| m.path == path).and_then(|m| m.settings.as_ref()).map_or(WHOLE, |s| (s.loop_in, s.loop_out, s.loop_beats))
    }

    /// What a beat match for movie `path` starts from when the clip or file has no settings yet: the layer's (speed
    /// and direction carry over), with the layer's cut only while it plays that file, else the file's own.
    fn match_base(&self, layer: u64, path: &str) -> ClipSettings {
        let Some(l) = self.layer(layer) else { return LayerState::default().clip_settings() };
        let mut s = l.clip_settings();
        if movie_file(&l.source) != Some(path) {
            (s.loop_in, s.loop_out, s.loop_beats) = self.file_cut(path);
        }
        s
    }

    /// Match the beat for the movie `path` on `layer`: `set` turns an Out point into an In point and BPM-sync beats.
    /// Set on the layer while it plays that file; otherwise (its launch waits for the beat, or another movie came on
    /// before the scan finished) kept in the file's memory, which the launch brings back: set on the layer then, it
    /// cut the movie playing there and was remembered as that one's cut. False when it had nowhere to go (the layer
    /// plays something else and the file isn't in the media list).
    pub fn match_layer_beat(&mut self, layer: u64, path: &str, set: impl Fn(f32) -> (f32, f32)) -> bool {
        let on_layer = self.layer(layer).is_some_and(|l| movie_file(&l.source) == Some(path));
        if on_layer {
            let Some(l) = self.layer_mut(layer) else { return false };
            let (loop_in, beats) = set(l.loop_out);
            (l.loop_in, l.sync_beats, l.loop_beats, l.looping) = (loop_in, beats, 0.0, true);
            return true;
        }
        let base = self.match_base(layer, path);
        let Some(item) = self.media_bin.iter_mut().find(|m| m.path == path) else { return false };
        let s = item.settings.get_or_insert(base);
        let (loop_in, beats) = set(s.loop_out);
        (s.loop_in, s.sync_beats, s.loop_beats) = (loop_in, beats, 0.0);
        // (Looping is the layer's, not the file's: on now, so the movie loops when it comes.)
        if let Some(l) = self.layer_mut(layer) {
            l.looping = true;
        }
        true
    }

    /// Match the beat for clip `slot` of `layer`'s row (a movie, `path`): its own settings, made from `match_base`
    /// when it has none (another movie's Out point isn't this one's). False when that slot holds another file by now.
    pub fn match_clip_beat(&mut self, layer: u64, slot: usize, path: &str, set: impl Fn(f32) -> (f32, f32)) -> bool {
        let base = self.match_base(layer, path);
        let Some(clip) = self.layer_mut(layer).and_then(|l| l.clips.get_mut(slot)).filter(|k| movie_file(&k.source) == Some(path)) else { return false };
        let own = clip.own.get_or_insert(base);
        let (loop_in, beats) = set(own.loop_out);
        (own.loop_in, own.sync_beats, own.loop_beats) = (loop_in, beats, 0.0);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    fn movie(path: &str) -> SourceSpec {
        SourceSpec::Movie { name: path.into(), path: Some(path.into()) }
    }

    fn cut_of(c: &Composition, layer: u64) -> (f32, f32, f32) {
        let l = c.layer(layer).unwrap();
        (l.loop_in, l.loop_out, l.loop_beats)
    }

    fn remembered_cut(c: &Composition, path: &str) -> Option<(f32, f32, f32)> {
        c.media_bin.iter().find(|m| m.path == path).and_then(|m| m.settings.as_ref()).map(|s| (s.loop_in, s.loop_out, s.loop_beats))
    }

    /// A cut (In, Out, loop length) made while one movie plays stays with that file: the next clip without settings
    /// of its own plays all of its movie and isn't remembered as cut (report 2026-11-23: one layer's 1-beat loop
    /// spread to every file played on it, each saved as a half-second loop for every project).
    #[test]
    fn a_cut_stays_with_its_file() {
        let mut c = Composition::default();
        let (a, b) = (movie("/x/a.mp4"), movie("/x/b.mp4"));
        c.add_media("a.mp4", "/x/a.mp4");
        c.add_media("b.mp4", "/x/b.mp4");
        let id = c.add_layer().id;
        c.place_clips(id, 0, vec![("a".into(), a.clone()), ("b".into(), b), ("a again".into(), a)]);
        assert!(c.layer(id).unwrap().clips.iter().all(|k| k.own.is_none()), "nothing remembered yet");
        c.press_clip(id, 0, 0.0);
        // (Set just before the next launch, before a's memory was saved again: the launch saves it.)
        let l = c.layer_mut(id).unwrap();
        (l.loop_in, l.loop_beats, l.rate) = (0.2, 1.0, 0.5);
        // Clip b: all of b, at the layer's speed (speed still carries over).
        c.press_clip(id, 1, 1.0);
        assert_eq!(cut_of(&c, id), (0.0, 1.0, 0.0));
        assert_eq!(c.layer(id).unwrap().rate, 0.5);
        c.remember_media_settings();
        assert_eq!(remembered_cut(&c, "/x/b.mp4"), Some((0.0, 1.0, 0.0)), "b isn't remembered with a's loop");
        // Another clip of a: a's cut comes back with it.
        c.press_clip(id, 2, 2.0);
        assert_eq!(cut_of(&c, id), (0.2, 1.0, 1.0));
        // A clip with settings of its own plays with its own cut.
        let l = c.layer_mut(id).unwrap();
        let mut own = l.clip_settings();
        (own.loop_in, own.loop_beats) = (0.5, 4.0);
        l.clips[1].own = Some(own);
        c.press_clip(id, 1, 3.0);
        assert_eq!(cut_of(&c, id), (0.5, 1.0, 4.0));
    }

    /// The same for a file put straight onto a layer, a column, and a row whose clips play on other layers.
    #[test]
    fn no_launch_keeps_the_last_files_cut() {
        let mut c = Composition::default();
        let top = c.add_layer().id;
        c.set_layer_source(top, movie("/x/a.mp4"), Vec::new(), String::new(), 0.0);
        c.layer_mut(top).unwrap().loop_beats = 1.0;
        c.set_layer_source(top, movie("/x/b.mp4"), Vec::new(), String::new(), 1.0);
        assert_eq!(cut_of(&c, top), (0.0, 1.0, 0.0), "straight onto the layer");
        c.layer_mut(top).unwrap().loop_beats = 1.0;
        c.place_clips(top, 0, vec![("c".into(), movie("/x/c.mp4"))]);
        c.trigger_column(0, 2.0);
        assert_eq!(cut_of(&c, top), (0.0, 1.0, 0.0), "a column");
        // The same movie again keeps the cut it has.
        c.layer_mut(top).unwrap().loop_beats = 1.0;
        c.set_layer_source(top, movie("/x/c.mp4"), Vec::new(), String::new(), 3.0);
        assert_eq!(cut_of(&c, top), (0.0, 1.0, 1.0), "the same file");
        // A row with voices: its clip plays on the other layer, which had a loop of its own movie.
        let other = c.add_layer().id;
        c.set_layer_source(other, movie("/x/d.mp4"), Vec::new(), String::new(), 4.0);
        c.layer_mut(other).unwrap().loop_beats = 2.0;
        c.layer_mut(top).unwrap().loop_beats = 0.0;
        voices_to(&mut c, top, other);
        c.press_clip(top, 0, 5.0);
        assert_eq!(c.layer(other).unwrap().source, movie("/x/c.mp4"));
        assert_eq!(cut_of(&c, other), (0.0, 1.0, 0.0), "a row's clip on another layer");
    }

    /// Makes `row`'s clips play on `to` next (voices: next layer each time).
    fn voices_to(c: &mut Composition, row: u64, to: u64) {
        c.layer_mut(row).unwrap().voices = VoiceMode::NextLayer;
        let turn = (0..c.layers.len()).find(|&t| {
            let mut probe = c.clone();
            probe.layer_mut(row).unwrap().voice_turn = t;
            probe.voice_for(row) == to
        });
        c.layer_mut(row).unwrap().voice_turn = turn.unwrap();
    }

    /// A flash clip gives the layer back with the cut it had; autopilot's launch leaves the last cut behind too; a clip
    /// with settings of its own keeps its cut when a row with voices plays it on another layer.
    #[test]
    fn flashes_and_voices_keep_the_right_cut() {
        let mut c = Composition::default();
        let top = c.add_layer().id;
        let other = c.add_layer().id;
        c.place_clips(top, 0, vec![("a".into(), movie("/x/a.mp4")), ("flash".into(), movie("/x/f.mp4")), ("own".into(), movie("/x/o.mp4"))]);
        c.set_layer_source(top, movie("/x/z.mp4"), Vec::new(), String::new(), 0.0);
        c.layer_mut(top).unwrap().loop_beats = 4.0;
        c.trigger_clip(top, 0, 1.0);
        assert_eq!(cut_of(&c, top), (0.0, 1.0, 0.0), "autopilot's launch");
        c.layer_mut(top).unwrap().loop_beats = 2.0;
        c.layer_mut(top).unwrap().clips[1].trigger = TriggerMode::Flash;
        c.press_clip(top, 1, 2.0);
        assert_eq!(cut_of(&c, top), (0.0, 1.0, 0.0), "the flash plays all of its movie");
        c.release_clip(top, 1, 3.0);
        assert_eq!(c.layer(top).unwrap().source, movie("/x/a.mp4"));
        assert_eq!(cut_of(&c, top), (0.0, 1.0, 2.0), "a comes back with its loop");
        let l = c.layer_mut(top).unwrap();
        let mut own = l.clip_settings();
        (own.loop_in, own.loop_out, own.loop_beats) = (0.25, 0.75, 0.0);
        l.clips[2].own = Some(own);
        voices_to(&mut c, top, other);
        c.press_clip(top, 2, 4.0);
        assert_eq!(c.layer(other).unwrap().source, movie("/x/o.mp4"));
        assert_eq!(cut_of(&c, other), (0.25, 0.75, 0.0), "its own cut, on the other layer");
    }

    /// A cue that switches a layer to another movie gives it that movie's cut too (cues keep sources, not cuts).
    #[test]
    fn a_cue_brings_the_files_cut() {
        let mut c = Composition::default();
        c.add_media("b.mp4", "/x/b.mp4");
        let id = c.add_layer().id;
        c.set_layer_source(id, movie("/x/b.mp4"), Vec::new(), String::new(), 0.0);
        c.layer_mut(id).unwrap().loop_out = 0.6;
        let cue = c.record_cue("B".into());
        c.cues.cues.push(cue);
        c.set_layer_source(id, movie("/x/a.mp4"), Vec::new(), String::new(), 1.0);
        c.layer_mut(id).unwrap().loop_beats = 1.0;
        c.go_to_cue(0, 2.0);
        assert_eq!(c.layer(id).unwrap().source, movie("/x/b.mp4"));
        assert_eq!(cut_of(&c, id), (0.0, 0.6, 0.0), "b's own cut, not a's loop");
        assert_eq!(remembered_cut(&c, "/x/b.mp4"), Some((0.0, 0.6, 0.0)));
    }

    /// A beat match for a movie that isn't on its layer yet (its launch waits for the beat) waits in the file's
    /// memory: the movie playing keeps its cut, and the launch brings the match. On the layer, it's set there.
    #[test]
    fn a_match_waits_for_its_movie() {
        let mut c = Composition::default();
        c.add_media("a.mp4", "/x/a.mp4");
        c.add_media("b.mp4", "/x/b.mp4");
        let id = c.add_layer().id;
        c.set_layer_source(id, movie("/x/a.mp4"), Vec::new(), String::new(), 0.0);
        let l = c.layer_mut(id).unwrap();
        (l.loop_out, l.rate) = (0.5, 2.0);
        // The scan of b comes back while a still plays: In at a quarter, 8 beats.
        let set = |out: f32| (out * 0.25, 8.0);
        assert!(c.match_layer_beat(id, "/x/b.mp4", set));
        assert_eq!(cut_of(&c, id), (0.0, 0.5, 0.0), "a keeps its cut");
        assert_eq!(c.layer(id).unwrap().sync_beats, 0.0);
        c.set_layer_source(id, movie("/x/b.mp4"), Vec::new(), String::new(), 1.0);
        let l = c.layer(id).unwrap();
        assert_eq!((l.loop_in, l.loop_out, l.sync_beats, l.looping), (0.25, 1.0, 8.0, true), "b comes with its match, from all of b");
        assert_eq!(l.rate, 2.0, "speed carried over");
        // Now on the layer: set there.
        assert!(c.match_layer_beat(id, "/x/b.mp4", |out| (out * 0.5, 4.0)));
        assert_eq!((c.layer(id).unwrap().loop_in, c.layer(id).unwrap().sync_beats), (0.5, 4.0));
        // A file neither on the layer nor in the media list: nowhere to keep it.
        assert!(!c.match_layer_beat(id, "/x/elsewhere.mp4", set));
        assert_eq!(c.layer(id).unwrap().loop_in, 0.5, "left as it was");
    }

    /// A clip's match starts from its file's cut, not the Out point of the movie playing on the layer.
    #[test]
    fn a_clip_match_takes_its_own_files_out_point() {
        let mut c = Composition::default();
        c.add_media("b.mp4", "/x/b.mp4");
        let id = c.add_layer().id;
        c.set_layer_source(id, movie("/x/a.mp4"), Vec::new(), String::new(), 0.0);
        c.layer_mut(id).unwrap().loop_out = 0.3;
        c.place_clips(id, 0, vec![("b".into(), movie("/x/b.mp4"))]);
        assert!(c.match_clip_beat(id, 0, "/x/b.mp4", |out| (0.1, out * 8.0)));
        let own = c.layer(id).unwrap().clips[0].own.clone().unwrap();
        assert_eq!((own.loop_in, own.loop_out, own.sync_beats), (0.1, 1.0, 8.0), "all of b, not a's Out at 0.3");
        // The slot holds another file by the time the scan comes back: left alone.
        c.place_clips(id, 0, vec![("c".into(), movie("/x/c.mp4"))]);
        assert!(!c.match_clip_beat(id, 0, "/x/b.mp4", |out| (0.1, out * 8.0)));
        assert!(c.layer(id).unwrap().clips[0].own.is_none());
    }
}
