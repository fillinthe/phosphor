//! Live inputs as layer sources: cameras and capture cards (macOS AVFoundation through FFmpeg),
//! and network streams (RTSP, SRT, HTTP/HLS, RTMP…). Decoded by the same player as movies, in
//! live mode (newest frame always wins, so there's no build-up of delay).

use phosphor_engine::SourceSpec;

use super::video::VideoPlayer;

/// A camera or capture device on this Mac.
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

#[cfg(not(target_os = "macos"))]
pub fn cameras() -> Vec<Camera> {
    Vec::new()
}

/// The source for a camera.
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
    /// Lists cameras (no permission needed) so the test log shows what this Mac has.
    #[test]
    fn lists_cameras() {
        let cams = super::cameras();
        println!("cameras: {:?}", cams.iter().map(|c| &c.name).collect::<Vec<_>>());
    }
}
