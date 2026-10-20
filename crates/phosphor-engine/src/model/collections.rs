//! Media collections: named sets of files for a show. A file is in a collection when you put it there (its
//! `collections`) or when it lives in one of the collection's watched folders (new files join by themselves).
//! A layer can play a collection (`LayerState::collection`): the autopilot steps through it in order or at random.

use serde::{Deserialize, Serialize};

use super::{Composition, MediaItem};

/// A folder whose files (and subfolders' files) belong to a collection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionFolder {
    pub collection: String,
    pub folder: String,
}

impl Composition {
    /// Is this file in collection `name`: put there by hand, or inside one of its watched folders.
    pub fn in_collection(&self, item: &MediaItem, name: &str) -> bool {
        item.collections.iter().any(|c| c == name) || self.collection_folders.iter().any(|f| f.collection == name && under(&item.path, &f.folder))
    }

    /// The media bin's files in collection `name`, by name (the order a layer plays them "in order").
    pub fn collection_items(&self, name: &str) -> Vec<usize> {
        let mut out: Vec<usize> = (0..self.media_bin.len()).filter(|&i| self.in_collection(&self.media_bin[i], name)).collect();
        out.sort_by_key(|&i| self.media_bin[i].name.to_lowercase());
        out
    }

    /// The folders collection `name` watches.
    pub fn collection_folders_of(&self, name: &str) -> Vec<String> {
        self.collection_folders.iter().filter(|f| f.collection == name).map(|f| f.folder.clone()).collect()
    }

    /// A new, empty collection named `name` (or the next free "name 2", "name 3"…). Returns the name used.
    pub fn create_collection(&mut self, name: &str) -> String {
        let base = name.trim();
        let base = if base.is_empty() { "Collection" } else { base };
        let mut name = base.to_string();
        let mut n = 2;
        while self.media_collections.contains(&name) {
            name = format!("{base} {n}");
            n += 1;
        }
        self.media_collections.push(name.clone());
        name
    }

    /// Renames a collection everywhere (files, watched folders, layers playing it). False if the new name is taken.
    pub fn rename_collection(&mut self, from: &str, to: &str) -> bool {
        let to = to.trim();
        if to.is_empty() || to == from || self.media_collections.iter().any(|c| c == to) {
            return false;
        }
        for c in self.media_collections.iter_mut().filter(|c| *c == from) {
            *c = to.to_string();
        }
        for m in &mut self.media_bin {
            for c in m.collections.iter_mut().filter(|c| *c == from) {
                *c = to.to_string();
            }
        }
        for f in self.collection_folders.iter_mut().filter(|f| f.collection == from) {
            f.collection = to.to_string();
        }
        for l in self.layers.iter_mut().filter(|l| l.collection.as_deref() == Some(from)) {
            l.collection = Some(to.to_string());
        }
        true
    }

    /// Deletes a collection (the files stay in the media bin; layers playing it stop following it).
    pub fn delete_collection(&mut self, name: &str) {
        self.media_collections.retain(|c| c != name);
        for m in &mut self.media_bin {
            m.collections.retain(|c| c != name);
        }
        self.collection_folders.retain(|f| f.collection != name);
        for l in self.layers.iter_mut().filter(|l| l.collection.as_deref() == Some(name)) {
            l.collection = None;
        }
    }

    /// Puts a file in a collection (no-op if it's already there).
    pub fn add_to_collection(&mut self, item: usize, name: &str) {
        if let Some(m) = self.media_bin.get_mut(item)
            && !m.collections.iter().any(|c| c == name)
        {
            m.collections.push(name.to_string());
        }
    }

    /// Watches `folder` for collection `name` (creating the collection if needed).
    pub fn add_collection_folder(&mut self, name: &str, folder: &str) {
        if !self.media_collections.iter().any(|c| c == name) {
            self.media_collections.push(name.to_string());
        }
        if !self.collection_folders.iter().any(|f| f.collection == name && f.folder == folder) {
            self.collection_folders.push(CollectionFolder { collection: name.to_string(), folder: folder.to_string() });
        }
    }

    pub fn remove_collection_folder(&mut self, name: &str, folder: &str) {
        self.collection_folders.retain(|f| !(f.collection == name && f.folder == folder));
    }

    /// The file a layer playing a collection shows after `current` (a path): the next by name, the previous, or
    /// a random other one (`pick` 0..1). The first one when `current` isn't in it.
    pub fn collection_next(&self, name: &str, current: Option<&str>, step: CollectionStep, pick: f32) -> Option<usize> {
        let items = self.collection_items(name);
        if items.is_empty() {
            return None;
        }
        let at = current.and_then(|p| items.iter().position(|&i| self.media_bin[i].path == p));
        let n = items.len();
        let k = match (at, step) {
            (None, _) => 0,
            (Some(a), CollectionStep::Next) => (a + 1) % n,
            (Some(a), CollectionStep::Previous) => (a + n - 1) % n,
            (Some(a), CollectionStep::Random) if n > 1 => (a + 1 + ((pick.clamp(0.0, 0.999) * (n - 1) as f32) as usize)) % n,
            (Some(a), CollectionStep::Random) => a,
        };
        items.get(k).copied()
    }
}

/// Which way a collection-playing layer moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionStep {
    Next,
    Previous,
    Random,
}

/// `path` is `folder` or inside it (either slash).
fn under(path: &str, folder: &str) -> bool {
    let folder = folder.trim_end_matches(['/', '\\']);
    !folder.is_empty() && path.len() > folder.len() && path.starts_with(folder) && path[folder.len()..].starts_with(['/', '\\'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp() -> Composition {
        let mut c = Composition::default();
        for (name, path) in [("b.mov", "/show/drops/b.mov"), ("a.mov", "/show/drops/a.mov"), ("c.mp4", "/other/c.mp4"), ("d.mp4", "/show/dropsies/d.mp4")] {
            c.add_media(name, path);
        }
        c
    }

    #[test]
    fn files_join_by_hand_or_by_folder() {
        let mut c = comp();
        let name = c.create_collection("Drops");
        c.add_collection_folder(&name, "/show/drops/");
        c.add_to_collection(2, &name);
        let names: Vec<&str> = c.collection_items(&name).iter().map(|&i| c.media_bin[i].name.as_str()).collect();
        assert_eq!(names, ["a.mov", "b.mov", "c.mp4"], "folder files by name, plus the one added; not the look-alike folder");
        assert_eq!(c.create_collection("Drops"), "Drops 2");
    }

    #[test]
    fn rename_and_delete_follow_everywhere() {
        let mut c = comp();
        c.create_collection("Drops");
        c.add_collection_folder("Drops", "/show/drops");
        c.add_to_collection(2, "Drops");
        let layer = c.add_layer().id;
        c.layer_mut(layer).unwrap().collection = Some("Drops".into());
        assert!(c.rename_collection("Drops", "Show A"));
        assert_eq!(c.collection_items("Show A").len(), 3);
        assert_eq!(c.layer_mut(layer).unwrap().collection.as_deref(), Some("Show A"));
        c.delete_collection("Show A");
        assert!(c.media_collections.is_empty() && c.collection_folders.is_empty() && c.media_bin[2].collections.is_empty());
        assert_eq!(c.layer_mut(layer).unwrap().collection, None);
    }

    #[test]
    fn a_layer_steps_through_in_order_or_at_random() {
        let mut c = comp();
        c.add_collection_folder("Drops", "/show/drops");
        let first = c.collection_next("Drops", None, CollectionStep::Next, 0.0).unwrap();
        assert_eq!(c.media_bin[first].name, "a.mov");
        let second = c.collection_next("Drops", Some("/show/drops/a.mov"), CollectionStep::Next, 0.0).unwrap();
        assert_eq!(c.media_bin[second].name, "b.mov");
        let wrapped = c.collection_next("Drops", Some("/show/drops/b.mov"), CollectionStep::Next, 0.0).unwrap();
        assert_eq!(c.media_bin[wrapped].name, "a.mov");
        for pick in [0.0, 0.5, 0.99] {
            let r = c.collection_next("Drops", Some("/show/drops/a.mov"), CollectionStep::Random, pick).unwrap();
            assert_eq!(c.media_bin[r].name, "b.mov", "random never repeats the current one");
        }
    }
}
