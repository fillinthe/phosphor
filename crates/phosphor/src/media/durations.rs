//! How long movies are, for the media list's Sort by Length: read on a thread of its own, one file at a time (a
//! movie's header only; no copy of a clip from another drive is asked for), and kept on the media item.

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, Sender, channel};

pub struct Durations {
    tx: Sender<String>,
    rx: Receiver<(String, f32)>,
    asked: HashSet<String>,
}

impl Default for Durations {
    fn default() -> Self {
        let (tx, req_rx) = channel::<String>();
        let (done_tx, rx) = channel();
        let _ = std::thread::Builder::new().name("durations".into()).spawn(move || {
            for path in req_rx {
                let secs = if crate::media::is_movie(&path) {
                    crate::media::video::probe_input(&path).ok().map(|i| i.duration()).filter(|d| *d > 0).map_or(0.0, |d| d as f32 / 1_000_000.0)
                } else {
                    0.0
                };
                if done_tx.send((path, secs)).is_err() {
                    return;
                }
            }
        });
        Durations { tx, rx, asked: HashSet::new() }
    }
}

impl Durations {
    /// Asks for a file's length (once).
    pub fn want(&mut self, path: &str) {
        if self.asked.insert(path.to_string()) {
            let _ = self.tx.send(path.to_string());
        }
    }

    /// Lengths that came in: (path, seconds; 0 for pictures and unreadable files).
    pub fn take(&mut self) -> Vec<(String, f32)> {
        self.rx.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Movies' lengths come back from the thread (pictures and unreadable files as 0), and Length sorts longest first.
    #[test]
    fn lengths_are_read_and_sort_longest_first() {
        let ffmpeg = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/ffmpeg/bin/ffmpeg");
        if !std::path::Path::new(ffmpeg).exists() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("phosphor-durations-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut d = Durations::default();
        let mut paths = Vec::new();
        for secs in [1, 3, 2] {
            let p = dir.join(format!("c{secs}.mp4"));
            let ok = std::process::Command::new(ffmpeg).args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=64x36:rate=10", "-t", &secs.to_string(), "-pix_fmt", "yuv420p"]).arg(&p).status().is_ok_and(|s| s.success());
            assert!(ok);
            let p = p.to_string_lossy().into_owned();
            d.want(&p);
            paths.push(p);
        }
        let mut got = std::collections::HashMap::new();
        let t = std::time::Instant::now();
        while got.len() < 3 && t.elapsed() < std::time::Duration::from_secs(10) {
            got.extend(d.take());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let mut items: Vec<phosphor_engine::MediaItem> = paths.iter().map(|p| phosphor_engine::MediaItem { name: p.clone(), path: p.clone(), duration: got.get(p).copied(), ..Default::default() }).collect();
        items.sort_by_key(crate::panels::length_key);
        let order: Vec<&str> = items.iter().map(|m| m.path.rsplit('/').next().unwrap()).collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(order, ["c3.mp4", "c2.mp4", "c1.mp4"], "{got:?}");
    }
}
