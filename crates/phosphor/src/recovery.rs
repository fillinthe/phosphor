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

fn lock_path() -> Option<PathBuf> {
    dir().map(|d| d.join("running.lock"))
}

fn heartbeat_path() -> Option<PathBuf> {
    dir().map(|d| d.join("heartbeat"))
}

/// Seconds without a heartbeat (while the watchdog itself was awake) before Phosphor counts as frozen.
const FROZEN_AFTER: f64 = 20.0;

/// Written once a second by the render loop (which keeps running even when the window is
/// hidden); the watchdog restarts Phosphor if it stops.
#[derive(Default)]
pub struct Heartbeat {
    last: f64,
    count: u64,
}

impl Heartbeat {
    pub fn beat(&mut self, now: f64) {
        if now - self.last < 1.0 {
            return;
        }
        self.last = now;
        self.count += 1;
        if let Some(p) = heartbeat_path() {
            let _ = std::fs::write(p, self.count.to_string());
        }
    }
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

/// The snapshot left by the last run, whatever way it ended (a normal quit writes one too): to reopen as it was.
pub fn last_snapshot() -> Option<Snapshot> {
    serde_json::from_str(&std::fs::read_to_string(snapshot_path()?).ok()?).ok()
}

/// A normal quit: the watchdog won't relaunch and the next start won't restore.
pub fn end_session() {
    if let Some(lock) = lock_path() {
        let _ = std::fs::remove_file(lock);
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
            let Ok(handle) = std::thread::Builder::new().name("recovery-snapshot".into()).spawn(move || {
                let mut last = String::new();
                while let Ok(snap) = rx.recv() {
                    let Ok(json) = serde_json::to_string(&snap) else { continue };
                    if json != last {
                        let _ = crate::project_files::write_whole(&path, &json);
                        last = json;
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
    let mut tick = std::time::Instant::now();
    let mut wall = std::time::SystemTime::now();
    let mut grace_until: Option<std::time::Instant> = None;
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
            let beat = heartbeat_path().and_then(|p| std::fs::read_to_string(p).ok());
            if beat.is_some() && beat != last_beat {
                last_beat = beat;
                stale = 0.0;
                noted_idle = false;
            } else if last_beat.is_some() && dt < 2.0 && !in_grace {
                stale += dt;
            }
            if stale < FROZEN_AFTER {
                continue;
            }
            // Frozen: write down where it's stuck (for the report), then end it so it can be reopened with the last
            // settings.
            let stuck = stuck_at(pid);
            // Not stuck: the main thread is waiting for events (nothing of Phosphor's own running), it just isn't
            // drawing frames. Ending it would only lose the show (reports 2026-11-22: five restarts in ten minutes of
            // an idle Phosphor). Noted once, left running.
            if stuck.as_deref().is_some_and(waiting_for_events) {
                if !noted_idle {
                    noted_idle = true;
                    log_line(&format!("[{}] no frames for {FROZEN_AFTER} s, but not stuck (waiting for events): left running\n", unix_now()));
                }
                stale = 0.0;
                continue;
            }
            let at = stuck.map_or(String::new(), |s| format!("\n  stuck at: {s}"));
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
#[cfg(target_os = "macos")]
fn stuck_at(pid: u32) -> Option<String> {
    let file = crash_log_path()?.with_file_name("freeze-sample.txt");
    crate::os::command("/usr/bin/sample")
        .args([&pid.to_string(), "1", "-mayDie", "-file"])
        .arg(&file)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    let text = std::fs::read_to_string(&file).ok()?;
    let path = main_thread_path(&text);
    (!path.is_empty()).then(|| path.join(" > "))
}

#[cfg(not(target_os = "macos"))]
fn stuck_at(_pid: u32) -> Option<String> {
    None
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
    ours <= 1 && (last.ends_with("mach_msg2_trap") || last.ends_with("mach_msg_trap"))
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
    /// The snapshot is written whole by the writer thread, only when it changed, and quitting waits for it.
    #[test]
    fn snapshots_are_written_off_the_ui_thread() {
        let dir = std::env::temp_dir().join(format!("phosphor-recovery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("recovery.json");
        let mut saver = super::Saver { to: Some(path.clone()), ..Default::default() };
        let snap = |selected| super::Snapshot { selected: Some(selected), ..Default::default() };
        saver.save(0.0, snap(1));
        assert!(!saver.due(1.0) && saver.due(2.0));
        // Let the writer take it, then hand over a newer one.
        for k in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            saver.save(2.0 + k as f64, snap(7));
            if std::fs::read_to_string(&path).is_ok_and(|t| t.contains("\"selected\":7")) {
                break;
            }
        }
        saver.finish();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"selected\":7"), "{text}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no part files left");
        let _ = std::fs::remove_dir_all(&dir);
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
    fn a_wake_from_sleep_is_noticed() {
        assert!(super::woke(3600.0, 0.7), "an hour on the wall clock, 0.7 s awake: slept");
        assert!(!super::woke(0.72, 0.7) && !super::woke(1.5, 0.7), "normal ticks and a little scheduling delay aren't");
    }

    #[test]
    fn an_idle_main_thread_is_not_a_freeze() {
        assert!(super::waiting_for_events("phosphor::main > in libsystem_kernel.dylib: mach_msg2_trap"));
        assert!(!super::waiting_for_events("phosphor::main > App::ui > in libsystem_kernel.dylib: __semwait_signal"));
        assert!(!super::waiting_for_events("phosphor::main > PhosphorApp::render > Engine::render > in libsystem_kernel.dylib: mach_msg2_trap"));
    }

    #[test]
    fn newer_rust_names_in_a_freeze_sample_are_decoded() {
        let report = "Call graph:\n    2600 Thread_1   DispatchQueue_1: com.apple.main-thread  (serial)\n    + 2600 start  (in dyld) + 6992  [0x1]\n    + ! 2600 _RNvNtCs1234_8phosphor3app6render  (in Phosphor) + 3  [0x3]\n    + !   2600 __psynch_cvwait  (in libsystem_kernel.dylib) + 8  [0x5]\n    1 Thread_2\n";
        assert_eq!(super::main_thread_path(report), ["app::render", "in libsystem_kernel.dylib: __psynch_cvwait"]);
    }
}
