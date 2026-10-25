//! Watched folders: every image/movie inside (and in subfolders) shows up in the media bin, and
//! new files appear on their own. Scanning runs on a background thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

/// (file name, full path, watched folder it came from)
pub type Found = Vec<(String, String, String)>;

/// What the last scan saw of one watched folder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FolderState {
    /// On a network drive (a NAS, a server share).
    pub network: bool,
    /// Missing at the last scan (drive unplugged, network down).
    pub missing: bool,
}

type Scan = (Found, Vec<(String, FolderState)>);

pub struct FolderWatcher {
    pub folders: Vec<String>,
    /// Per folder, from the last scan.
    pub state: std::collections::HashMap<String, FolderState>,
    tx: Sender<Scan>,
    rx: Receiver<Scan>,
    scanning: bool,
    last_scan: f64,
    /// What each folder held at the last scan, with its modification time (`walk`).
    dirs: DirCache,
}

/// Per folder: when it last changed, the media files right in it (name, path) and its subfolders. A folder whose
/// time hasn't moved is not listed again: a rescan of an external drive is one look per folder, not per file.
type DirCache = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<PathBuf, (std::time::SystemTime, Vec<(String, String)>, Vec<PathBuf>)>>>;

impl FolderWatcher {
    pub fn new(folders: Vec<String>) -> Self {
        let (tx, rx) = channel();
        Self { folders, state: Default::default(), tx, rx, scanning: false, last_scan: f64::NEG_INFINITY, dirs: Default::default() }
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

    /// Missing at the last scan: its files stay in the bin (they come back with the drive).
    pub fn missing(&self, folder: &str) -> bool {
        self.state.get(folder).is_some_and(|s| s.missing)
    }

    pub fn scanning(&self) -> bool {
        self.scanning
    }

    /// Starts a rescan every 10 s; returns files found by a finished scan.
    pub fn poll(&mut self, now: f64) -> Option<Found> {
        let found = self.rx.try_recv().ok().map(|(found, states)| {
            self.state = states.into_iter().collect();
            found
        });
        if found.is_some() {
            self.scanning = false;
        }
        if !self.scanning && !self.folders.is_empty() && now - self.last_scan > 10.0 {
            self.scanning = true;
            self.last_scan = now;
            let folders = self.folders.clone();
            let tx = self.tx.clone();
            let dirs = self.dirs.clone();
            std::thread::Builder::new()
                .name("phosphor-folders".into())
                .spawn(move || {
                    let mut out = Vec::new();
                    let mut states = Vec::new();
                    for f in &folders {
                        let mut budget = 3000;
                        let state = FolderState { network: on_network(Path::new(f)), missing: !Path::new(f).is_dir() };
                        states.push((f.clone(), state));
                        walk(Path::new(f), 4, &mut budget, f, &mut out, &dirs);
                    }
                    let _ = tx.send((out, states));
                })
                .ok();
        }
        found
    }
}

/// Does this disk update a folder's modified time when files come or go? The Mac's own formats do; exFAT / FAT
/// (many external SSDs and cards) and network shares may not, so their folders are listed every time.
fn folder_times_reliable(dir: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        let Ok(c) = std::ffi::CString::new(dir.to_string_lossy().as_bytes()) else { return false };
        let mut s: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: statfs fills the struct for an existing path; f_fstypename is a NUL-terminated name.
        if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 {
            return false;
        }
        let name = unsafe { std::ffi::CStr::from_ptr(s.f_fstypename.as_ptr()) }.to_string_lossy();
        matches!(name.as_ref(), "apfs" | "hfs")
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = dir;
        true
    }
}

fn walk(dir: &Path, depth: u32, budget: &mut usize, root: &str, out: &mut Found, dirs: &DirCache) {
    let Ok(modified) = std::fs::metadata(dir).and_then(|m| m.modified()) else { return };
    let reliable = folder_times_reliable(dir);
    let known = if reliable { dirs.lock().ok().and_then(|d| d.get(dir).filter(|(t, _, _)| *t == modified).map(|(_, f, s)| (f.clone(), s.clone()))) } else { None };
    let (files, subs) = match known {
        Some(listing) => listing,
        None => {
            let listing = list_dir(dir);
            if let Ok(mut d) = dirs.lock() {
                d.insert(dir.to_path_buf(), (modified, listing.0.clone(), listing.1.clone()));
            }
            listing
        }
    };
    for (name, path) in files {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        out.push((name, path, root.to_string()));
    }
    if depth > 0 {
        for sub in subs {
            walk(&sub, depth - 1, budget, root, out, dirs);
        }
    }
}

/// One folder's media files (name, path; numbered pictures as one sequence) and subfolders. The kind of each entry
/// comes with the listing (no extra look at each file).
fn list_dir(dir: &Path) -> (Vec<(String, String)>, Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return (Vec::new(), Vec::new()) };
    let mut files: Vec<PathBuf> = Vec::new();
    let mut subs: Vec<PathBuf> = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // (A link is followed to see what it points at.)
        let kind = e.file_type().ok().filter(|t| !t.is_symlink()).or_else(|| std::fs::metadata(e.path()).ok().map(|m| m.file_type()));
        match kind {
            Some(t) if t.is_dir() => subs.push(e.path()),
            Some(t) if t.is_file() => files.push(e.path()),
            _ => {}
        }
    }
    files.sort();
    subs.sort();
    let names: Vec<String> = files.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).collect();
    let sequences = crate::media::sequence::find(&names, crate::media::is_picture);
    let members: std::collections::HashSet<&str> = sequences.iter().flat_map(|(_, m)| m.iter().map(String::as_str)).collect();
    let mut out: Vec<(String, String)> = sequences.iter().map(|(pattern, _)| (pattern.clone(), dir.join(pattern).to_string_lossy().into_owned())).collect();
    for p in &files {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        // (The whole path: a .json is only media if it's a Lottie animation, which needs the file to look inside.)
        if !members.contains(name.as_str()) && crate::media::is_supported(&p.to_string_lossy()) {
            out.push((name, p.to_string_lossy().into_owned()));
        }
    }
    (out, subs)
}

/// A folder on a network drive (the drive isn't on this Mac). Runs on the scan thread: asking can wait on the network.
#[cfg(unix)]
pub fn on_network(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: a valid C string and a zeroed struct for statfs to fill.
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        st.f_flags & libc::MNT_LOCAL as u32 == 0
    }
    #[cfg(not(target_os = "macos"))]
    {
        // NFS, SMB / CIFS
        matches!(st.f_type as i64, 0x6969 | 0x517B | 0xFF534D42 | 0xFE534D42)
    }
}

#[cfg(not(unix))]
pub fn on_network(path: &Path) -> bool {
    path.to_string_lossy().starts_with("\\\\")
}

#[cfg(test)]
mod tests {
    /// A downloaded clip's credit (.credit.json) and other .json files beside clips aren't media.
    #[test]
    fn only_media_files_join_the_bin() {
        let dir = std::env::temp_dir().join(format!("phosphor-walk-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        for f in ["a.mp4", "a.mp4.credit.json", "notes.json", "._b.mov"] {
            std::fs::write(dir.join(f), b"{}").unwrap();
        }
        let mut out = Vec::new();
        let mut budget = 100;
        super::walk(&dir, 2, &mut budget, "root", &mut out, &Default::default());
        let names: Vec<&str> = out.iter().map(|(n, _, _)| n.as_str()).collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(names, ["a.mp4"]);
    }

    /// The Mac's own disk keeps folder times (listings can be reused); the test reads the temp folder's disk.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_macs_own_disk_keeps_folder_times() {
        assert!(super::folder_times_reliable(&std::env::temp_dir()));
    }

    #[test]
    fn this_macs_own_folders_are_not_network() {
        assert!(!super::on_network(&std::env::temp_dir()));
    }

    /// A rescan uses what it knew of a folder that hasn't changed, and still finds a file added since.
    #[test]
    fn unchanged_folders_are_not_listed_again() {
        let dir = std::env::temp_dir().join(format!("phosphor-rescan-{}", std::process::id()));
        let sub = dir.join("Intro");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("a.mp4"), b"x").unwrap();
        std::fs::write(sub.join("b.mov"), b"x").unwrap();
        let cache: super::DirCache = Default::default();
        let scan = |cache: &super::DirCache| {
            let mut out = Vec::new();
            let mut budget = 100;
            super::walk(&dir, 3, &mut budget, "root", &mut out, cache);
            let mut names: Vec<String> = out.into_iter().map(|(n, _, _)| n).collect();
            names.sort();
            names
        };
        assert_eq!(scan(&cache), ["a.mp4", "b.mov"]);
        assert_eq!(cache.lock().unwrap().len(), 2, "both folders remembered");
        // Unchanged: the remembered listing is used (a stale entry proves it: it comes back as remembered).
        let t = cache.lock().unwrap().get(&dir).unwrap().0;
        cache.lock().unwrap().insert(dir.clone(), (t, vec![("remembered.mp4".into(), "x".into())], vec![sub.clone()]));
        assert_eq!(scan(&cache), ["b.mov", "remembered.mp4"]);
        // A file added: the folder's time moves on, so it's listed again.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(dir.join("c.mp4"), b"x").unwrap();
        assert_eq!(scan(&cache), ["a.mp4", "b.mov", "c.mp4"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}