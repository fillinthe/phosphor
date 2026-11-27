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
    /// (layer, seconds) for each playing video, so it resumes where it was.
    #[serde(default)]
    pub video_positions: Vec<(u64, f64)>,
}

fn dir() -> Option<PathBuf> {
    crate::show::storage_dir()
}

fn snapshot_path() -> Option<PathBuf> {
    dir().map(|d| d.join("recovery.json"))
}

// Each running copy has its own marker and heartbeat, named by its process id: two copies open at once (say an old one
// from a disk image next to the updated one) shared one heartbeat, so one copy's watchdog judged the other and restarted
// the old version over and over (reports 2026-11-22). Up to 0.5.17 these were "running.lock" and "heartbeat".
fn lock_path(pid: u32) -> Option<PathBuf> {
    dir().map(|d| d.join(format!("running-{pid}.lock")))
}

fn heartbeat_path(pid: u32) -> Option<PathBuf> {
    dir().map(|d| d.join(format!("heartbeat-{pid}")))
}

/// This run's marker, kept open and locked for as long as Phosphor runs: the system lets go of the lock the moment the
/// process ends, however it ends. (A process id alone can't say "still running": after a restart of the computer
/// another program gets the same number.)
static SESSION_LOCK: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);

/// Is this marker's copy of Phosphor still running (does it hold the lock)?
fn held(lock: &std::path::Path) -> bool {
    let Ok(f) = std::fs::File::open(lock) else { return false };
    match f.try_lock() {
        Ok(()) => {
            let _ = f.unlock();
            false
        }
        Err(std::fs::TryLockError::WouldBlock) => true,
        Err(std::fs::TryLockError::Error(_)) => false,
    }
}

/// Markers left by runs that are no longer running (crashed, or killed): nobody holds them. The old shared
/// "running.lock" counts too.
fn stale_locks(dir: &std::path::Path, held: impl Fn(&std::path::Path) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "running.lock" {
                return true;
            }
            name.starts_with("running-") && name.ends_with(".lock") && !held(p)
        })
        .collect()
}

/// Seconds without a heartbeat (while the watchdog itself was awake) before Phosphor counts as frozen.
const FROZEN_AFTER: f64 = 20.0;

/// Written once a second by the render loop (which keeps running even when the window is
/// hidden); the watchdog restarts Phosphor if it stops.
#[derive(Default)]
pub struct Heartbeat {
    last: f64,
    count: u64,
    /// The control window's state at the last frame (minimized, focused, occluded, fullscreen) and the outputs open:
    /// written with each beat, so when drawing stops the watchdog can say what the window was doing.
    pub window: [Option<bool>; 4],
    pub outputs: usize,
}

impl Heartbeat {
    pub fn beat(&mut self, now: f64) {
        if now - self.last < 1.0 {
            return;
        }
        self.last = now;
        self.count += 1;
        #[cfg(unix)]
        crate::crash_signals::frame();
        if let Some(p) = heartbeat_path(std::process::id()) {
            let _ = std::fs::write(p, format!("{} {}", self.count, self.state()));
        }
    }

    /// "window minimized=no focused=yes occluded=? fullscreen=no, 1 output open"
    fn state(&self) -> String {
        let f = |b: Option<bool>| match b {
            Some(true) => "yes",
            Some(false) => "no",
            None => "?",
        };
        let [m, foc, occ, full] = self.window;
        format!("window minimized={} focused={} occluded={} fullscreen={}, {} outputs open", f(m), f(foc), f(occ), f(full), self.outputs)
    }
}

/// What the last heartbeat said about the window ("" for an old-style heartbeat).
fn beat_state(beat: &str) -> &str {
    beat.split_once(' ').map_or("", |(_, rest)| rest)
}

fn crashes_path() -> Option<PathBuf> {
    dir().map(|d| d.join("crashes.txt"))
}

pub fn crash_log_path() -> Option<PathBuf> {
    dir().map(|d| d.join("crash.log"))
}

fn reported_path() -> Option<PathBuf> {
    dir().map(|d| d.join("crash-reported"))
}

/// How old a crash.log entry may be to go into a crash report on a computer that has no "reported up to" mark yet.
const FRESH_SECS: u64 = 7 * 24 * 3600;

/// The part of crash.log a crash report hasn't carried yet: everything after `reported` bytes (a log that's shorter
/// was started again: all of it); with no mark, the entries of the last week. (A run that ends without a panic or a
/// freeze logs nothing: its report used to carry the end of the log anyway, so a weeks-old crash read as today's;
/// report 2026-11-24.)
fn unreported(log: &str, reported: Option<usize>, now: u64) -> &str {
    match reported {
        Some(n) if n <= log.len() && log.is_char_boundary(n) => &log[n..],
        Some(_) => log,
        None => {
            let mut at = 0;
            for line in log.split_inclusive('\n') {
                let stamp = line.strip_prefix('[').and_then(|l| l.split_once(']')).and_then(|(t, _)| t.parse::<u64>().ok());
                if stamp.is_some_and(|t| t + FRESH_SECS >= now) {
                    return &log[at..];
                }
                at += line.len();
            }
            ""
        }
    }
}

/// What crash.log gained since the last crash report (empty = nothing new).
pub fn unreported_crash_log() -> String {
    let Some(log) = crash_log_path().and_then(|p| std::fs::read_to_string(p).ok()) else { return String::new() };
    let reported = reported_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| t.trim().parse().ok());
    unreported(&log, reported, unix_now()).to_string()
}

/// A crash report was made: the next one starts after what crash.log holds now.
pub fn mark_crash_log_reported() {
    if let (Some(mark), Some(log)) = (reported_path(), crash_log_path()) {
        let len = std::fs::metadata(log).map_or(0, |m| m.len());
        let _ = std::fs::write(mark, len.to_string());
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Marks this run as live. Returns the last snapshot if the previous run ended without a clean quit.
pub fn begin_session() -> Option<Snapshot> {
    let lock = lock_path(std::process::id())?;
    let _ = std::fs::create_dir_all(lock.parent()?);
    // Another copy that's still running isn't a crash; a marker whose run is gone is.
    let stale = stale_locks(lock.parent()?, held);
    let unclean = !stale.is_empty();
    for old in stale {
        let _ = std::fs::remove_file(old);
    }
    // Heartbeats of runs that are gone (their watchdog normally removes them), and the old shared one.
    if let Ok(entries) = std::fs::read_dir(lock.parent()?) {
        for p in entries.flatten().map(|e| e.path()) {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let gone = name == "heartbeat" || name.strip_prefix("heartbeat-").and_then(|n| n.parse::<u32>().ok()).is_some_and(|pid| !lock_path(pid).is_some_and(|l| held(&l)));
            if gone {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    if let Ok(f) = std::fs::OpenOptions::new().create(true).truncate(true).write(true).read(true).open(&lock) {
        let _ = f.try_lock();
        *SESSION_LOCK.lock().unwrap() = Some(f);
    }
    if !unclean {
        return None;
    }
    let json = std::fs::read_to_string(snapshot_path()?).ok()?;
    serde_json::from_str(&json).ok()
}

/// The project in the snapshot the last run left (whatever way it ended): the app's settings leave it out while the
/// snapshot holds it (`Saver::holds_project`).
pub fn last_project() -> Option<phosphor_engine::Composition> {
    project_in(&std::fs::read_to_string(snapshot_path()?).ok()?)
}

/// The project in a snapshot's text, read the way a project file is (`Composition::from_json`: older formats brought
/// up to date, groups tidied, the id counter past every id), as the settings' copy was.
fn project_in(snapshot: &str) -> Option<phosphor_engine::Composition> {
    let mut snap: serde_json::Value = serde_json::from_str(snapshot).ok()?;
    let comp = snap.get_mut("comp")?.take();
    if comp.is_null() {
        return None;
    }
    phosphor_engine::Composition::from_json(&comp.to_string()).ok()
}

/// The snapshot left by the last run, whatever way it ended (a normal quit writes one too): to reopen as it was.
pub fn last_snapshot() -> Option<Snapshot> {
    serde_json::from_str(&std::fs::read_to_string(snapshot_path()?).ok()?).ok()
}

/// A normal quit: the watchdog won't relaunch and the next start won't restore.
pub fn end_session() {
    let pid = std::process::id();
    *SESSION_LOCK.lock().unwrap() = None;
    for p in [lock_path(pid), heartbeat_path(pid)].into_iter().flatten() {
        let _ = std::fs::remove_file(p);
    }
}

/// Writes the snapshot every couple of seconds when something changed. The UI thread only hands over a copy; one
/// writer thread of its own turns it into text, compares it with the last and writes it whole (never two at once).
#[derive(Default)]
pub struct Saver {
    last_time: f64,
    worker: Option<(std::sync::mpsc::SyncSender<Snapshot>, std::thread::JoinHandle<()>)>,
    /// Where it writes (None = `snapshot_path`; tests give their own).
    to: Option<PathBuf>,
    /// The file on disk holds the project as of the last write, and that write went through (a failed one clears it).
    written: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Saver {
    pub fn due(&self, now: f64) -> bool {
        now - self.last_time >= 2.0
    }

    pub fn save(&mut self, now: f64, snapshot: Snapshot) {
        self.last_time = now;
        if self.worker.is_none() {
            let Some(path) = self.to.clone().or_else(snapshot_path) else { return };
            // (No room to queue: a copy is only taken while the writer waits, so the newest one is what gets written.)
            let (tx, rx) = std::sync::mpsc::sync_channel::<Snapshot>(0);
            let written = self.written.clone();
            let Ok(handle) = std::thread::Builder::new().name("recovery-snapshot".into()).spawn(move || {
                let mut last = String::new();
                while let Ok(snap) = rx.recv() {
                    let Ok(json) = serde_json::to_string(&snap) else {
                        written.store(false, std::sync::atomic::Ordering::Relaxed);
                        continue;
                    };
                    if json != last {
                        let ok = crate::project_files::write_whole(&path, &json).is_ok();
                        written.store(ok && snap.comp.is_some(), std::sync::atomic::Ordering::Relaxed);
                        // (A failed write is tried again with the next snapshot, even if nothing changed.)
                        last = if ok { json } else { String::new() };
                    }
                }
            }) else {
                return;
            };
            self.worker = Some((tx, handle));
        }
        // Still writing the last one: this one is skipped (the next comes in two seconds).
        if let Some((tx, _)) = &self.worker {
            let _ = tx.try_send(snapshot);
        }
    }

    /// The snapshot on disk holds the project, freshly (written within the last couple of seconds, the last write
    /// went through): the app's settings needn't carry a second copy (`save_settings`). False until the first write.
    pub fn holds_project(&self) -> bool {
        self.worker.is_some() && self.written.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Lets the writer finish what it has and stop (before the last snapshot is written on quit).
    pub fn finish(&mut self) {
        if let Some((tx, handle)) = self.worker.take() {
            drop(tx);
            let _ = handle.join();
        }
    }
}

/// Starts the watchdog for this process (a second copy of the binary with `--watchdog <pid>`).
pub fn spawn_watchdog() {
    let Ok(exe) = std::env::current_exe() else { return };
    let _ = crate::os::command(exe)
        .args(["--watchdog", &std::process::id().to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// The watchdog: waits for `pid` to exit, or to freeze (no heartbeat for `FROZEN_AFTER`
/// seconds, in which case it's killed). If it went without a clean quit, reopens Phosphor
/// (unless it has crashed 3 times in the last 2 minutes, to avoid a crash loop).
pub fn run_watchdog(pid: u32) {
    let mut last_beat: Option<String> = None;
    let mut stale = 0.0f64;
    let mut noted_idle = false;
    // The last look at a stalled Phosphor couldn't see where it was (no new beat since).
    let mut looked_blind = false;
    let mut tick = std::time::Instant::now();
    let mut wall = std::time::SystemTime::now();
    let mut grace_until: Option<std::time::Instant> = None;
    // When the last new heartbeat was seen: how long before the end the last frame was.
    let mut beat_seen: Option<std::time::Instant> = None;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(700));
        // Only count time we were awake ourselves: after the Mac sleeps, a stale heartbeat isn't a freeze.
        let dt = tick.elapsed().as_secs_f64();
        tick = std::time::Instant::now();
        // Just woke (the wall clock jumped while our own clock, which stops in sleep, didn't): the first frames after a
        // wake can take many seconds while memory comes back (reports 2026-11-22: 9 to 38 s frames overnight on an
        // Intel Mac, then restarted as "frozen"). A minute's grace before a stale heartbeat counts again.
        let wall_dt = wall.elapsed().map_or(dt, |d| d.as_secs_f64());
        wall = std::time::SystemTime::now();
        if woke(wall_dt, dt) {
            grace_until = Some(tick + std::time::Duration::from_secs(WAKE_GRACE));
            stale = 0.0;
        }
        let in_grace = grace_until.is_some_and(|g| tick < g);
        let alive = process_alive(pid);
        if alive {
            let beat = heartbeat_path(pid).and_then(|p| std::fs::read_to_string(p).ok());
            if beat.is_some() && beat != last_beat {
                last_beat = beat;
                beat_seen = Some(tick);
                stale = 0.0;
                noted_idle = false;
                looked_blind = false;
            } else if last_beat.is_some() && dt < 2.0 && !in_grace {
                stale += dt;
            }
            if stale < FROZEN_AFTER {
                continue;
            }
            // Frozen: write down where it's stuck (for the report), then end it so it can be reopened with the last
            // settings.
            let stuck = stuck_at(pid);
            // Looking takes a second: a beat that came meanwhile means it's drawing again.
            let beat_moved = heartbeat_path(pid).and_then(|p| std::fs::read_to_string(p).ok()).is_some_and(|b| Some(&b) != last_beat.as_ref());
            match verdict(beat_moved, &stuck, looked_blind) {
                Verdict::Alive => {
                    // The next turn takes the new beat (and clears the counters).
                    continue;
                }
                // Not stuck: the main thread is waiting for events (nothing of Phosphor's own running), it just isn't
                // drawing frames. Ending it would only lose the show (reports 2026-11-22: five restarts in ten minutes
                // of an idle Phosphor). Noted once, left running.
                Verdict::Idle => {
                    if !noted_idle {
                        noted_idle = true;
                        let last = last_beat.as_deref().map(beat_state).filter(|s| !s.is_empty()).map_or(String::new(), |s| format!(" (last frame: {s})"));
                        log_line(&format!("[{}] no frames for {FROZEN_AFTER} s, but not stuck (waiting for events): left running{last}\n", unix_now()));
                    }
                    stale = 0.0;
                    continue;
                }
                Verdict::LookAgain => {
                    let why = stuck.as_ref().err().map_or("", |e| e.as_str());
                    log_line(&format!("[{}] no frames for {FROZEN_AFTER} s and couldn't see where ({why}): looking again in {FROZEN_AFTER} s\n", unix_now()));
                    looked_blind = true;
                    stale = 0.0;
                    continue;
                }
                Verdict::Frozen => {}
            }
            let mut at = match &stuck {
                Ok(s) => format!("\n  stuck at: {s}"),
                Err(e) if !e.is_empty() => format!("\n  couldn't see where: {e}"),
                Err(_) => String::new(),
            };
            // What the window was doing at the last frame (minimized, focused…): a freeze report from Windows said
            // nothing else (2026-11-24). Kept out of the report's one-line summary.
            if let Some(s) = last_beat.as_deref().map(beat_state).filter(|s| !s.is_empty()) {
                at.push_str(&format!("\n  last frame: {s}"));
            }
            if let Some(log) = crash_log_path() {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log)
                    .and_then(|mut f| std::io::Write::write_all(&mut f, format!("[{}] froze (no heartbeat for {FROZEN_AFTER} s); restarted by the watchdog{at}\n", unix_now()).as_bytes()));
            }
            kill_process(pid);
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        // Gone: its heartbeat goes too (the marker stays when it didn't quit cleanly: the next start sees the crash).
        if let Some(p) = heartbeat_path(pid) {
            let _ = std::fs::remove_file(p);
        }
        let Some(lock) = lock_path(pid) else { return };
        if !lock.exists() {
            return;
        }
        let now = unix_now();
        // Gone with nothing written down (no panic, no freeze, no crash signal): say so, with how long before the end
        // it last drew a frame (reports 2026-11-25: runs that ended this way said nothing at all).
        let log = crash_log_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        if !logged_since(&log, now.saturating_sub(JUST_LOGGED)) {
            let state = last_beat.as_deref().map(beat_state).unwrap_or("");
            log_line(&ended_line(now, beat_seen.map(|t| t.elapsed().as_secs()), state));
        }
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

/// Is the watched process still running?
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // SAFETY: signal 0 only checks whether the process exists.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: a query-only handle to the process we watch, closed again.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        CloseHandle(h);
        ok && code == STILL_ACTIVE as u32
    }
}

/// Where a frozen Phosphor's main thread is (macOS: one second of `sample`): the source lines along its busiest
/// stack, outermost first.
/// Err = why it couldn't be seen (reports 2026-11-23: two Macs on 0.5.20 were restarted with no "stuck at", a
/// minute after the same look had found them only idle).
#[cfg(target_os = "macos")]
fn stuck_at(pid: u32) -> Result<String, String> {
    let file = crash_log_path().ok_or("no folder for the sample")?.with_file_name("freeze-sample.txt");
    let _ = std::fs::remove_file(&file);
    let out = crate::os::command("/usr/bin/sample")
        .args([&pid.to_string(), "1", "-mayDie", "-file"])
        .arg(&file)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("sample didn't run: {e}"))?;
    let said = || String::from_utf8_lossy(&out.stderr).split_whitespace().take(30).collect::<Vec<_>>().join(" ");
    let text = std::fs::read_to_string(&file).map_err(|_| format!("sample wrote nothing ({}; {})", out.status, said()))?;
    let path = main_thread_path(&text);
    if path.is_empty() {
        return Err(format!("no main thread in the sample ({} bytes; {})", text.len(), said()));
    }
    Ok(path.join(" > "))
}

/// What `stuck_at` says on Windows when Phosphor's window still answers messages: waiting for events, not frozen.
const ANSWERS_WINDOWS: &str = "phosphor::main > in user32.dll: waiting for messages (its window answers)";

/// Windows has no `sample`: Phosphor's window is asked something empty (WM_NULL), as Windows itself does before it
/// calls a window "Not responding". An answer within 5 s = the main thread is handling messages (a freeze report on
/// Windows 2026-11-24 couldn't tell). Err("") when it has no window to ask (as before: no second look).
#[cfg(windows)]
fn stuck_at(pid: u32) -> Result<String, String> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SMTO_ABORTIFHUNG, SMTO_BLOCK, SendMessageTimeoutW, WM_NULL,
    };
    struct Find {
        pid: u32,
        found: HWND,
    }
    unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> i32 {
        // SAFETY: `data` is the `Find` handed to EnumWindows below, alive for the whole call.
        let find = unsafe { &mut *(data as *mut Find) };
        let mut owner = 0u32;
        // SAFETY: plain queries on a window handle EnumWindows gave us.
        unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
        if owner == find.pid && unsafe { IsWindowVisible(hwnd) } != 0 {
            find.found = hwnd;
            return 0;
        }
        1
    }
    let mut find = Find { pid, found: std::ptr::null_mut() };
    // SAFETY: the callback only reads windows and writes `find`, which outlives the call.
    unsafe { EnumWindows(Some(each), &mut find as *mut Find as LPARAM) };
    if find.found.is_null() {
        return Err(String::new());
    }
    let mut result = 0usize;
    // SAFETY: an empty message with a timeout; nothing is read from or written to the other process.
    let answered = unsafe { SendMessageTimeoutW(find.found, WM_NULL, 0, 0, SMTO_ABORTIFHUNG | SMTO_BLOCK, 5000, &mut result) } != 0;
    Ok(if answered { ANSWERS_WINDOWS.to_string() } else { "phosphor::main > not answering window messages".to_string() })
}

/// Elsewhere there's no look: Err("") (nothing to say, and no second look).
#[cfg(not(any(target_os = "macos", windows)))]
fn stuck_at(_pid: u32) -> Result<String, String> {
    Err(String::new())
}

#[derive(Debug, PartialEq)]
enum Verdict {
    /// A beat came while it was being looked at.
    Alive,
    /// Only waiting for events: not frozen.
    Idle,
    /// Couldn't see where (the first time since the last beat): wait another stretch before ending it.
    LookAgain,
    Frozen,
}

/// What a stalled Phosphor is, from one look at it. `looked_blind` = the last look couldn't see either. An Err with no
/// words is "no way to look here" (not macOS): no second look.
fn verdict(beat_moved: bool, stuck: &Result<String, String>, looked_blind: bool) -> Verdict {
    match stuck {
        _ if beat_moved => Verdict::Alive,
        Ok(path) if waiting_for_events(path) => Verdict::Idle,
        Ok(_) => Verdict::Frozen,
        Err(why) if !why.is_empty() && !looked_blind => Verdict::LookAgain,
        Err(_) => Verdict::Frozen,
    }
}

/// From a `sample` report: the main thread's busiest stack (the first child at each level), outermost first, as
/// the Phosphor places on it: source lines ("frame.rs:289") when the build has them, else function names
/// ("PhosphorApp::render"; released builds carry no line tables). Ends with what the innermost frame waits in when
/// that's outside Phosphor ("in libsystem_kernel.dylib: __psynch_cvwait": a lock, the graphics card…).
fn main_thread_path(report: &str) -> Vec<String> {
    let mut lines = report.lines().skip_while(|l| !l.contains("com.apple.main-thread")).skip(1);
    let mut out: Vec<String> = Vec::new();
    let mut depth = 0;
    let mut innermost: Option<(String, String)> = None;
    for line in lines.by_ref() {
        let Some(d) = line.find(|c: char| c.is_ascii_digit()) else { break };
        if d <= depth {
            break;
        }
        depth = d;
        // "1287 name  (in image) + 3  [0x3]  frame.rs:289"
        let body = &line[d..];
        let rest = body.split_once(' ').map_or("", |(_, r)| r);
        let (symbol, after) = rest.split_once("  (in ").unwrap_or((rest, ""));
        // Newer Rust names ("_RNvNtCs…_8phosphor3app6render") come out of `sample` undecoded: decoded here.
        let decoded = rustc_demangle::try_demangle(symbol.trim()).map(|d| format!("{d:#}")).ok();
        let symbol = decoded.as_deref().unwrap_or(symbol);
        let image = after.split(')').next().unwrap_or("");
        innermost = Some((symbol.trim().to_string(), image.to_string()));
        let ours = symbol.contains("phosphor") || image.eq_ignore_ascii_case("phosphor");
        if !ours {
            continue;
        }
        let at = match line.split_whitespace().last().filter(|w| w.contains(".rs:")) {
            Some(at) => {
                // Rust's own plumbing (closures, boxes, unwinding, sorting) says nothing about where.
                const PLUMBING: [&str; 9] = ["rt.rs", "backtrace.rs", "function.rs", "panic.rs", "panicking.rs", "unwind_safe.rs", "boxed.rs", "slice.rs", "mod.rs:135"];
                if PLUMBING.iter().any(|p| at.starts_with(p)) {
                    continue;
                }
                at.to_string()
            }
            None => match short_symbol(symbol) {
                Some(name) => name,
                None => continue,
            },
        };
        if out.last() != Some(&at) {
            out.push(at);
        }
    }
    // The innermost dozen.
    let skip = out.len().saturating_sub(12);
    let mut out = out.split_off(skip);
    if let Some((symbol, image)) = innermost.filter(|(s, i)| !s.contains("phosphor") && !i.eq_ignore_ascii_case("phosphor") && !s.is_empty()) {
        out.push(format!("in {image}: {symbol}"));
    }
    out
}

/// A Rust function name as `sample` prints it without line tables, made short: "phosphor::app::render::_$LT$impl$u20$
/// phosphor..app..PhosphorApp$GT$::render::h0123456789abcdef" → "PhosphorApp::render". None for Rust's own
/// plumbing (std / core / alloc, closures' call shims).
fn short_symbol(symbol: &str) -> Option<String> {
    let s = symbol.trim();
    if !s.contains("phosphor") || s.starts_with("std::") || s.starts_with("core::") || s.starts_with("alloc::") {
        return None;
    }
    let s = s.replace("$LT$", "<").replace("$GT$", ">").replace("$u20$", " ").replace("$u7b$", "{").replace("$u7d$", "}").replace("..", "::");
    // Without the hash on the end.
    let s = match s.rsplit_once("::h") {
        Some((head, hash)) if hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()) => head.to_string(),
        _ => s,
    };
    let parts: Vec<&str> = s.split("::").filter(|p| !p.is_empty() && !p.starts_with("{{closure")).collect();
    let n = parts.len();
    if n == 0 {
        return None;
    }
    let tail = parts[n.saturating_sub(2)..].join("::");
    let tail: String = tail.replace("_<impl ", "").replace(['<', '>'], "");
    Some(tail.trim_start_matches('_').to_string())
}

/// Seconds after the computer wakes before a stale heartbeat counts toward a freeze.
const WAKE_GRACE: u64 = 60;

/// The computer slept between two looks: more wall-clock time passed than our own (sleep-stopping) clock saw.
fn woke(wall_dt: f64, awake_dt: f64) -> bool {
    wall_dt - awake_dt > 5.0
}

/// A `stuck_at` path that is only the main thread waiting for the next event ("phosphor::main > in
/// libsystem_kernel.dylib: mach_msg2_trap"): idle, not frozen.
fn waiting_for_events(path: &str) -> bool {
    let parts: Vec<&str> = path.split(" > ").collect();
    let ours = parts.iter().filter(|p| !p.starts_with("in ")).count();
    let last = parts.last().copied().unwrap_or("");
    path == ANSWERS_WINDOWS || (ours <= 1 && (last.ends_with("mach_msg2_trap") || last.ends_with("mach_msg_trap")))
}

/// Seconds back a crash.log entry counts as being about the end the watchdog has just seen.
const JUST_LOGGED: u64 = 30;

/// Does crash.log hold an entry stamped at `since` or later?
fn logged_since(log: &str, since: u64) -> bool {
    log.lines().any(|line| line.strip_prefix('[').and_then(|l| l.split_once(']')).and_then(|(t, _)| t.parse::<u64>().ok()).is_some_and(|t| t >= since))
}

/// What crash.log says for a run that went without quitting and without anything being logged. On Mac and Linux a
/// crash inside Phosphor logs its signal, so nothing logged means it was ended from outside; Windows can't tell yet.
fn ended_line(now: u64, last_frame_ago: Option<u64>, state: &str) -> String {
    let what = if cfg!(unix) {
        "ended from outside (no panic, freeze or crash signal): Force Quit, the system, or the power"
    } else {
        "ended without quitting (no panic or freeze logged)"
    };
    let mut line = format!("[{now}] {what}\n");
    let ago = last_frame_ago.map(|s| format!("{s} s before the end"));
    let last: Vec<&str> = ago.as_deref().into_iter().chain((!state.is_empty()).then_some(state)).collect();
    if !last.is_empty() {
        line.push_str(&format!("  last frame: {}\n", last.join("; ")));
    }
    line
}

/// Appends a line to crash.log.
fn log_line(line: &str) {
    if let Some(log) = crash_log_path() {
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(log).and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }
}

/// Ends the frozen process we watch.
#[cfg(unix)]
fn kill_process(pid: u32) {
    // SAFETY: killing the frozen Phosphor process we're watching.
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
}

#[cfg(windows)]
fn kill_process(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
    // SAFETY: a terminate handle to the frozen process we watch, closed again.
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !h.is_null() {
            TerminateProcess(h, 1);
            CloseHandle(h);
        }
    }
}

fn relaunch() {
    let Ok(exe) = std::env::current_exe() else { return };
    let path = exe.to_string_lossy().to_string();
    match path.find(".app/Contents/MacOS/") {
        // Inside an app bundle: let macOS open it so it gets its Dock icon and permissions.
        Some(i) => {
            crate::os::open(&path[..i + 4]);
        }
        None => {
            let _ = crate::os::command(exe).spawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::stale_locks;
    use super::{ended_line, logged_since};

    #[test]
    fn an_end_with_nothing_logged_gets_a_line_of_its_own() {
        // A panic, freeze or crash signal a moment ago is this end's cause: nothing to add.
        assert!(logged_since("[90] old\n[1000] crashed: SIGSEGV (touched memory it doesn't have)\n  at: a < b\n", 970));
        // Only old entries (or none): this end said nothing.
        assert!(!logged_since("[90] froze (no heartbeat for 20 s); restarted by the watchdog\n", 970));
        assert!(!logged_since("", 970));
        let line = ended_line(1000, Some(1), "window minimized=no focused=yes occluded=no fullscreen=no, 1 outputs open");
        assert!(line.starts_with("[1000] ended "), "{line}");
        assert!(line.ends_with("  last frame: 1 s before the end; window minimized=no focused=yes occluded=no fullscreen=no, 1 outputs open\n"), "{line}");
        assert_eq!(ended_line(5, None, "").lines().count(), 1);
        // The line it writes is found by the next look.
        assert!(logged_since(&line, 1000));
    }

    #[test]
    fn a_crash_report_carries_only_what_the_log_gained() {
        use super::{FRESH_SECS, unreported};
        let old = "[1000] panicked at a.rs:1:2:\nold crash\n";
        let new = "[900000] froze (no heartbeat for 20 s); restarted by the watchdog\n  stuck at: x\n";
        let log = format!("{old}{new}");
        // After a report, only what came later; nothing later = nothing.
        assert_eq!(unreported(&log, Some(old.len()), 900_010), new);
        assert_eq!(unreported(&log, Some(log.len()), 900_010), "");
        // A log that was started again (shorter than the mark): all of it.
        assert_eq!(unreported(new, Some(log.len()), 900_010), new);
        // No mark yet: the last week's entries with their following lines, and none of the old ones.
        assert_eq!(unreported(&log, None, 900_010), new);
        assert_eq!(unreported(&log, None, 900_000 + FRESH_SECS + 1), "");
        assert_eq!(unreported(old, None, 900_010), "");
    }
    use std::path::PathBuf;

    /// The snapshot is written whole by the writer thread, only when it changed, and quitting waits for it.
    #[test]
    fn snapshots_are_written_off_the_ui_thread() {
        let dir = std::env::temp_dir().join(format!("phosphor-recovery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("recovery.json");
        let mut saver = super::Saver { to: Some(path.clone()), ..Default::default() };
        let snap = |selected| super::Snapshot { selected: Some(selected), comp: Some(phosphor_engine::Composition::default()), ..Default::default() };
        assert!(!saver.holds_project(), "nothing written yet");
        saver.save(0.0, snap(1));
        assert!(!saver.due(1.0) && saver.due(2.0));
        // Let the writer take it, then hand over a newer one.
        for k in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            saver.save(2.0 + k as f64, snap(7));
            if saver.holds_project() && std::fs::read_to_string(&path).is_ok_and(|t| t.contains("\"selected\":7")) {
                break;
            }
        }
        assert!(saver.holds_project(), "written, with the project in it");
        saver.finish();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"selected\":7"), "{text}");
        assert!(serde_json::from_str::<super::Snapshot>(&text).unwrap().comp.is_some());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no part files left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The project comes back from a snapshot exactly as from the settings' copy it replaces.
    #[test]
    fn the_project_reads_back_from_a_snapshot() {
        let mut comp = phosphor_engine::Composition::opening();
        comp.notes = "show notes".into();
        let snap = super::Snapshot { comp: Some(comp.clone()), selected: Some(3), ..Default::default() };
        let text = serde_json::to_string(&snap).unwrap();
        assert_eq!(super::project_in(&text), phosphor_engine::Composition::from_json(&comp.to_json()).ok());
        assert_eq!(super::project_in(&text).map(|c| c.notes), Some("show notes".to_string()));
        let empty = serde_json::to_string(&super::Snapshot::default()).unwrap();
        assert!(super::project_in(&empty).is_none(), "no project in it");
        assert!(super::project_in("{cut off").is_none());
    }

    #[test]
    fn a_freeze_sample_reads_as_source_lines() {
        let report = "Call graph:\n    1290 Thread_1   DispatchQueue_1: com.apple.main-thread  (serial)\n    + 1290 start  (in dyld) + 6992  [0x1]\n    + ! 1290 main  (in phosphor) + 36  [0x2]  main.rs:214\n    + !   1290 f  (in phosphor) + 3  [0x3]  function.rs:250\n    + !     1287 run  (in phosphor) + 3  [0x3]  frame.rs:289\n    + !     : 900 media  (in phosphor) + 60  [0x4]  media.rs:202\n    + !     : 387 other  (in phosphor) + 60  [0x5]  layers.rs:9\n    + !     3 idle  (in AppKit) + 1\n    1 Thread_2\n";
        assert_eq!(super::main_thread_path(report), ["main.rs:214", "frame.rs:289", "media.rs:202"]);
    }

    #[test]
    fn a_freeze_sample_from_a_released_build_reads_as_function_names() {
        // Released builds have no line tables: the names, and the system call it's waiting in.
        let report = "Call graph:\n    2600 Thread_1   DispatchQueue_1: com.apple.main-thread  (serial)\n    + 2600 start  (in dyld) + 6992  [0x1]\n    + ! 2600 main  (in Phosphor) + 36  [0x2]\n    + !   2600 phosphor::app::render::_$LT$impl$u20$phosphor..app..PhosphorApp$GT$::render::h0123456789abcdef  (in Phosphor) + 3  [0x3]\n    + !     2600 core::ops::function::FnOnce::call_once::h0123456789abcdef  (in Phosphor) + 3  [0x3]\n    + !       2600 phosphor_engine::engine::encode::_$LT$impl$u20$phosphor_engine..engine..Engine$GT$::read_target::hfedcba9876543210  (in Phosphor) + 60  [0x4]\n    + !         2600 __psynch_cvwait  (in libsystem_kernel.dylib) + 8  [0x5]\n    1 Thread_2\n";
        assert_eq!(super::main_thread_path(report), ["PhosphorApp::render", "Engine::read_target", "in libsystem_kernel.dylib: __psynch_cvwait"]);
    }

    #[test]
    fn the_heartbeat_carries_the_window_state() {
        let hb = super::Heartbeat { window: [Some(true), Some(false), None, Some(false)], outputs: 2, ..Default::default() };
        let line = format!("7 {}", hb.state());
        assert_eq!(super::beat_state(&line), "window minimized=yes focused=no occluded=? fullscreen=no, 2 outputs open");
        assert_eq!(super::beat_state("7"), "", "an old heartbeat says nothing more");
    }

    #[test]
    fn only_markers_of_runs_that_are_gone_count_as_crashes() {
        let dir = std::env::temp_dir().join(format!("phosphor-locks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["running-100.lock", "running-200.lock", "heartbeat-100", "other.lock"] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        // 100 is still running (another copy open at the same time: it holds its marker's lock); 200 crashed.
        let running = std::fs::File::open(dir.join("running-100.lock")).unwrap();
        running.try_lock().unwrap();
        let names = |v: Vec<PathBuf>| v.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect::<Vec<_>>();
        assert_eq!(names(stale_locks(&dir, super::held)), ["running-200.lock"]);
        // The shared marker of 0.5.17 and before is an unclean end too.
        std::fs::write(dir.join("running.lock"), "1").unwrap();
        let mut stale = names(stale_locks(&dir, super::held));
        stale.sort();
        assert_eq!(stale, ["running-200.lock", "running.lock"]);
        // Once that copy is gone (however it ended), its marker counts.
        drop(running);
        assert_eq!(stale_locks(&dir, super::held).len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wake_from_sleep_is_noticed() {
        assert!(super::woke(3600.0, 0.7), "an hour on the wall clock, 0.7 s awake: slept");
        assert!(!super::woke(0.72, 0.7) && !super::woke(1.5, 0.7), "normal ticks and a little scheduling delay aren't");
    }

    #[test]
    fn a_stall_is_only_ended_when_it_is_seen_stuck_or_looked_at_twice() {
        use super::{Verdict, verdict};
        let stuck = Ok("phosphor::main > App::ui > in libsystem_kernel.dylib: __semwait_signal".to_string());
        let idle = Ok("phosphor::main > in libsystem_kernel.dylib: mach_msg2_trap".to_string());
        let blind = Err("sample wrote nothing".to_string());
        assert_eq!(verdict(true, &stuck, false), Verdict::Alive, "drew again while being looked at (M5, 0.5.20)");
        assert_eq!(verdict(false, &idle, false), Verdict::Idle);
        assert_eq!(verdict(false, &stuck, false), Verdict::Frozen);
        assert_eq!(verdict(false, &blind, false), Verdict::LookAgain, "couldn't see: one more stretch first");
        assert_eq!(verdict(false, &blind, true), Verdict::Frozen, "still couldn't see: end it");
        assert_eq!(verdict(false, &Err(String::new()), false), Verdict::Frozen, "no way to look (Windows): as before");
    }

    #[test]
    fn an_idle_main_thread_is_not_a_freeze() {
        assert!(super::waiting_for_events("phosphor::main > in libsystem_kernel.dylib: mach_msg2_trap"));
        assert!(!super::waiting_for_events("phosphor::main > App::ui > in libsystem_kernel.dylib: __semwait_signal"));
        assert!(!super::waiting_for_events("phosphor::main > PhosphorApp::render > Engine::render > in libsystem_kernel.dylib: mach_msg2_trap"));
        assert!(super::waiting_for_events(super::ANSWERS_WINDOWS), "Windows: its window still answers");
        assert!(!super::waiting_for_events("phosphor::main > not answering window messages"));
    }

    #[test]
    fn newer_rust_names_in_a_freeze_sample_are_decoded() {
        let report = "Call graph:\n    2600 Thread_1   DispatchQueue_1: com.apple.main-thread  (serial)\n    + 2600 start  (in dyld) + 6992  [0x1]\n    + ! 2600 _RNvNtCs1234_8phosphor3app6render  (in Phosphor) + 3  [0x3]\n    + !   2600 __psynch_cvwait  (in libsystem_kernel.dylib) + 8  [0x5]\n    1 Thread_2\n";
        assert_eq!(super::main_thread_path(report), ["app::render", "in libsystem_kernel.dylib: __psynch_cvwait"]);
    }
}
