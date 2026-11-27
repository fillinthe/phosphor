//! Live inputs as layer sources: cameras and capture cards (through FFmpeg: AVFoundation on the Mac, DirectShow
//! on Windows, Video4Linux on Linux),
//! and network streams (RTSP, SRT, HTTP/HLS, RTMP…). Decoded by the same player as movies, in
//! live mode (newest frame always wins, so there's no build-up of delay).

use phosphor_engine::SourceSpec;

use super::video::VideoPlayer;

/// A camera or capture device on this computer.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub name: String,
}

/// Cameras and capture cards (built-in, USB, Continuity Camera, HDMI capture…). Listing doesn't
/// need camera permission; macOS asks the first time one is opened.
#[cfg(target_os = "macos")]
pub fn cameras() -> Vec<Camera> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSArray, NSString};

    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {}

    let Some(class) = AnyClass::get(c"AVCaptureDevice") else { return Vec::new() };
    let mut out: Vec<Camera> = Vec::new();
    // "vide" = video devices, "muxa" = muxed audio+video (some capture cards, DV).
    for media in ["vide", "muxa"] {
        let media = NSString::from_str(media);
        // SAFETY: +[AVCaptureDevice devicesWithMediaType:] returns an NSArray of AVCaptureDevice.
        let devices: Option<Retained<NSArray<AnyObject>>> = unsafe { msg_send![class, devicesWithMediaType: &*media] };
        for d in devices.iter().flat_map(|a| a.iter()) {
            // SAFETY: localizedName is an NSString property of AVCaptureDevice.
            let name: Option<Retained<NSString>> = unsafe { msg_send![&*d, localizedName] };
            if let Some(name) = name.map(|n| n.to_string())
                && !out.iter().any(|c| c.name == name)
            {
                out.push(Camera { name });
            }
        }
    }
    out
}

/// Windows: the DirectShow video devices (cameras, capture cards, virtual cameras), as the bundled FFmpeg lists them.
/// Windows: cameras being opened right now. Some webcams refuse to start ("Input/output error") while the device list
/// is being read, so the list isn't read again then (the last one is used) and an opening that fails that way tries
/// again.
#[cfg(windows)]
static OPENING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(windows)]
static LAST_LIST: std::sync::Mutex<Vec<Camera>> = std::sync::Mutex::new(Vec::new());

#[cfg(windows)]
pub fn cameras() -> Vec<Camera> {
    if OPENING.load(std::sync::atomic::Ordering::SeqCst) > 0 {
        return LAST_LIST.lock().map(|l| l.clone()).unwrap_or_default();
    }
    let found = list_dshow();
    if let Ok(mut l) = LAST_LIST.lock() {
        *l = found.clone();
    }
    found
}

/// The camera list for a caller that can wait (a background thread: Report a Bug): on Windows, after any camera that's
/// starting has started (up to `secs`), so the list is read fresh instead of skipped.
#[cfg(not(target_arch = "wasm32"))]
pub fn cameras_when_settled(secs: f64) -> Vec<Camera> {
    #[cfg(windows)]
    {
        let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(secs);
        while OPENING.load(std::sync::atomic::Ordering::SeqCst) > 0 && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    #[cfg(not(windows))]
    let _ = secs;
    cameras()
}

#[cfg(windows)]
fn list_dshow() -> Vec<Camera> {
    let Some(ffmpeg) = crate::stream::ffmpeg_path() else { return Vec::new() };
    let Ok(out) = crate::os::command(ffmpeg).args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"]).stdin(std::process::Stdio::null()).output() else {
        return Vec::new();
    };
    dshow_devices(&String::from_utf8_lossy(&out.stderr)).into_iter().map(|name| Camera { name }).collect()
}

/// The video devices in FFmpeg's `-list_devices` answer for DirectShow: lines like `[in#0 @ 0x…] "Cam Link 4K"
/// (video)` (older FFmpeg: a "DirectShow video devices" heading, then the names). Two of the same card (each with
/// its own "Alternative name") are both listed, the second as "Cam Link 4K #2" (`repeat_of` opens it).
#[cfg(any(windows, test))]
fn dshow_devices(text: &str) -> Vec<String> {
    // (name, alternative name) of each video device, a device listed twice (same alternative name) once.
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    let mut in_video = false;
    let mut last_was_video = false;
    for line in text.lines() {
        if line.contains("DirectShow video devices") {
            in_video = true;
            continue;
        }
        if line.contains("DirectShow audio devices") {
            in_video = false;
            continue;
        }
        let Some(start) = line.find('"') else { continue };
        let Some(len) = line[start + 1..].find('"') else { continue };
        let name = &line[start + 1..start + 1 + len];
        if line.contains("Alternative name") {
            if last_was_video && let Some(last) = found.last_mut() {
                last.1 = Some(name.to_string());
                if found[..found.len() - 1].iter().any(|(n, a)| *n == found[found.len() - 1].0 && a == &found[found.len() - 1].1) {
                    found.pop();
                }
            }
            continue;
        }
        let tail = &line[start + 2 + len..];
        // "(none)": FFmpeg couldn't tell its kind without opening it (OBS Virtual Camera, some capture cards): audio
        // devices always say "(audio)", so it's taken as video.
        let video = if tail.contains('(') { tail.contains("video") || tail.contains("(none)") } else { in_video };
        last_was_video = video && !name.is_empty();
        if last_was_video {
            found.push((name.to_string(), None));
        }
    }
    let mut out: Vec<String> = Vec::new();
    for (i, (name, alt)) in found.iter().enumerate() {
        // A device with no alternative name listed twice is one device.
        if alt.is_none() && found[..i].iter().any(|(n, a)| n == name && a.is_none()) {
            continue;
        }
        let same = out.iter().filter(|n| repeat_of(n).0 == name).count();
        out.push(if same == 0 { name.clone() } else { format!("{name} #{}", same + 1) });
    }
    out
}

/// "Cam Link 4K #2" → ("Cam Link 4K", 1): the device name and which of the same-named ones it is (DirectShow's
/// `video_device_number`, from 0). A plain name → (name, 0).
#[cfg(any(windows, test))]
fn repeat_of(listed: &str) -> (&str, u32) {
    match listed.rsplit_once(" #") {
        Some((name, n)) if !name.is_empty() => match n.parse::<u32>() {
            Ok(n) if n >= 2 => (name, n - 1),
            _ => (listed, 0),
        },
        _ => (listed, 0),
    }
}

/// A camera that answered "busy" when asked to start ("Input/output error", "I/O error").
#[cfg(any(windows, test))]
fn busy(error: &str) -> bool {
    let e = error.to_lowercase();
    e.contains("input/output error") || e.contains("i/o error")
}

/// Linux: the Video4Linux capture devices (/dev/video*, each device's first node), by the name the driver gives.
#[cfg(target_os = "linux")]
pub fn cameras() -> Vec<Camera> {
    v4l2_devices().into_iter().map(|(name, _)| Camera { name }).collect()
}

/// (name, /dev path) of each Video4Linux capture device, in number order.
#[cfg(target_os = "linux")]
fn v4l2_devices() -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir("/sys/class/video4linux") else { return Vec::new() };
    let mut found: Vec<(u32, String, String)> = entries
        .flatten()
        .filter_map(|e| {
            let node = e.file_name().to_string_lossy().into_owned();
            let n: u32 = node.strip_prefix("video")?.parse().ok()?;
            // A device has several nodes (pictures, metadata): its first (index 0) is the picture.
            let index = std::fs::read_to_string(e.path().join("index")).map_or(0, |i| i.trim().parse().unwrap_or(0));
            (index == 0).then_some(())?;
            let name = std::fs::read_to_string(e.path().join("name")).map_or_else(|_| node.clone(), |n| n.trim().to_string());
            Some((n, name, format!("/dev/{node}")))
        })
        .collect();
    found.sort_by_key(|(n, ..)| *n);
    let mut out: Vec<(String, String)> = Vec::new();
    for (_, name, dev) in found {
        // Two of the same card: told apart by their device.
        let name = if out.iter().any(|(n, _)| *n == name) { format!("{name} ({dev})") } else { name };
        out.push((name, dev));
    }
    out
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
pub fn cameras() -> Vec<Camera> {
    Vec::new()
}

/// The source for a camera (the saved project keeps "avfoundation" as the kind on every system: a project made on a
/// Mac opens the same-named camera on Windows or Linux; `open` picks this system's way).
pub fn camera_source(cam: &Camera) -> SourceSpec {
    SourceSpec::Live { name: cam.name.clone(), format: "avfoundation".into(), url: cam.name.clone() }
}

/// The source for a stream address (rtsp://, srt://, http(s)://…m3u8, rtmp://, udp://).
pub fn stream_source(url: &str) -> SourceSpec {
    let name = url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(url).to_string();
    SourceSpec::Live { name, format: String::new(), url: url.trim().to_string() }
}

/// Live sources that are a web page always showing a live stream (`FEEDS`, or any channel's /live page): the
/// page is kept, and its current stream address is asked for each time it starts (`open_page`).
pub const PAGE: &str = "page";

/// Live pictures from the internet that are on all the time: (name, page, what it is).
pub const FEEDS: [(&str, &str, &str); 4] = [
    ("Earth from the ISS (NASA)", "https://www.youtube.com/@NASA/live", "NASA's live view from the International Space Station (dark half the time: the station's night)"),
    ("Earth in 4K from the ISS (Sen)", "https://www.youtube.com/@Sen/live", "Sen's 4K cameras on the International Space Station, around the clock"),
    ("Rocket pads, Florida (NASASpaceflight)", "https://www.youtube.com/@NASASpaceflight/live", "Cape Canaveral and Starbase pads, launches when they happen"),
    ("Volcano watch (afarTV)", "https://www.youtube.com/@afarTV/live", "Live cameras on an active volcano"),
];

pub fn page_source(name: &str, page: &str) -> SourceSpec {
    SourceSpec::Live { name: name.to_string(), format: PAGE.into(), url: page.trim().to_string() }
}

/// Opens a live page: its stream address now (yt-dlp), then the stream. Slow (seconds): off the UI thread only.
pub fn open_page(name: &str, page: &str) -> Result<VideoPlayer, String> {
    let url = crate::web_media::live_url(page).map_err(|e| format!("{name}: {e}"))?;
    open(name, "", &url)
}

/// The source for another app's Syphon output (`label` from `syphon::ServerInfo::label`).
pub fn syphon_source(label: &str) -> SourceSpec {
    SourceSpec::Live { name: label.to_string(), format: "syphon".into(), url: label.to_string() }
}

/// The source for a display or window capture (`url` from `screen_capture::Target`).
pub fn screen_source(label: &str, url: &str) -> SourceSpec {
    SourceSpec::Live { name: label.to_string(), format: "screen".into(), url: url.to_string() }
}

/// The source for an NDI sender on the network (name from `ndi::sources`).
pub fn ndi_source(name: &str) -> SourceSpec {
    SourceSpec::Live { name: name.to_string(), format: "ndi".into(), url: name.to_string() }
}

/// Starts decoding a live source.
/// What kind of live input (for grouping and icons).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Camera,
    Syphon,
    Screen,
    Window,
    Ndi,
}

impl InputKind {
    pub const ALL: [InputKind; 5] = [InputKind::Camera, InputKind::Screen, InputKind::Window, InputKind::Syphon, InputKind::Ndi];

    pub fn title(self) -> &'static str {
        match self {
            InputKind::Camera => "Cameras & capture cards",
            InputKind::Syphon => "Syphon (other apps)",
            InputKind::Screen => "Screens",
            InputKind::Window => "Windows",
            InputKind::Ndi => "NDI (network)",
        }
    }
}

/// One live input that can go on a layer or a clip.
#[derive(Clone, Debug)]
pub struct LiveInput {
    pub kind: InputKind,
    pub label: String,
    pub source: SourceSpec,
}

/// Every live input there is right now: `cameras` (asked for separately: listing them is slow), Syphon servers,
/// screens and windows, NDI sources. Plus a note per kind when there's something to say (none found, needs
/// permission…). Cheap enough to call while a menu or the Inputs panel is showing.
pub fn inputs(cameras: &[String]) -> (Vec<LiveInput>, Vec<(InputKind, String)>) {
    let mut out: Vec<LiveInput> = cameras.iter().map(|n| LiveInput { kind: InputKind::Camera, label: n.clone(), source: camera_source(&Camera { name: n.clone() }) }).collect();
    let mut notes = Vec::new();
    if cameras.is_empty() {
        notes.push((InputKind::Camera, "None found.".to_string()));
    }
    #[cfg(target_os = "macos")]
    {
        let servers = crate::syphon::servers();
        if servers.is_empty() {
            notes.push((InputKind::Syphon, "No app is publishing Syphon right now.".into()));
        }
        out.extend(servers.iter().map(|s| LiveInput { kind: InputKind::Syphon, label: s.label(), source: syphon_source(&s.label()) }));
        let (targets, error) = crate::screen_capture::targets();
        match error {
            Some(e) => notes.push((InputKind::Screen, e)),
            None if targets.is_empty() => notes.push((InputKind::Screen, "Looking…".into())),
            None => {}
        }
        for t in targets {
            let kind = if t.url.starts_with("display:") { InputKind::Screen } else { InputKind::Window };
            out.push(LiveInput { kind, source: screen_source(&t.label, &t.url), label: t.label });
        }
    }
    #[cfg(any(target_os = "macos", windows))]
    {
        let sources = crate::ndi::sources();
        if sources.is_empty() {
            let note = if crate::ndi::Ndi::library().is_some() { "No NDI sources found yet (they appear within a second or two)." } else { "Install the free NDI Tools (ndi.video) to receive NDI." };
            notes.push((InputKind::Ndi, note.into()));
        }
        out.extend(sources.iter().map(|s| LiveInput { kind: InputKind::Ndi, label: s.clone(), source: ndi_source(s) }));
    }
    (out, notes)
}

/// The camera list, asked for once and kept in egui's memory (`refresh` asks again).
pub fn cameras_cached(ctx: &eframe::egui::Context, refresh: bool) -> Vec<String> {
    let id = eframe::egui::Id::new("live_cameras");
    if !refresh && let Some(c) = ctx.data(|d| d.get_temp::<Vec<String>>(id)) {
        return c;
    }
    let list: Vec<String> = cameras().into_iter().map(|c| c.name).collect();
    ctx.data_mut(|d| d.insert_temp(id, list.clone()));
    list
}

pub fn open(name: &str, format: &str, url: &str) -> Result<VideoPlayer, String> {
    // A camera on Windows: DirectShow by its name (a big buffer, so a 4K capture card doesn't drop frames while the
    // first ones decode); the card's own frame rate, else 30.
    #[cfg(windows)]
    if format == "avfoundation" {
        struct Opening;
        impl Drop for Opening {
            fn drop(&mut self) {
                OPENING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
        OPENING.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _opening = Opening;
        // The second of two same-named cards: by its number among them.
        let (device_name, number) = repeat_of(url);
        let device = format!("video={device_name}");
        let number = number.to_string();
        let first = [("rtbufsize", "256M"), ("video_device_number", number.as_str())];
        let second = [("rtbufsize", "256M"), ("framerate", "30"), ("video_device_number", number.as_str())];
        let tries: &[&[(&str, &str)]] = &[&first, &second];
        let mut result = VideoPlayer::open_live(name, "dshow", &device, tries);
        // Busy for a moment (another program, or the list being read when it started): twice more, a second apart.
        for _ in 0..2 {
            match &result {
                Err(e) if busy(e) => {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    result = VideoPlayer::open_live(name, "dshow", &device, tries);
                }
                _ => break,
            }
        }
        return result;
    }
    // Linux: Video4Linux, by the device the name belongs to now (numbers change when cards are plugged in).
    #[cfg(target_os = "linux")]
    if format == "avfoundation" {
        let device = v4l2_devices().into_iter().find(|(n, _)| n == url).map_or_else(|| url.to_string(), |(_, d)| d);
        let tries: &[&[(&str, &str)]] = &[&[], &[("framerate", "30")]];
        return VideoPlayer::open_live(name, "v4l2", &device, tries);
    }
    if format == "avfoundation" {
        // Cameras only open at a frame rate they support; try the common ones. NV12 is what
        // the GPU path uploads directly.
        let rates: &[&[(&str, &str)]] = &[
            &[("framerate", "30"), ("pixel_format", "nv12")],
            &[("framerate", "30")],
            &[("framerate", "29.970030")],
            &[("framerate", "60")],
            &[("framerate", "25")],
            &[("framerate", "15")],
        ];
        return VideoPlayer::open_live(name, format, &format!("{url}:none"), rates);
    }
    // Streams: low-latency demuxing; TCP for RTSP (works through more networks).
    let low_latency: &[&[(&str, &str)]] = &[&[("fflags", "nobuffer"), ("flags", "low_delay"), ("rtsp_transport", "tcp"), ("timeout", "5000000")]];
    VideoPlayer::open_live(name, format, url, low_latency)
}

#[cfg(test)]
mod tests {
    #[test]
    fn capture_cards_on_windows_are_read_from_ffmpegs_device_list() {
        // FFmpeg 5 and later: one line a device, with its kind.
        let now = "[in#0 @ 000001] \"Cam Link 4K\" (video)\n[in#0 @ 000001]   Alternative name \"@device_pnp_\\\\?\\usb#vid_0fd9\"\n[in#0 @ 000001] \"USB Video\" (video)\n[in#0 @ 000001] \"Microphone (Cam Link 4K)\" (audio)\n[in#0 @ 000001] \"OBS Virtual Camera\" (none)\n[in#0 @ 000001] \"Elgato HD60 X\" (audio, video)\ndummy: Immediate exit requested\n";
        assert_eq!(super::dshow_devices(now), ["Cam Link 4K", "USB Video", "OBS Virtual Camera", "Elgato HD60 X"]);
        // As a Windows 11 machine answered (webcam + OBS Virtual Camera + a microphone and line in).
        let real = "[in#0 @ 000001fb685af840] \"Razer Kiyo Pro\" (video)\n[in#0 @ 000001fb685af840]   Alternative name \"@device_pnp_x\"\n[in#0 @ 000001fb685af840] \"OBS Virtual Camera\" (none)\n[in#0 @ 000001fb685af840] \"Microphone (Razer Kiyo Pro)\" (audio)\n[in#0 @ 000001fb685af840] \"Line In (High Definition Audio Device)\" (audio)\nError opening input file dummy.\n";
        assert_eq!(super::dshow_devices(real), ["Razer Kiyo Pro", "OBS Virtual Camera"]);
        // Older FFmpeg: headings, then names.
        let old = "[dshow @ 0x1] DirectShow video devices (some may be both video and audio devices)\n[dshow @ 0x1]  \"Integrated Camera\"\n[dshow @ 0x1]     Alternative name \"@device_pnp_x\"\n[dshow @ 0x1] DirectShow audio devices\n[dshow @ 0x1]  \"Microphone\"\n";
        assert_eq!(super::dshow_devices(old), ["Integrated Camera"]);
        // A laptop's own webcam on Windows 11 also says "(none)".
        assert_eq!(super::dshow_devices("[in#0 @ 01] \"Integrated Webcam\" (none)\n[in#0 @ 01] \"Microphone (Realtek(R) Audio)\" (audio)\n"), ["Integrated Webcam"]);
        assert!(super::busy("Live: can't open (Input/output error)") && !super::busy("can't open (No such file or directory)"));
        // Two of the same capture card (each its own alternative name): both listed, the second numbered.
        let twins = "[in#0 @ 01] \"USB3 Capture\" (video)\n[in#0 @ 01]   Alternative name \"@device_pnp_a\"\n[in#0 @ 01] \"USB3 Capture\" (video)\n[in#0 @ 01]   Alternative name \"@device_pnp_b\"\n[in#0 @ 01] \"USB3 Capture\" (audio)\n[in#0 @ 01]   Alternative name \"@device_cm_c\"\n";
        assert_eq!(super::dshow_devices(twins), ["USB3 Capture", "USB3 Capture #2"]);
        // The same device listed twice (same alternative name) is one device.
        let again = "[in#0 @ 01] \"Cam\" (video)\n[in#0 @ 01]   Alternative name \"@device_pnp_a\"\n[in#0 @ 01] \"Cam\" (video)\n[in#0 @ 01]   Alternative name \"@device_pnp_a\"\n";
        assert_eq!(super::dshow_devices(again), ["Cam"]);
        assert_eq!(super::repeat_of("USB3 Capture #2"), ("USB3 Capture", 1));
        assert_eq!(super::repeat_of("USB3 Capture"), ("USB3 Capture", 0));
        assert_eq!(super::repeat_of("Studio #1"), ("Studio #1", 0), "#1 is never made: a name of its own");
    }

    /// Lists cameras (no permission needed) so the test log shows what this Mac has.
    #[test]
    fn lists_cameras() {
        let cams = super::cameras();
        println!("cameras: {:?}", cams.iter().map(|c| &c.name).collect::<Vec<_>>());
    }
}
