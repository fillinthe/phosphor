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

/// Each file's cut as last seen in the project that's open (a cut taken back to the whole clip in it is then erased
/// everywhere; a project that simply has the file uncut isn't).
fn seen() -> &'static Mutex<HashMap<String, Cut>> {
    static SEEN: std::sync::OnceLock<Mutex<HashMap<String, Cut>>> = std::sync::OnceLock::new();
    SEEN.get_or_init(Default::default)
}

/// Another project was opened: what was seen in the last one doesn't count.
pub fn project_changed() {
    if let Ok(mut s) = seen().lock() {
        s.clear();
    }
}

/// After the media list remembered how things played: notes every cut file, and gives a file that has no
/// settings in this project the cut it had elsewhere. Writes the file only when a cut changed.
pub fn sync(comp: &mut Composition) {
    let Ok(mut cuts) = store().lock() else { return };
    let Ok(mut seen) = seen().lock() else { return };
    let mut changed = false;
    for item in &mut comp.media_bin {
        match &mut item.settings {
            Some(s) => {
                let cut = Cut { loop_in: s.loop_in, loop_out: s.loop_out, loop_beats: s.loop_beats };
                let known = cuts.get(&item.path).copied();
                let was = seen.insert(item.path.clone(), cut);
                // (Only a new cut replaces one: an older project holding the file uncut doesn't erase it. A cut
                // taken back to the whole clip here does.)
                if cut.is_cut() && known != Some(cut) {
                    cuts.insert(item.path.clone(), cut);
                    changed = true;
                } else if !cut.is_cut() && was.is_some_and(|w| w.is_cut()) && known.is_some() {
                    cuts.remove(&item.path);
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
    /// Taking a cut back to the whole clip forgets it for other projects too; opening a project that has the file
    /// uncut doesn't.
    #[test]
    fn a_reset_cut_is_forgotten_but_an_old_project_erases_nothing() {
        let path = format!("/tmp/cuts-reset-{}.mov", std::process::id());
        let mut a = Composition::default();
        a.add_media("clip", &path);
        let mut s = phosphor_engine::LayerState::default().clip_settings();
        (s.loop_in, s.loop_out) = (0.25, 0.5);
        a.media_bin[0].settings = Some(s.clone());
        sync(&mut a);
        // Another project with the file uncut opens: the cut stays known.
        project_changed();
        let mut old = Composition::default();
        old.add_media("clip", &path);
        old.media_bin[0].settings = Some(phosphor_engine::LayerState::default().clip_settings());
        sync(&mut old);
        assert!(store().lock().unwrap().contains_key(&path));
        // Back in the first, the cut is taken back to the whole clip: forgotten.
        project_changed();
        sync(&mut a);
        (s.loop_in, s.loop_out) = (0.0, 1.0);
        a.media_bin[0].settings = Some(s);
        sync(&mut a);
        assert!(!store().lock().unwrap().contains_key(&path));
    }

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
