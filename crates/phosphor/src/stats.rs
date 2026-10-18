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
}

impl Snapshot {
    pub fn to_json(self) -> String {
        json!({
            "fps": round(self.fps),
            "frame_ms": round(self.frame_ms),
            "render_ms": round(self.render_ms),
            "gpu_ms": round(self.gpu_ms),
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
    pub latest: Snapshot,
}

impl Stats {
    pub fn new(now: f64) -> Self {
        Self { window_start: now, frames: 0, render_total: 0.0, last_cpu: process_cpu_seconds(), latest: Snapshot::default() }
    }

    /// Records one frame; returns a fresh snapshot once per second.
    pub fn record(&mut self, now: f64, render_secs: f64, passes: usize, audio_level: f64, gpu_ms: f64) -> Option<Snapshot> {
        self.frames += 1;
        self.render_total += render_secs;
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
        };
        self.window_start = now;
        self.frames = 0;
        self.render_total = 0.0;
        self.last_cpu = cpu;
        Some(self.latest)
    }
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

#[cfg(not(any(target_os = "macos", windows)))]
fn memory_mb() -> Option<f64> {
    None
}
