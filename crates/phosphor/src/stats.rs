//! Frame-rate and resource counters shown in the UI and printed by `--bench`.

use serde_json::json;

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub fps: f64,
    pub frame_ms: f64,
    /// CPU time spent recording + submitting the engine frame.
    pub render_ms: f64,
    /// Whole-process CPU, as % of one core (native only).
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub passes: usize,
    pub audio_level: f64,
    /// GPU time per frame (0 when the adapter has no timestamps).
    pub gpu_ms: f64,
    /// The longest wait between two rendered frames (what a projector shows as a stutter).
    pub worst_ms: f64,
}

impl Snapshot {
    pub fn to_json(self) -> String {
        json!({
            "fps": round(self.fps),
            "frame_ms": round(self.frame_ms),
            "render_ms": round(self.render_ms),
            "gpu_ms": round(self.gpu_ms),
            "worst_ms": round(self.worst_ms),
            "cpu_percent": round(self.cpu_percent),
            "memory_mb": round(self.memory_mb),
            "passes": self.passes,
            "audio_level": round(self.audio_level),
        })
        .to_string()
    }
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

pub struct Stats {
    window_start: f64,
    frames: u32,
    render_total: f64,
    last_cpu: Option<f64>,
    /// Wall-clock time of the last frame, and the longest gap in this second.
    last_frame: Option<web_time::Instant>,
    /// When a long frame was last logged.
    last_hitch_log: f64,
    worst: f64,
    pub latest: Snapshot,
    /// Graphics memory the device has reserved, MB (None where the backend doesn't say: Metal, GL, the web).
    pub gpu_mb: Option<f64>,
    /// When graphics memory was last read, and the amount last written to the log.
    gpu_checked: f64,
    gpu_logged: f64,
}

impl Stats {
    pub fn new(now: f64) -> Self {
        Self { window_start: now, frames: 0, render_total: 0.0, last_cpu: process_cpu_seconds(), last_frame: None, last_hitch_log: f64::NEG_INFINITY, worst: 0.0, latest: Snapshot::default(), gpu_mb: None, gpu_checked: f64::NEG_INFINITY, gpu_logged: 0.0 }
    }

    /// Records one frame; returns a fresh snapshot once per second.
    pub fn record(&mut self, now: f64, render_secs: f64, passes: usize, audio_level: f64, gpu_ms: f64) -> Option<Snapshot> {
        self.record_at(web_time::Instant::now(), now, render_secs, passes, audio_level, gpu_ms)
    }

    /// Records one frame drawn at `t` (the render thread's frames are counted later, on the interface's side).
    pub fn record_at(&mut self, t: web_time::Instant, now: f64, render_secs: f64, passes: usize, audio_level: f64, gpu_ms: f64) -> Option<Snapshot> {
        self.frames += 1;
        self.render_total += render_secs;
        if let Some(last) = self.last_frame {
            let gap = t.duration_since(last).as_secs_f64();
            self.worst = self.worst.max(gap);
            // A frame that took over a tenth of a second: in the log, with what the frame was doing (a clip loading
            // from a slow drive, a heavy effect), at most one line a second.
            if gap > 0.1 && now - self.last_hitch_log > 1.0 {
                self.last_hitch_log = now;
                log::warn!(target: "phosphor::hitch", "a frame took {:.0} ms (render {:.1} ms, GPU {:.1} ms)", gap * 1000.0, render_secs * 1000.0, gpu_ms);
            }
        }
        self.last_frame = Some(t);
        let elapsed = now - self.window_start;
        if elapsed < 1.0 {
            return None;
        }
        let cpu = process_cpu_seconds();
        let cpu_percent = match (cpu, self.last_cpu) {
            (Some(now_cpu), Some(prev)) => (now_cpu - prev) / elapsed * 100.0,
            _ => 0.0,
        };
        let frames = self.frames.max(1) as f64;
        self.latest = Snapshot {
            fps: frames / elapsed,
            frame_ms: elapsed / frames * 1000.0,
            render_ms: self.render_total / frames * 1000.0,
            cpu_percent,
            memory_mb: memory_mb().unwrap_or(0.0),
            passes,
            audio_level,
            gpu_ms,
            worst_ms: self.worst * 1000.0,
        };
        self.worst = 0.0;
        self.window_start = now;
        self.frames = 0;
        self.render_total = 0.0;
        self.last_cpu = cpu;
        Some(self.latest)
    }

    /// Every 10 s, reads the graphics memory in use (`read` = reserved and allocated bytes) and logs it each time it
    /// has grown by a quarter since the last line: a run that ends "out of memory" then shows what it climbed from
    /// (report 2026-11-26: an Intel HD 620 on DX12 ran out three times, a minute or two after each start).
    pub fn watch_gpu_memory(&mut self, now: f64, read: impl FnOnce() -> Option<(u64, u64)>) {
        if now - self.gpu_checked < 10.0 {
            return;
        }
        self.gpu_checked = now;
        let Some((reserved, allocated)) = read() else { return };
        let mb = |b: u64| b as f64 / 1_048_576.0;
        self.gpu_mb = Some(mb(reserved));
        if let Some(line) = gpu_memory_line(mb(reserved), mb(allocated), self.gpu_logged) {
            self.gpu_logged = mb(reserved);
            log::info!(target: "phosphor::gpu", "{line}");
        }
    }
}

/// The log line for graphics memory now, if it's worth one: the first reading past 256 MB, then each 25 % more.
fn gpu_memory_line(reserved_mb: f64, allocated_mb: f64, logged_mb: f64) -> Option<String> {
    (reserved_mb >= 256.0 && reserved_mb > logged_mb * 1.25)
        .then(|| format!("graphics memory: {reserved_mb:.0} MB reserved, {allocated_mb:.0} MB in use"))
}

#[cfg(windows)]
fn process_cpu_seconds() -> Option<f64> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let z = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut create, mut exit, mut kernel, mut user) = (z, z, z, z);
    // SAFETY: our own process handle, FILETIMEs we own.
    if unsafe { GetProcessTimes(GetCurrentProcess(), &mut create, &mut exit, &mut kernel, &mut user) } == 0 {
        return None;
    }
    let secs = |t: FILETIME| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 / 1e7;
    Some(secs(kernel) + secs(user))
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
fn process_cpu_seconds() -> Option<f64> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    let secs = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    Some(secs(usage.ru_utime) + secs(usage.ru_stime))
}

#[cfg(target_arch = "wasm32")]
fn process_cpu_seconds() -> Option<f64> {
    None
}

/// Current resident memory.
#[cfg(target_os = "macos")]
fn memory_mb() -> Option<f64> {
    #[allow(deprecated)]
    unsafe {
        let mut info: libc::mach_task_basic_info = std::mem::zeroed();
        let mut count =
            (std::mem::size_of::<libc::mach_task_basic_info>() / std::mem::size_of::<libc::natural_t>()) as u32;
        let kr = libc::task_info(
            libc::mach_task_self(),
            libc::MACH_TASK_BASIC_INFO,
            &mut info as *mut _ as libc::task_info_t,
            &mut count,
        );
        (kr == libc::KERN_SUCCESS).then(|| info.resident_size as f64 / 1_048_576.0)
    }
}

#[cfg(windows)]
fn memory_mb() -> Option<f64> {
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: our own process handle and a counters struct we own, with its size.
    unsafe {
        let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        (GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) != 0).then(|| pmc.WorkingSetSize as f64 / 1_048_576.0)
    }
}

/// Linux: the resident memory from /proc (VmRSS, in kB).
#[cfg(target_os = "linux")]
fn memory_mb() -> Option<f64> {
    rss_from_status(&std::fs::read_to_string("/proc/self/status").ok()?)
}

#[cfg(any(target_os = "linux", test))]
fn rss_from_status(status: &str) -> Option<f64> {
    let kb: f64 = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?.trim().trim_end_matches("kB").trim().parse().ok()?;
    Some(kb / 1024.0)
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn memory_mb() -> Option<f64> {
    None
}

#[cfg(test)]
#[test]
fn graphics_memory_is_logged_as_it_grows() {
    assert_eq!(gpu_memory_line(100.0, 90.0, 0.0), None, "small amounts aren't logged");
    assert!(gpu_memory_line(300.0, 250.0, 0.0).unwrap().contains("300 MB reserved, 250 MB in use"));
    assert_eq!(gpu_memory_line(350.0, 300.0, 300.0), None, "under a quarter more");
    assert!(gpu_memory_line(400.0, 380.0, 300.0).is_some());
}

#[cfg(test)]
#[test]
fn linux_memory_is_read_from_proc() {
    assert_eq!(rss_from_status("Name:\tphosphor\nVmRSS:\t  204800 kB\nThreads:\t9\n"), Some(200.0));
    assert_eq!(rss_from_status("Name:\tphosphor\n"), None);
}
