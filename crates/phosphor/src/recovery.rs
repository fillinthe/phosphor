//! Crash recovery. While Phosphor runs it keeps a rolling snapshot of the show and a "running"
//! marker. A tiny watchdog process (no window) reopens the app if it dies without a normal quit,
//! and the next launch restores the snapshot: project, layout, output windows on their screens,
//! session and audio input.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::panels::Tab;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OutputWindowState {
    pub id: u64,
    pub open: bool,
    pub display: Option<u32>,
    pub fullscreen: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub comp: Option<phosphor_engine::Composition>,
    pub dock: Option<serde_json::Value>,
    pub detached: Vec<Tab>,
    pub outputs: Vec<OutputWindowState>,
    pub session: bool,
    pub audio_open: bool,
    pub audio_device: Option<String>,
    pub streaming: bool,
    pub selected: Option<u64>,
}

fn dir() -> Option<PathBuf> {
    eframe::storage_dir("Phosphor")
}

fn snapshot_path() -> Option<PathBuf> {
    dir().map(|d| d.join("recovery.json"))
}

fn lock_path() -> Option<PathBuf> {
    dir().map(|d| d.join("running.lock"))
}

fn crashes_path() -> Option<PathBuf> {
    dir().map(|d| d.join("crashes.txt"))
}

pub fn crash_log_path() -> Option<PathBuf> {
    dir().map(|d| d.join("crash.log"))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Marks this run as live. Returns the last snapshot if the previous run ended without a clean quit.
pub fn begin_session() -> Option<Snapshot> {
    let lock = lock_path()?;
    let _ = std::fs::create_dir_all(lock.parent()?);
    let unclean = lock.exists();
    let _ = std::fs::write(&lock, std::process::id().to_string());
    if !unclean {
        return None;
    }
    let json = std::fs::read_to_string(snapshot_path()?).ok()?;
    serde_json::from_str(&json).ok()
}

/// A normal quit: the watchdog won't relaunch and the next start won't restore.
pub fn end_session() {
    if let Some(lock) = lock_path() {
        let _ = std::fs::remove_file(lock);
    }
}

/// Writes the snapshot every couple of seconds when something changed (atomically, off the UI thread).
#[derive(Default)]
pub struct Saver {
    last: String,
    last_time: f64,
}

impl Saver {
    pub fn due(&self, now: f64) -> bool {
        now - self.last_time >= 2.0
    }

    pub fn save(&mut self, now: f64, snapshot: &Snapshot) {
        self.last_time = now;
        let Ok(json) = serde_json::to_string(snapshot) else { return };
        if json == self.last {
            return;
        }
        self.last = json.clone();
        let Some(path) = snapshot_path() else { return };
        std::thread::spawn(move || {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        });
    }
}

/// Starts the watchdog for this process (a second copy of the binary with `--watchdog <pid>`).
pub fn spawn_watchdog() {
    let Ok(exe) = std::env::current_exe() else { return };
    let _ = std::process::Command::new(exe)
        .args(["--watchdog", &std::process::id().to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// The watchdog: waits for `pid` to exit. If it exited without a clean quit, reopens Phosphor
/// (unless it has crashed 3 times in the last 2 minutes, to avoid a crash loop).
pub fn run_watchdog(pid: u32) {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(700));
        // SAFETY: signal 0 only checks whether the process exists.
        let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
        if alive {
            continue;
        }
        let Some(lock) = lock_path() else { return };
        if !lock.exists() {
            return;
        }
        let now = unix_now();
        let mut recent: Vec<u64> = crashes_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| s.lines().filter_map(|l| l.trim().parse().ok()).filter(|t| now.saturating_sub(*t) < 120).collect())
            .unwrap_or_default();
        recent.push(now);
        if let Some(p) = crashes_path() {
            let _ = std::fs::write(p, recent.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("\n"));
        }
        if recent.len() > 3 {
            return;
        }
        relaunch();
        return;
    }
}

fn relaunch() {
    let Ok(exe) = std::env::current_exe() else { return };
    let path = exe.to_string_lossy().to_string();
    match path.find(".app/Contents/MacOS/") {
        // Inside an app bundle: let macOS open it so it gets its Dock icon and permissions.
        Some(i) => {
            let _ = std::process::Command::new("open").arg(&path[..i + 4]).spawn();
        }
        None => {
            let _ = std::process::Command::new(exe).spawn();
        }
    }
}
