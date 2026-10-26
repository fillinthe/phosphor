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

/// What `targets` last found, an error if listing failed (no permission), a look running, and when it finished.
static TARGETS: Mutex<(Vec<Target>, Option<String>, bool, Option<std::time::Instant>)> = Mutex::new((Vec::new(), None, false, None));

pub const NO_PERMISSION: &str = "Screens and windows need permission: press Allow, or System Settings › Privacy & Security › Screen & System Audio Recording › turn on Phosphor, then reopen it";

/// The displays and windows found last time (empty until the first look finishes), and an error; starts a fresh
/// look in the background (one at a time).
pub fn targets() -> (Vec<Target>, Option<String>) {
    let mut t = TARGETS.lock().unwrap_or_else(|e| e.into_inner());
    // A fresh look every 3 seconds at most (windows come and go; asking every frame was wasted work).
    if !t.2 && t.3.is_none_or(|at| at.elapsed().as_secs_f32() > 3.0) {
        t.2 = true;
        std::thread::spawn(|| {
            let found = list();
            let mut t = TARGETS.lock().unwrap_or_else(|e| e.into_inner());
            let now = Some(std::time::Instant::now());
            match found {
                Ok(v) => *t = (v, None, false, now),
                Err(e) => *t = (Vec::new(), Some(e), false, now),
            }
        });
    }
    (t.0.clone(), t.1.clone())
}

fn list() -> Result<Vec<Target>, String> {
    // Without permission, asking ScreenCaptureKit makes macOS show its prompt again (every look): check quietly.
    if !crate::permissions::screen_allowed() {
        return Err(NO_PERMISSION.to_string());
    }
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

/// This Mac's sound, every app but Phosphor (ScreenCaptureKit audio): for the beat detector and the meters, with no
/// loopback driver. Needs the same Screen & System Audio Recording permission as screen capture.
pub struct SystemSound {
    stream: Arc<Mutex<Option<SCStream>>>,
    error: Arc<Mutex<Option<String>>>,
}

/// Sample rate the system sound arrives at.
pub const SYSTEM_SOUND_RATE: u32 = 48_000;

impl SystemSound {
    /// Starts on a thread; each block of sound (left, right) goes to `push`.
    pub fn start(push: impl Fn(&[[f32; 2]]) + Send + Sync + 'static) -> Self {
        let s = Self { stream: Arc::default(), error: Arc::default() };
        let (stream, error) = (s.stream.clone(), s.error.clone());
        std::thread::spawn(move || match open_sound(push) {
            Ok(st) => *stream.lock().unwrap_or_else(|e| e.into_inner()) = Some(st),
            Err(e) => *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e),
        });
        s
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().ok()?.clone()
    }
}

impl Drop for SystemSound {
    fn drop(&mut self) {
        if let Some(s) = self.stream.lock().ok().and_then(|mut s| s.take()) {
            std::thread::spawn(move || {
                let _ = s.stop_capture();
            });
        }
    }
}

/// Float sound buffers → (left, right) frames: one buffer per channel (planar), or one interleaved buffer.
fn frames_of(buffers: &[(u32, &[u8])]) -> Vec<[f32; 2]> {
    let floats = |b: &[u8]| -> Vec<f32> { b.chunks_exact(4).map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect() };
    match buffers {
        [(1, l), (1, r), ..] => floats(l).into_iter().zip(floats(r)).map(|(l, r)| [l, r]).collect(),
        [(ch, b), ..] if *ch >= 2 => floats(b).chunks_exact(*ch as usize).map(|f| [f[0], f[1]]).collect(),
        [(_, b), ..] => floats(b).into_iter().map(|v| [v, v]).collect(),
        [] => Vec::new(),
    }
}

fn open_sound(push: impl Fn(&[[f32; 2]]) + Send + Sync + 'static) -> Result<SCStream, String> {
    let content = SCShareableContent::get().map_err(|_| NO_PERMISSION.to_string())?;
    let display = content.displays().into_iter().next().ok_or("no display")?;
    let filter = SCContentFilter::create().with_display(&display).with_excluding_windows(&[]).build().map_err(|e| e.to_string())?;
    // The picture isn't used: the smallest, slowest one ScreenCaptureKit allows.
    let config = SCStreamConfiguration::new()
        .with_width(2)
        .with_height(2)
        .with_fps(1)
        .with_captures_audio(true)
        .with_excludes_current_process_audio(true)
        .with_sample_rate(SYSTEM_SOUND_RATE as i32)
        .with_channel_count(2);
    let mut stream = SCStream::new(&filter, &config).map_err(|e| e.to_string())?;
    stream
        .add_output_handler(
            move |sample: CMSampleBuffer, kind: SCStreamOutputType| {
                if !matches!(kind, SCStreamOutputType::Audio) {
                    return;
                }
                let Ok(list) = sample.audio_buffer_list() else { return };
                let buffers: Vec<(u32, &[u8])> = (0..list.num_buffers()).filter_map(|i| list.get(i)).map(|b| (b.number_channels, b.data())).collect();
                push(&frames_of(&buffers));
            },
            SCStreamOutputType::Audio,
        )
        .map_err(|e| e.to_string())?;
    stream.start_capture().map_err(|e| e.to_string())?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    #[test]
    fn system_sound_buffers_become_frames() {
        let b = |v: &[f32]| v.iter().flat_map(|x| x.to_ne_bytes()).collect::<Vec<u8>>();
        let (l, r, i) = (b(&[0.1, 0.2]), b(&[-0.1, -0.2]), b(&[0.5, -0.5, 0.25, -0.25]));
        assert_eq!(super::frames_of(&[(1, &l), (1, &r)]), vec![[0.1, -0.1], [0.2, -0.2]], "planar");
        assert_eq!(super::frames_of(&[(2, &i)]), vec![[0.5, -0.5], [0.25, -0.25]], "interleaved");
        assert_eq!(super::frames_of(&[(1, &l)]), vec![[0.1, 0.1], [0.2, 0.2]], "mono");
    }

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
