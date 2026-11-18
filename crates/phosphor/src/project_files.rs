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

    /// Takes the results when they're in (true then). Call each frame the window is open.
    pub fn poll(&mut self) -> bool {
        if !self.searched {
            self.searched = true;
            self.search();
        }
        if let Some(found) = self.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.found = found;
            self.rx = None;
            return true;
        }
        false
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
    if let Ok(out) = crate::os::command("/usr/bin/mdfind").arg("kMDItemFSName == '*.phosphor'c").output() {
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
        // (The Mac's own disk shows up there as a link to "/": skipped. Network shares are skipped too: walking one is
        // slow, and Spotlight covers the ones it indexes.)
        out.extend(rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).filter(|p| !crate::folders::on_network(p)));
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
        let names: Vec<String> = found.iter().map(|f| std::path::Path::new(&f.path).file_name().unwrap().to_string_lossy().into_owned()).collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(names, ["new.PHOSPHOR", "old.phosphor"]);
        assert!(skipped("/Users/me/Desktop/Untitled Backups/Untitled 2026-10-25.phosphor"));
        assert!(skipped("/Users/me/.Trash/a.phosphor") && skipped("/Users/me/Library/x/a.phosphor") && !skipped("/Volumes/WD/shows/a.phosphor"));
    }

    /// A whole write replaces the file and leaves nothing beside it; a write that can't happen keeps the old file.
    #[test]
    fn whole_writes_replace_or_keep() {
        let dir = std::env::temp_dir().join(format!("phosphor-whole-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Show.phosphor");
        std::fs::write(&file, "old").unwrap();
        write_whole(&file, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no part files left");
        // Into a folder that isn't there: an error, nothing written.
        assert!(write_whole(&dir.join("gone").join("x.phosphor"), "x").is_err());
        // Many writers at once: each finishes whole.
        let writers: Vec<_> = (0..8).map(|k| { let f = file.clone(); std::thread::spawn(move || write_whole(&f, format!("writer {k}").repeat(1000)).unwrap()) }).collect();
        writers.into_iter().for_each(|w| w.join().unwrap());
        let text = std::fs::read_to_string(&file).unwrap();
        assert!((0..8).any(|k| text == format!("writer {k}").repeat(1000)), "one writer's text, whole");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The copy of a project kept on this computer (the settings folder's cache), so a project whose file is on a drive
/// that isn't plugged in still opens. Written at every open and save.
pub fn kept_copy_path(project: &str) -> Option<PathBuf> {
    let h = phosphor_engine::stable_hash(&[project.as_bytes()]);
    Some(crate::show::storage_dir()?.join("cache").join("project copies").join(format!("{h:016x}.phosphor")))
}

/// Beside a kept copy: it was saved while the project's own file couldn't be written (its drive away), so it is
/// newer than that file until a save reaches the drive again.
fn ahead_mark(copy: &Path) -> PathBuf {
    copy.with_extension("ahead")
}

/// Keeps `json` as this computer's copy of `project`. `ahead` = the project's own file doesn't have this yet.
pub fn keep_copy(project: &str, json: &str, ahead: bool) -> std::io::Result<()> {
    let path = kept_copy_path(project).ok_or_else(|| std::io::Error::other("no settings folder"))?;
    keep_copy_at(&path, json, ahead)
}

/// Writes `data` to `path` whole or not at all: into a file of its own beside it (named for this process and
/// write, so two writes at once never share one), flushed to the disk, then swapped in. A crash or a full disk
/// mid-write leaves the old file as it was, never half of the new one.
pub fn write_whole(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    use std::io::Write;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = path.file_name().map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
    let part = path.with_file_name(format!(".{name}.{}-{n}.part", std::process::id()));
    let written = std::fs::File::create(&part).and_then(|mut f| {
        f.write_all(data.as_ref())?;
        f.sync_all()
    });
    match written.and_then(|()| std::fs::rename(&part, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

fn keep_copy_at(path: &Path, json: &str, ahead: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    write_whole(path, json)?;
    let mark = ahead_mark(path);
    if ahead {
        std::fs::write(mark, "")?;
    } else {
        let _ = std::fs::remove_file(mark);
    }
    Ok(())
}

/// Keeps a fresh copy of every project in `paths` whose file is there now (Recent: a drive plugged in gives each of its
/// projects a copy, so they open when it's away later, opened lately or not). A copy saved while its drive was away
/// is newer than the file and is left alone. On a thread of its own (they may be on a slow drive).
pub fn refresh_copies(paths: Vec<String>) {
    let _ = std::thread::Builder::new().name("project-copies".into()).spawn(move || {
        for p in paths {
            let Some(copy) = kept_copy_path(&p) else { return };
            refresh_copy_at(Path::new(&p), &copy);
        }
    });
}

fn refresh_copy_at(project: &Path, copy: &Path) {
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let Some(file_time) = modified(project) else { return };
    if ahead_mark(copy).exists() || modified(copy).is_some_and(|c| c >= file_time) {
        return;
    }
    if let Ok(json) = std::fs::read_to_string(project)
        && phosphor_engine::Composition::from_json(&json).is_ok()
    {
        let _ = keep_copy_at(copy, &json, false);
    }
}

/// This computer's copy of `project`, and whether it is newer than the project's own file.
pub fn kept_copy(project: &str) -> Option<(String, bool)> {
    kept_copy_at(&kept_copy_path(project)?)
}

fn kept_copy_at(path: &Path) -> Option<(String, bool)> {
    Some((std::fs::read_to_string(path).ok()?, ahead_mark(path).exists()))
}

#[cfg(test)]
mod kept_copy_tests {
    #[test]
    fn a_copy_saved_while_the_drive_was_away_is_known_to_be_newer() {
        let dir = std::env::temp_dir().join(format!("phosphor-copies-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("a.phosphor");
        assert_eq!(super::kept_copy_at(&path), None);
        super::keep_copy_at(&path, "one", false).unwrap();
        assert_eq!(super::kept_copy_at(&path), Some(("one".to_string(), false)));
        // Saved with the drive away: this copy is ahead of the file on the drive.
        super::keep_copy_at(&path, "two", true).unwrap();
        assert_eq!(super::kept_copy_at(&path), Some(("two".to_string(), true)));
        // A save that reached the drive again: level.
        super::keep_copy_at(&path, "three", false).unwrap();
        assert_eq!(super::kept_copy_at(&path), Some(("three".to_string(), false)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn projects_whose_drive_is_there_get_a_copy() {
        let dir = std::env::temp_dir().join(format!("phosphor-refresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (project, copy) = (dir.join("show.phosphor"), dir.join("copies").join("a.phosphor"));
        let json = phosphor_engine::Composition::default().to_json();
        std::fs::write(&project, &json).unwrap();
        super::refresh_copy_at(&project, &copy);
        assert_eq!(super::kept_copy_at(&copy), Some((json.clone(), false)));
        // A copy saved with the drive away is newer: never overwritten by the older file.
        super::keep_copy_at(&copy, "work done away", true).unwrap();
        super::refresh_copy_at(&project, &copy);
        assert_eq!(super::kept_copy_at(&copy).map(|c| c.0), Some("work done away".into()));
        // Not a project (half-written, another file): no copy made.
        let (bad, bad_copy) = (dir.join("bad.phosphor"), dir.join("copies").join("b.phosphor"));
        std::fs::write(&bad, "{not json").unwrap();
        super::refresh_copy_at(&bad, &bad_copy);
        assert!(!bad_copy.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Where a project's picture is kept (a small picture of its output, saved with it): the settings folder's cache.
pub fn picture_path(project: &str) -> Option<PathBuf> {
    let h = phosphor_engine::stable_hash(&[project.as_bytes()]);
    Some(crate::show::storage_dir()?.join("cache").join("project pictures").join(format!("{h:016x}.png")))
}

/// Saves a project's picture from the output's pixels (RGBA), made small, on a thread of its own.
pub fn save_picture(project: &str, w: u32, h: u32, rgba: Vec<u8>) {
    let Some(path) = picture_path(project) else { return };
    let _ = std::thread::Builder::new().name("project-picture".into()).spawn(move || {
        let Some(img) = image::RgbaImage::from_raw(w, h, rgba) else { return };
        let small = image::imageops::thumbnail(&img, 480, (480 * h / w.max(1)).max(1));
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
        let part = path.with_extension("part.png");
        if small.save(&part).is_ok() {
            let _ = std::fs::rename(&part, &path);
        }
    });
}

/// Where a file on an external drive is now, when it isn't where its path says: macOS mounts a drive under another
/// name when the old name is still taken ("_FOOTAGE" comes back as "_FOOTAGE 1"), and the path saved in Recent
/// then points at nothing though the drive is plugged in. Looks through the drives for one whose name starts the
/// same and holds the rest of the path. None when the path is fine or nothing fits.
pub fn relocated(path: &str) -> Option<String> {
    relocated_in(std::path::Path::new("/Volumes"), path)
}

/// Is `name` the drive `drive` mounted again the way macOS names it: the same name, a space, a number
/// ("_FOOTAGE 1")? Nothing looser: "SHOW OLD" or "SHOW_BACKUP" is another drive, and a project with the same
/// path on it is another (older) project.
fn remounted(drive: &str, name: &str) -> bool {
    name.strip_prefix(drive).and_then(|rest| rest.strip_prefix(' ')).is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

fn relocated_in(volumes: &std::path::Path, path: &str) -> Option<String> {
    let p = std::path::Path::new(path);
    if p.exists() {
        return None;
    }
    let rest = p.strip_prefix(volumes).ok()?;
    let mut parts = rest.components();
    let drive = parts.next()?.as_os_str().to_string_lossy().into_owned();
    let inside = parts.as_path();
    let mut names: Vec<String> = std::fs::read_dir(volumes).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| remounted(&drive, n)).collect();
    names.sort();
    names.into_iter().map(|n| volumes.join(n).join(inside)).find(|c| c.exists()).map(|c| c.to_string_lossy().into_owned())
}

#[cfg(test)]
mod relocated_tests {
    #[test]
    fn a_drive_mounted_under_another_name_is_found() {
        let root = std::env::temp_dir().join(format!("phosphor-volumes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("_FOOTAGE 1/shows")).unwrap();
        std::fs::create_dir_all(root.join("Other")).unwrap();
        // (Drives that only begin the same are other drives: never opened in its place.)
        for other in ["_FOOTAGE OLD", "_FOOTAGE_BACKUP", "_FOOTAGE2023"] {
            std::fs::create_dir_all(root.join(other).join("shows")).unwrap();
            std::fs::write(root.join(other).join("shows/a.phosphor"), "{}").unwrap();
        }
        assert!(super::remounted("SHOW", "SHOW 2") && !super::remounted("SHOW", "SHOW") && !super::remounted("SHOW", "SHOW OLD") && !super::remounted("SHOW", "SHOW 2b") && !super::remounted("SHOW", "SHOW2"));
        std::fs::write(root.join("_FOOTAGE 1/shows/a.phosphor"), "{}").unwrap();
        let saved = root.join("_FOOTAGE/shows/a.phosphor").to_string_lossy().into_owned();
        let found = super::relocated_in(&root, &saved).unwrap();
        // (Compared as paths: Windows writes the separators its own way.)
        assert_eq!(std::path::Path::new(&found), root.join("_FOOTAGE 1").join("shows").join("a.phosphor"));
        // A file that's where it says, one on no drive at all, one on a drive that isn't there: nothing to change.
        assert_eq!(super::relocated_in(&root, &found), None);
        assert_eq!(super::relocated_in(&root, "/somewhere/else/a.phosphor"), None);
        assert_eq!(super::relocated_in(&root, &root.join("Gone/a.phosphor").to_string_lossy()), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
