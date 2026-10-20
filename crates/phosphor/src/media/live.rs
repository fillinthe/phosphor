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
