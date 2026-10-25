//! Saved projects on this Mac and its drives (the Projects window): found on a thread of their own, with Spotlight
//! on the Mac (fast, everywhere it indexes) plus a shallow look through the usual folders and the other drives
//! (Spotlight often skips external drives). Nothing is opened or read beyond the folder listings.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

/// A project file found.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub path: String,
    /// Last saved (seconds since 1970).
    pub modified: i64,
    pub size: u64,
}

/// How deep the folder look goes, and how many entries per place at most (a footage drive has many files).
const DEPTH: usize = 4;
const MAX_ENTRIES: usize = 40_000;

#[derive(Default)]
pub struct ProjectFiles {
    pub found: Vec<Found>,
    rx: Option<Receiver<Vec<Found>>>,
    pub searched: bool,
}

impl ProjectFiles {
    /// Looks again (in the background).
    pub fn search(&mut self) {
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let _ = std::thread::Builder::new().name("project-files".into()).spawn(move || {
            let _ = tx.send(find_all());
        });
    }

    pub fn busy(&self) -> bool {
        self.rx.is_some()
    }

    /// Takes the results when they're in. Call each frame the window is open.
    pub fn poll(&mut self) {
        if !self.searched {
            self.searched = true;
            self.search();
        }
        if let Some(found) = self.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.found = found;
            self.rx = None;
        }
    }
}

/// Not a project to list: backups ("… Backups" folders beside a project), autosaves, the bin, app support,
/// temporary copies.
fn skipped(path: &str) -> bool {
    let backups = crate::show::storage_dir().map(|d| d.to_string_lossy().into_owned());
    let in_backups = Path::new(path).parent().and_then(|p| p.file_name()).is_some_and(|n| n.to_string_lossy().ends_with(" Backups"));
    in_backups || path.contains("/.Trash/") || path.contains("/Library/") || path.contains("/.") || path.starts_with("/private/") || path.starts_with("/tmp/") || backups.is_some_and(|b| path.starts_with(&b))
}

fn find_all() -> Vec<Found> {
    let mut paths: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("/usr/bin/mdfind").arg("kMDItemFSName == '*.phosphor'c").output() {
        paths.extend(String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.is_empty()).map(PathBuf::from));
    }
    for place in places() {
        let mut budget = MAX_ENTRIES;
        walk(&place, DEPTH, &mut budget, &mut paths);
    }
    collect(paths)
}

/// Where to look by hand: the usual folders at home, and every other drive.
fn places() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        out.extend(["Documents", "Desktop", "Movies", "Music", "Downloads"].iter().map(|d| home.join(d)));
    }
    #[cfg(target_os = "macos")]
    if let Ok(rd) = std::fs::read_dir("/Volumes") {
        // (The Mac's own disk shows up there as a link to "/": skipped.)
        out.extend(rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()));
    }
    out
}

fn walk(dir: &Path, depth: usize, budget: &mut usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            if depth > 0 && !name.ends_with(".app") && name != "node_modules" {
                walk(&e.path(), depth - 1, budget, out);
            }
        } else if t.is_file() && name.to_ascii_lowercase().ends_with(".phosphor") {
            out.push(e.path());
        }
    }
}

/// Inside a source code checkout (a folder with `.git` above it): test projects, not shows.
fn in_source_checkout(path: &Path) -> bool {
    path.ancestors().skip(1).take(8).any(|d| d.join(".git").exists())
}

/// One entry per file, with its date and size, newest first.
fn collect(paths: Vec<PathBuf>) -> Vec<Found> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Found> = paths
        .into_iter()
        .filter_map(|p| {
            let path = p.to_string_lossy().into_owned();
            if skipped(&path) || in_source_checkout(&p) || !seen.insert(path.clone()) {
                return None;
            }
            let meta = std::fs::metadata(&p).ok().filter(|m| m.is_file())?;
            let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64);
            Some(Found { path, modified, size: meta.len() })
        })
        .collect();
    out.sort_by_key(|f| std::cmp::Reverse(f.modified));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Project files in nested folders are found (newest first); hidden folders, backups and other files aren't.
    #[test]
    fn finds_project_files_newest_first() {
        let dir = std::env::temp_dir().join(format!("phosphor-projfiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("shows/2026")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        std::fs::write(dir.join("shows/old.phosphor"), "{}").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(dir.join("shows/2026/new.PHOSPHOR"), "{}").unwrap();
        std::fs::write(dir.join(".hidden/secret.phosphor"), "{}").unwrap();
        std::fs::write(dir.join("shows/notes.txt"), "").unwrap();
        let mut paths = Vec::new();
        let mut budget = MAX_ENTRIES;
        walk(&dir, DEPTH, &mut budget, &mut paths);
        // (temp folders count as skipped places for the real list; check the walk and the order here.)
        let mut found: Vec<Found> = paths.iter().filter_map(|p| std::fs::metadata(p).ok().map(|m| (p, m))).map(|(p, m)| Found { path: p.to_string_lossy().into_owned(), modified: m.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64, size: m.len() }).collect();
        found.sort_by_key(|f| std::cmp::Reverse(f.modified));
        let names: Vec<&str> = found.iter().map(|f| f.path.rsplit('/').next().unwrap()).collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(names, ["new.PHOSPHOR", "old.phosphor"]);
        assert!(skipped("/Users/me/Desktop/Untitled Backups/Untitled 2026-10-25.phosphor"));
        assert!(skipped("/Users/me/.Trash/a.phosphor") && skipped("/Users/me/Library/x/a.phosphor") && !skipped("/Volumes/WD/shows/a.phosphor"));
    }
}
