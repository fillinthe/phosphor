//! Watched folders: every image/movie inside (and in subfolders) shows up in the media bin, and
//! new files appear on their own. Scanning runs on a background thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

/// (file name, full path, watched folder it came from)
pub type Found = Vec<(String, String, String)>;

pub struct FolderWatcher {
    pub folders: Vec<String>,
    tx: Sender<Found>,
    rx: Receiver<Found>,
    scanning: bool,
    last_scan: f64,
}

impl FolderWatcher {
    pub fn new(folders: Vec<String>) -> Self {
        let (tx, rx) = channel();
        Self { folders, tx, rx, scanning: false, last_scan: f64::NEG_INFINITY }
    }

    pub fn add(&mut self, folder: String) {
        if !self.folders.contains(&folder) {
            self.folders.push(folder);
        }
        self.last_scan = f64::NEG_INFINITY;
    }

    pub fn remove(&mut self, folder: &str) {
        self.folders.retain(|f| f != folder);
    }

    pub fn scanning(&self) -> bool {
        self.scanning
    }

    /// Starts a rescan every 10 s; returns files found by a finished scan.
    pub fn poll(&mut self, now: f64) -> Option<Found> {
        let found = self.rx.try_recv().ok();
        if found.is_some() {
            self.scanning = false;
        }
        if !self.scanning && !self.folders.is_empty() && now - self.last_scan > 10.0 {
            self.scanning = true;
            self.last_scan = now;
            let folders = self.folders.clone();
            let tx = self.tx.clone();
            std::thread::Builder::new()
                .name("phosphor-folders".into())
                .spawn(move || {
                    let mut out = Vec::new();
                    for f in &folders {
                        let mut budget = 3000;
                        walk(Path::new(f), 4, &mut budget, f, &mut out);
                    }
                    let _ = tx.send(out);
                })
                .ok();
        }
        found
    }
}

fn walk(dir: &Path, depth: u32, budget: &mut usize, root: &str, out: &mut Found) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if *budget == 0 {
            return;
        }
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if depth > 0 {
                walk(&p, depth - 1, budget, root, out);
            }
        } else if crate::media::is_supported(&name) {
            *budget -= 1;
            out.push((name, p.to_string_lossy().to_string(), root.to_string()));
        }
    }
}
