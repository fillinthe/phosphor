//! A display or another app's window as a layer source (macOS ScreenCaptureKit; VDMX's Window / Display Capture).
//! Frames arrive on ScreenCaptureKit's own queue as NV12 IOSurfaces and go on the layer without a copy
//! (`media::show_surface`). Nothing here runs on the UI thread: listing and starting happen on threads, and the
//! newest frame wins. Needs the Screen Recording permission (macOS asks the first time).

use std::sync::{Arc, Mutex};

use screencapturekit::prelude::*;

use crate::media::video::surface::PixelBuffer;

/// Something to capture: `url` = "display:<id>" or "window:<id>" (what `SourceSpec::Live` stores).
#[derive(Clone, Debug)]
pub struct Target {
    pub label: String,
    pub url: String,
}

/// What `targets` last found, and an error if listing failed (no permission).
static TARGETS: Mutex<(Vec<Target>, Option<String>, bool)> = Mutex::new((Vec::new(), None, false));

pub const NO_PERMISSION: &str = "Screen capture needs permission: System Settings › Privacy & Security › Screen & System Audio Recording › turn on Phosphor, then reopen it";

/// The displays and windows found last time (empty until the first look finishes), and an error; starts a fresh
/// look in the background (one at a time).
pub fn targets() -> (Vec<Target>, Option<String>) {
    let mut t = TARGETS.lock().unwrap_or_else(|e| e.into_inner());
    if !t.2 {
        t.2 = true;
        std::thread::spawn(|| {
            let found = list();
            let mut t = TARGETS.lock().unwrap_or_else(|e| e.into_inner());
            match found {
                Ok(v) => *t = (v, None, false),
                Err(e) => *t = (Vec::new(), Some(e), false),
            }
        });
    }
    (t.0.clone(), t.1.clone())
}

fn list() -> Result<Vec<Target>, String> {
    let content = SCShareableContent::get().map_err(|_| NO_PERMISSION.to_string())?;
    let mut out: Vec<Target> = content
        .displays()
        .iter()
        .enumerate()
        .map(|(i, d)| Target { label: format!("Display {} ({}×{})", i + 1, d.width(), d.height()), url: format!("display:{}", d.display_id()) })
        .collect();
    let mut windows: Vec<Target> = content
        .windows()
        .iter()
        .filter(|w| w.is_on_screen() && w.frame().size.width >= 64.0 && w.frame().size.height >= 64.0)
        .filter_map(|w| {
            let app = w.owning_application().map(|a| a.application_name()).unwrap_or_default();
            let title = w.title().unwrap_or_default();
            // Menu bar items, the Dock and other untitled helpers aren't useful sources.
            if app.is_empty() || title.is_empty() || app == "Window Server" || app == "Dock" {
                return None;
            }
            Some(Target { label: format!("{app}: {title}"), url: format!("window:{}", w.window_id()) })
        })
        .collect();
    windows.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    out.extend(windows);
    Ok(out)
}

/// A running capture: the newest frame, or why it couldn't start.
pub struct ScreenCapture {
    latest: Arc<Mutex<Option<PixelBuffer>>>,
    error: Arc<Mutex<Option<String>>>,
    stream: Arc<Mutex<Option<SCStream>>>,
}

impl ScreenCapture {
    /// Starts capturing `url` ("display:<id>" / "window:<id>") on a thread; frames follow as they come.
    pub fn start(url: &str) -> Self {
        let cap = Self { latest: Arc::default(), error: Arc::default(), stream: Arc::default() };
        let (latest, error, stream, url) = (cap.latest.clone(), cap.error.clone(), cap.stream.clone(), url.to_string());
        std::thread::spawn(move || match open(&url, latest) {
            Ok(s) => *stream.lock().unwrap_or_else(|e| e.into_inner()) = Some(s),
            Err(e) => *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e),
        });
        cap
    }

    /// The newest frame since the last call.
    pub fn take(&self) -> Option<PixelBuffer> {
        self.latest.lock().ok()?.take()
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().ok()?.clone()
    }
}

impl Drop for ScreenCapture {
    fn drop(&mut self) {
        if let Some(s) = self.stream.lock().ok().and_then(|mut s| s.take()) {
            // Stopping waits for ScreenCaptureKit; don't hold up the UI for it.
            std::thread::spawn(move || {
                let _ = s.stop_capture();
            });
        }
    }
}

fn open(url: &str, latest: Arc<Mutex<Option<PixelBuffer>>>) -> Result<SCStream, String> {
    let (kind, id) = url.split_once(':').ok_or("not a capture address")?;
    let id: u32 = id.parse().map_err(|_| "not a capture address")?;
    let content = SCShareableContent::get().map_err(|_| NO_PERMISSION.to_string())?;
    // Retina: twice the size in points, at most 4K wide.
    let (filter, w, h) = match kind {
        "display" => {
            let d = content.displays().into_iter().find(|d| d.display_id() == id).ok_or("that display is gone")?;
            let f = SCContentFilter::create().with_display(&d).with_excluding_windows(&[]).build().map_err(|e| e.to_string())?;
            (f, d.width() as f64, d.height() as f64)
        }
        _ => {
            let win = content.windows().into_iter().find(|w| w.window_id() == id).ok_or("that window is closed")?;
            let size = win.frame().size;
            let f = SCContentFilter::create().with_window(&win).build().map_err(|e| e.to_string())?;
            (f, size.width, size.height)
        }
    };
    let scale = (2.0f64).min(3840.0 / w.max(1.0));
    let config = SCStreamConfiguration::new()
        .with_width((w * scale).round() as u32 & !1)
        .with_height((h * scale).round() as u32 & !1)
        .with_pixel_format(PixelFormat::YCbCr_420v)
        .with_shows_cursor(true)
        .with_fps(60)
        .with_queue_depth(5);
    let mut stream = SCStream::new(&filter, &config).map_err(|e| e.to_string())?;
    stream
        .add_output_handler(
            move |sample: CMSampleBuffer, kind: SCStreamOutputType| {
                if !matches!(kind, SCStreamOutputType::Screen) {
                    return;
                }
                let Some(cv) = sample.pixel_buffer() else { return };
                // SAFETY: a valid CVPixelBuffer for this call; retain_nv12 takes its own reference.
                if let Some(pb) = unsafe { PixelBuffer::retain_nv12(cv.as_ptr()) }
                    && let Ok(mut l) = latest.lock()
                {
                    *l = Some(pb);
                }
            },
            SCStreamOutputType::Screen,
        )
        .map_err(|e| e.to_string())?;
    stream.start_capture().map_err(|e| e.to_string())?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    /// Needs the Screen Recording permission: `cargo test -p phosphor captures_a_window -- --ignored`.
    #[test]
    #[ignore]
    fn captures_a_window() {
        let found = super::list().expect("permission");
        let (displays, windows): (Vec<_>, Vec<_>) = found.iter().partition(|t| t.url.starts_with("display:"));
        println!("CAPTURE targets: {} displays, {} windows", displays.len(), windows.len());
        for t in [displays.first(), windows.first()].into_iter().flatten() {
            let cap = super::ScreenCapture::start(&t.url);
            let mut frames = 0;
            let mut size = (0, 0);
            for _ in 0..100 {
                std::thread::sleep(std::time::Duration::from_millis(20));
                if let Some(pb) = cap.take() {
                    frames += 1;
                    size = pb.size();
                }
            }
            assert!(cap.error().is_none(), "{:?}", cap.error());
            println!("CAPTURE {}: {frames} frames in 2 s, {}×{}", t.url.split(':').next().unwrap_or(""), size.0, size.1);
            assert!(frames > 0, "no frames from {}", t.url);
        }
    }
}
