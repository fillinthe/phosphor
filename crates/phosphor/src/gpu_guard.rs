//! GPU errors don't end the show: wgpu's default for an error nobody caught is to stop the app; here it's written to
//! the log (and said once on screen) and the frame carries on. A lost device (driver reset, eGPU unplugged) is
//! logged and said too: Phosphor can't draw again without restarting, and the watchdog + reopen-as-it-was cover that.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct GpuGuard {
    errors: Arc<Mutex<Vec<String>>>,
    lost: Arc<AtomicBool>,
    /// Already said on screen (once per run is enough).
    told: Arc<AtomicBool>,
}

/// How many errors are kept until the next frame looks (a broken shader can make one per frame).
const KEEP: usize = 8;

impl GpuGuard {
    pub fn install(device: &wgpu::Device) -> Self {
        let guard = GpuGuard::default();
        let errors = guard.errors.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            if let Ok(mut list) = errors.lock()
                && list.len() < KEEP
            {
                list.push(e.to_string());
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
        let errors: Vec<String> = self.errors.lock().map(|mut l| std::mem::take(&mut *l)).unwrap_or_default();
        for e in &errors {
            log::warn!(target: "phosphor::gpu", "skipped a GPU error: {}", e.lines().next().unwrap_or(e));
        }
        if self.lost.swap(false, Ordering::Relaxed) {
            return Some("The graphics card reset. Save and restart Phosphor (it reopens where you were).".into());
        }
        if !errors.is_empty() && !self.told.swap(true, Ordering::Relaxed) {
            return Some(format!("A GPU error was skipped (the show goes on; details in Help › Report a Bug): {}", errors[0].lines().next().unwrap_or_default()));
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
