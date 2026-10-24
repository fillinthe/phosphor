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
}

impl FolderWatcher {
    pub fn new(folders: Vec<String>) -> Self {
        let (tx, rx) = channel();
        Self { folders, state: Default::default(), tx, rx, scanning: false, last_scan: f64::NEG_INFINITY }
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
            std::thread::Builder::new()
                .name("phosphor-folders".into())
                .spawn(move || {
                    let mut out = Vec::new();
                    let mut states = Vec::new();
                    for f in &folders {
                        let mut budget = 3000;
                        let state = FolderState { network: on_network(Path::new(f)), missing: !Path::new(f).is_dir() };
                        states.push((f.clone(), state));
                        walk(Path::new(f), 4, &mut budget, f, &mut out);
                    }
                    let _ = tx.send((out, states));
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
    for (name, path) in crate::media::sequence::take_sequences(dir, &mut entries) {
        out.push((name, path, root.to_string()));
    }
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
        // (The whole path: a .json is only media if it's a Lottie animation, which needs the file to look inside.)
        } else if crate::media::is_supported(&p.to_string_lossy()) {
            *budget -= 1;
            out.push((name, p.to_string_lossy().to_string(), root.to_string()));
        }
    }
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
        super::walk(&dir, 2, &mut budget, "root", &mut out);
        let names: Vec<&str> = out.iter().map(|(n, _, _)| n.as_str()).collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(names, ["a.mp4"]);
    }

    #[test]
    fn this_macs_own_folders_are_not_network() {
        assert!(!super::on_network(&std::env::temp_dir()));
    }
}
