//! GPU errors don't end the show: wgpu's default for an error nobody caught is to stop the app; here it's written to
//! the log (and said once on screen) and the frame carries on. A lost device (driver reset, eGPU unplugged) is
//! logged and said too: Phosphor can't draw again without restarting, and the watchdog + reopen-as-it-was cover that.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use phosphor_engine::wgpu;

#[derive(Clone, Default)]
pub struct GpuGuard {
    /// The first error since the last log line, and how many came in all.
    errors: Arc<Mutex<(Option<String>, u64)>>,
    /// When the log was last written (an error repeating every frame is one line a second).
    logged: Arc<Mutex<Option<std::time::Instant>>>,
    lost: Arc<AtomicBool>,
    /// Already said on screen (once per run is enough).
    told: Arc<AtomicBool>,
}

impl GpuGuard {
    pub fn install(device: &wgpu::Device) -> Self {
        let guard = GpuGuard::default();
        let errors = guard.errors.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            if let Ok(mut errs) = errors.lock() {
                if errs.0.is_none() {
                    errs.0 = Some(e.to_string());
                }
                errs.1 += 1;
            }
        }));
        let lost = guard.lost.clone();
        device.set_device_lost_callback(move |reason, message| {
            log::error!(target: "phosphor::gpu", "graphics device lost ({reason:?}): {message}");
            lost.store(true, Ordering::Relaxed);
        });
        guard
    }

    /// Errors since the last look, written to the log; the first one ever (or a lost device) as a notice.
    pub fn poll(&self) -> Option<String> {
        let lost = self.lost.swap(false, Ordering::Relaxed);
        let mut logged = self.logged.lock().ok()?;
        if logged.is_some_and(|t| t.elapsed().as_secs_f32() < 1.0) && !lost {
            return None;
        }
        let (first, count) = self.errors.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default();
        let first = first.map(|e| e.lines().next().unwrap_or_default().to_string());
        if let Some(e) = &first {
            *logged = Some(std::time::Instant::now());
            let more = if count > 1 { format!(" ({} like it in the last second)", count - 1) } else { String::new() };
            log::warn!(target: "phosphor::gpu", "skipped a GPU error: {e}{more}");
        }
        if lost {
            return Some("The graphics card reset. Save and restart Phosphor (it reopens where you were).".into());
        }
        if let Some(e) = first
            && !self.told.swap(true, Ordering::Relaxed)
        {
            return Some(format!("A GPU error was skipped (the show goes on; details in Help › Report a Bug): {e}"));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GPU mistake (a buffer that can't exist) is logged and said once, and doesn't stop the app.
    #[test]
    fn a_gpu_error_is_caught_not_fatal() {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else { return };
        let Ok((device, _queue)) = pollster::block_on(adapter.request_device(&Default::default())) else { return };
        let guard = GpuGuard::install(&device);
        for _ in 0..2 {
            // MAP_READ with STORAGE isn't allowed: a validation error (would end the app without the guard).
            let _ = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        }
        let said = guard.poll();
        assert!(said.is_some_and(|s| s.contains("GPU error")));
        let _ = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        assert_eq!(guard.poll(), None, "said once");
    }
}
