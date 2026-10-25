//! A movie's cut (its In and Out points, and a loop length in beats) remembered by file, on this Mac, for every
//! project: cut a clip once and it comes back cut wherever it's used again. (Only the cut: a project's own media
//! list still remembers speed, direction and effects for itself.) Kept in `media_cuts.json` in Phosphor's settings.

use std::collections::HashMap;
use std::sync::Mutex;

use phosphor_engine::Composition;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cut {
    pub loop_in: f32,
    pub loop_out: f32,
    pub loop_beats: f32,
}

impl Cut {
    /// Anything other than the whole clip.
    pub fn is_cut(&self) -> bool {
        self.loop_in > 0.001 || self.loop_out < 0.999 || self.loop_beats > 0.0
    }
}

fn file() -> Option<std::path::PathBuf> {
    crate::show::storage_dir().map(|d| d.join("media_cuts.json"))
}

fn store() -> &'static Mutex<HashMap<String, Cut>> {
    static STORE: std::sync::OnceLock<Mutex<HashMap<String, Cut>>> = std::sync::OnceLock::new();
    STORE.get_or_init(|| Mutex::new(file().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()))
}

/// After the media list remembered how things played: notes every cut file, and gives a file that has no
/// settings in this project the cut it had elsewhere. Writes the file only when a cut changed.
pub fn sync(comp: &mut Composition) {
    let Ok(mut cuts) = store().lock() else { return };
    let mut changed = false;
    for item in &mut comp.media_bin {
        match &mut item.settings {
            Some(s) => {
                let cut = Cut { loop_in: s.loop_in, loop_out: s.loop_out, loop_beats: s.loop_beats };
                let known = cuts.get(&item.path).copied();
                // (Only a new cut replaces one: an older project holding the file uncut doesn't erase it.)
                if cut.is_cut() && known != Some(cut) {
                    cuts.insert(item.path.clone(), cut);
                    changed = true;
                }
            }
            None => {
                if let Some(c) = cuts.get(&item.path) {
                    let mut s = phosphor_engine::LayerState::default().clip_settings();
                    (s.loop_in, s.loop_out, s.loop_beats) = (c.loop_in, c.loop_out, c.loop_beats);
                    item.settings = Some(s);
                }
            }
        }
    }
    if changed && let Some(p) = file() {
        let tmp = p.with_extension("json.part");
        if std::fs::write(&tmp, serde_json::to_vec(&*cuts).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clip cut in one project comes back cut in another (and only the cut comes along).
    #[test]
    fn a_cut_follows_the_file_to_another_project() {
        let path = format!("/tmp/cuts-test-{}.mov", std::process::id());
        let mut a = Composition::default();
        a.add_media("clip", &path);
        let mut s = phosphor_engine::LayerState::default().clip_settings();
        (s.loop_in, s.loop_out, s.rate) = (0.25, 0.5, 2.0);
        a.media_bin[0].settings = Some(s);
        sync(&mut a);
        let mut b = Composition::default();
        b.add_media("clip", &path);
        sync(&mut b);
        let got = b.media_bin[0].settings.clone().expect("the cut came along");
        assert_eq!((got.loop_in, got.loop_out), (0.25, 0.5));
        assert_eq!(got.rate, 1.0, "speed isn't carried between projects");
        store().lock().unwrap().remove(&path);
    }
}
