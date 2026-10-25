//! Clip preload: movies open on background threads, never on the UI thread, and the clips you're likely to trigger
//! next (the open bank, nearest the playing clip first) are kept open and paused on their first frame. A triggered
//! clip that was waiting shows on the next frame instead of 100–150 ms later.
//!
//! `want` says which movies to keep ready (at most `CAP`); `take` hands one over; `open_for` opens a layer's new
//! movie in the background and `opened` returns it when ready.

use std::collections::HashSet;
use std::sync::mpsc;

use phosphor_engine::{Composition, SourceSpec};

use super::video::VideoPlayer;

/// Movies kept ready at most (each holds a decoder and a few frames).
pub const CAP: usize = 6;

enum Opened {
    Warm(String, Result<VideoPlayer, String>),
    Layer(u64, SourceSpec, Result<VideoPlayer, String>),
}

struct Warm {
    path: String,
    player: VideoPlayer,
}

pub struct Preloader {
    warm: Vec<Warm>,
    /// Paths opening for the warm set.
    opening: HashSet<String>,
    tx: mpsc::Sender<Opened>,
    rx: mpsc::Receiver<Opened>,
    /// Layer movies that came back (layer, source, player or error), for `MediaManager` to put in place.
    ready: Vec<(u64, SourceSpec, Result<VideoPlayer, String>)>,
}

impl Default for Preloader {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Preloader { warm: Vec::new(), opening: HashSet::new(), tx, rx, ready: Vec::new() }
    }
}

impl Preloader {
    fn spawn(&self, path: String, done: impl FnOnce(String, Result<VideoPlayer, String>) -> Opened + Send + 'static) {
        let tx = self.tx.clone();
        let _ = std::thread::Builder::new().name("open movie".into()).spawn(move || {
            let result = VideoPlayer::open(&path);
            let _ = tx.send(done(path, result));
        });
    }

    /// Keep these movies open and ready (in order of importance; the first `CAP`). Others are closed.
    pub fn want(&mut self, paths: &[String]) {
        let wanted: Vec<&String> = paths.iter().take(CAP).collect();
        self.warm.retain(|w| wanted.contains(&&w.path));
        for p in wanted {
            if !self.opening.contains(p) && !self.warm.iter().any(|w| &w.path == p) {
                self.opening.insert(p.clone());
                self.spawn(p.clone(), Opened::Warm);
            }
        }
    }

    /// A ready player for `path`, if one is waiting (it's no longer kept ready).
    pub fn take(&mut self, path: &str) -> Option<VideoPlayer> {
        let i = self.warm.iter().position(|w| w.path == path)?;
        Some(self.warm.remove(i).player)
    }

    /// Opens layer `layer`'s new movie in the background (`opened` returns it).
    pub fn open_for(&mut self, layer: u64, source: SourceSpec, path: String) {
        self.spawn(path, move |_, r| Opened::Layer(layer, source, r));
    }

    /// Movies opened since the last call for layers (warm ones go to the waiting list).
    pub fn opened(&mut self) -> Vec<(u64, SourceSpec, Result<VideoPlayer, String>)> {
        for o in self.rx.try_iter() {
            match o {
                Opened::Warm(path, result) => {
                    self.opening.remove(&path);
                    // Paused on its first frame until it's triggered; a file that won't open just isn't kept.
                    if let Ok(mut player) = result {
                        player.set_playing(false);
                        self.warm.push(Warm { path, player });
                    }
                }
                Opened::Layer(layer, source, result) => self.ready.push((layer, source, result)),
            }
        }
        std::mem::take(&mut self.ready)
    }
}

/// The movies worth keeping ready: in the open comp, each layer's clips in the bank on screen (`page`, `per_page`
/// columns), nearest after the playing one first, layers taking turns; never what's already playing.
pub fn candidates(comp: &Composition, page: usize, per_page: usize) -> Vec<String> {
    let cols = page * per_page..(page + 1) * per_page;
    let mut per_layer: Vec<Vec<(usize, String)>> = Vec::new();
    for l in comp.layers.iter() {
        let playing = l.active_clip.and_then(|id| l.clips.iter().position(|c| c.id == id));
        let mut list: Vec<(usize, String)> = l
            .clips
            .iter()
            .enumerate()
            .filter(|(i, c)| cols.contains(i) && Some(*i) != playing && c.source != l.source)
            .filter_map(|(i, c)| match &c.source {
                SourceSpec::Movie { path: Some(p), .. } => {
                    // Distance after the playing clip (wrapping round the bank): the next one first.
                    let d = match playing {
                        Some(p) => (i as isize - p as isize - 1).rem_euclid(per_page.max(1) as isize) as usize,
                        None => i - cols.start,
                    };
                    Some((d, p.clone()))
                }
                _ => None,
            })
            .collect();
        list.sort_by_key(|(d, _)| *d);
        per_layer.push(list);
    }
    let mut out: Vec<String> = Vec::new();
    for rank in 0.. {
        let mut any = false;
        for list in &per_layer {
            if let Some((_, p)) = list.get(rank) {
                any = true;
                if !out.contains(p) {
                    out.push(p.clone());
                }
            }
        }
        if !any || out.len() >= CAP {
            break;
        }
    }
    out.truncate(CAP);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use phosphor_engine::Clip;

    fn movie(id: u64, path: &str) -> Clip {
        Clip { id, source: SourceSpec::Movie { name: path.into(), path: Some(path.into()) }, ..Default::default() }
    }

    #[test]
    fn the_next_clips_in_the_bank_come_first_and_layers_take_turns() {
        let mut c = Composition::default();
        let a = c.add_layer().id;
        let b = c.add_layer().id;
        let la = c.layer_mut(a).unwrap();
        la.clips = vec![movie(1, "a0"), movie(2, "a1"), movie(3, "a2"), movie(4, "a3")];
        la.active_clip = Some(2);
        la.source = la.clips[1].source.clone();
        c.layer_mut(b).unwrap().clips = vec![movie(5, "b0"), Clip::default(), movie(6, "b2")];
        let list = candidates(&c, 0, 9);
        assert_eq!(list[0], "a2", "the clip after the playing one first");
        assert_eq!(list[1], "b0");
        assert!(!list.contains(&"a1".to_string()), "not what's playing");
        assert_eq!(list.len(), 5);
        // Another bank: nothing from this one.
        assert!(candidates(&c, 1, 9).is_empty());
    }
}
