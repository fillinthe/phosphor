//! Live streaming and recording without OBS: output frames are piped to FFmpeg, which encodes
//! with VideoToolbox and sends to RTMP (YouTube, Twitch, …), SRT, or a file.

#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Destination {
    #[default]
    YouTube,
    Twitch,
    CustomRtmp,
    Srt,
    File,
}

impl Destination {
    pub const ALL: [Destination; 5] =
        [Destination::YouTube, Destination::Twitch, Destination::CustomRtmp, Destination::Srt, Destination::File];

    pub fn name(self) -> &'static str {
        match self {
            Destination::YouTube => "YouTube Live",
            Destination::Twitch => "Twitch",
            Destination::CustomRtmp => "Custom RTMP",
            Destination::Srt => "SRT",
            Destination::File => "Record to file",
        }
    }
}

/// What a recording to a file is encoded as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FileFormat {
    /// By the file name: .mp4 = H.264, .mov = ProRes 422.
    #[default]
    Auto,
    /// Hap (DXT1): plays back with almost no CPU in VJ apps.
    Hap,
    /// Hap Alpha (DXT5): keeps transparency.
    HapAlpha,
    /// HEVC (H.265) through the Mac's encoder: about half the size of H.264 at the same quality.
    Hevc,
}

impl FileFormat {
    pub const ALL: [FileFormat; 4] = [FileFormat::Auto, FileFormat::Hap, FileFormat::HapAlpha, FileFormat::Hevc];

    pub fn name(self) -> &'static str {
        match self {
            FileFormat::Auto => "H.264 (.mp4) / ProRes (.mov)",
            FileFormat::Hap => "Hap (.mov)",
            FileFormat::HapAlpha => "Hap Alpha (.mov)",
            FileFormat::Hevc => "HEVC (.mp4, smaller files)",
        }
    }
}

impl phosphor_engine::Choice for FileFormat {
    const OPTIONS: &'static [Self] = &FileFormat::ALL;
    fn label(self) -> &'static str {
        self.name()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamSettings {
    pub destination: Destination,
    pub rtmp_url: String,
    pub stream_key: String,
    pub srt_url: String,
    pub file_path: String,
    pub file_format: FileFormat,
    /// 0 = same as canvas.
    pub height: u32,
    pub fps: u32,
    pub kbps: u32,
    /// AVFoundation audio device index to include (-1 = no audio).
    pub audio_device: i32,
    /// A second live destination at the same time (one encode, FFmpeg's tee): YouTube, Twitch or custom RTMP.
    pub second_enabled: bool,
    pub second_destination: Destination,
    pub second_key: String,
    pub second_url: String,
    /// While streaming, also write an H.264 copy to this file.
    pub copy_enabled: bool,
    pub copy_path: String,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            destination: Destination::YouTube,
            rtmp_url: "rtmp://live.example.com/app".into(),
            stream_key: String::new(),
            srt_url: "srt://host:9000?mode=caller".into(),
            file_path: String::new(),
            file_format: FileFormat::Auto,
            height: 1080,
            fps: 30,
            kbps: 6000,
            audio_device: -1,
            second_enabled: false,
            second_destination: Destination::Twitch,
            second_key: String::new(),
            second_url: "rtmp://live.example.com/app".into(),
            copy_enabled: false,
            copy_path: String::new(),
        }
    }
}

impl StreamSettings {
    /// Where a recording to a file ends up (Hap always goes in a .mov).
    pub fn recorded_path(&self) -> String {
        let p = self.file_path.trim();
        match self.file_format {
            FileFormat::Auto => p.to_string(),
            FileFormat::Hevc => std::path::Path::new(p).with_extension("mp4").to_string_lossy().to_string(),
            _ => std::path::Path::new(p).with_extension("mov").to_string_lossy().to_string(),
        }
    }

    /// An RTMP address for a live destination with these credentials (None for SRT / file).
    fn rtmp_target(dest: Destination, url: &str, key: &str) -> Option<String> {
        Some(match dest {
            Destination::YouTube => format!("rtmp://a.rtmp.youtube.com/live2/{key}"),
            Destination::Twitch => format!("rtmp://live.twitch.tv/app/{key}"),
            Destination::CustomRtmp => {
                let url = url.trim_end_matches('/');
                if key.is_empty() { url.to_string() } else { format!("{url}/{key}") }
            }
            _ => return None,
        })
    }

    /// Everything the stream goes to besides the main destination: (container, address).
    fn extra_targets(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if self.second_enabled && let Some(url) = Self::rtmp_target(self.second_destination, &self.second_url, &self.second_key) {
            out.push(("flv", url));
        }
        if self.copy_enabled && self.destination != Destination::File && !self.copy_path.trim().is_empty() {
            out.push(("mp4", self.copy_path.trim().to_string()));
        }
        out
    }

    fn target(&self) -> (Vec<String>, String) {
        match self.destination {
            Destination::YouTube | Destination::Twitch | Destination::CustomRtmp => {
                (vec!["-f".into(), "flv".into()], Self::rtmp_target(self.destination, &self.rtmp_url, &self.stream_key).unwrap_or_default())
            }
            Destination::Srt => (vec!["-f".into(), "mpegts".into()], self.srt_url.clone()),
            Destination::File => (vec!["-movflags".into(), "+faststart".into()], self.file_path.clone()),
        }
    }
}

#[derive(Default)]
struct Shared {
    status: String,
    error: bool,
    sent: u64,
    dropped: u64,
    log: Vec<String>,
}

enum Msg {
    Frame(Vec<u8>),
    Repeat,
}

pub struct Streamer {
    pub settings: StreamSettings,
    tx: Option<SyncSender<Msg>>,
    child: Arc<Mutex<Option<Child>>>,
    shared: Arc<Mutex<Shared>>,
    pacer: crate::pacing::FramePacer,
    pub frame_size: (u32, u32),
    started: Option<f64>,
    /// Size the user started streaming at; while set, a dropped network stream reconnects by itself.
    wanted: Option<(u32, u32)>,
    /// Next reconnect attempt (app seconds) and how many in a row so far.
    retry_at: Option<f64>,
    retries: u32,
}

/// Seconds before reconnect attempt `n` (1, 2, 3…): 2, 4, 8, then every 15.
pub fn reconnect_delay(n: u32) -> f64 {
    (2.0 * 2f64.powi(n.saturating_sub(1).min(3) as i32)).min(15.0)
}

/// AVFoundation audio capture devices as (index, name), from FFmpeg's device listing.
pub fn list_audio_devices() -> Vec<(i32, String)> {
    let Some(ffmpeg) = ffmpeg_path() else { return Vec::new() };
    let Ok(out) = Command::new(ffmpeg).args(["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""]).stdin(Stdio::null()).output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stderr);
    let mut devices = Vec::new();
    let mut in_audio = false;
    for line in text.lines() {
        if line.contains("audio devices") {
            in_audio = true;
            continue;
        }
        if line.contains("video devices") {
            in_audio = false;
        }
        if in_audio && let Some(rest) = line.split("] [").nth(1) && let Some((idx, name)) = rest.split_once("] ") {
            if let Ok(i) = idx.trim().parse::<i32>() {
                devices.push((i, name.trim().to_string()));
            }
        }
    }
    devices
}

/// The FFmpeg command line used for streaming and ProRes / H.264 recording: the copy inside the app bundle
/// first (scripts/bundle-mac.sh ships one), then Homebrew / MacPorts, then whatever is on the PATH.
pub fn ffmpeg_path() -> Option<String> {
    let bundled = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("ffmpeg"))).filter(|p| p.exists());
    bundled
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/opt/local/bin/ffmpeg"].iter().find(|p| std::path::Path::new(p).exists()).map(|p| p.to_string()))
        .or_else(|| {
            Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().ok().map(|_| "ffmpeg".into())
        })
}

impl Streamer {
    pub fn new(settings: StreamSettings) -> Self {
        Self {
            settings,
            tx: None,
            child: Arc::new(Mutex::new(None)),
            shared: Arc::new(Mutex::new(Shared::default())),
            pacer: Default::default(),
            frame_size: (0, 0),
            started: None,
            wanted: None,
            retry_at: None,
            retries: 0,
        }
    }

    pub fn running(&self) -> bool {
        self.tx.is_some()
    }

    pub fn status(&self) -> (String, bool, u64, u64, Vec<String>) {
        let s = self.shared.lock().unwrap();
        (s.status.clone(), s.error, s.sent, s.dropped, s.log.clone())
    }

    pub fn elapsed(&self, now: f64) -> f64 {
        self.started.map_or(0.0, |t| now - t)
    }

    /// Starts FFmpeg for frames of `width`×`height` (the canvas size).
    pub fn start(&mut self, width: u32, height: u32, now: f64) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (width, height, now);
            return Err("Streaming from the browser arrives with network sessions".into());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start_native(width, height, now)?;
            self.wanted = Some((width, height));
            self.retries = 0;
            self.retry_at = None;
            Ok(())
        }
    }

    /// While a network stream should be running but isn't (FFmpeg exited: connection lost, server
    /// hiccup), restarts it with growing pauses. Recordings to a file never restart (they'd overwrite it).
    fn keep_alive(&mut self, now: f64) {
        let Some((w, h)) = self.wanted else { return };
        if self.running() || self.settings.destination == Destination::File {
            if !self.running() {
                self.wanted = None;
            }
            return;
        }
        let at = *self.retry_at.get_or_insert_with(|| {
            self.retries += 1;
            now + reconnect_delay(self.retries)
        });
        if now < at {
            let mut s = self.shared.lock().unwrap();
            s.status = format!("Connection lost · reconnecting in {:.0} s (try {})", (at - now).ceil(), self.retries);
            s.error = true;
            return;
        }
        self.retry_at = None;
        let started = self.started;
        #[cfg(not(target_arch = "wasm32"))]
        match self.start_native(w, h, now) {
            Ok(()) => {
                // Elapsed time keeps counting from the first start.
                self.started = started.or(Some(now));
                let mut s = self.shared.lock().unwrap();
                s.status = format!("{} · reconnected (try {})", s.status, self.retries);
            }
            Err(e) => self.shared.lock().unwrap().status = format!("Reconnect failed: {e}"),
        }
    }

    /// Recording to Hap: Phosphor encodes and writes the movie itself (no FFmpeg program needed).
    #[cfg(not(target_arch = "wasm32"))]
    fn start_hap(&mut self, width: u32, height: u32, now: f64) -> Result<(), String> {
        let s = &self.settings;
        if s.file_path.trim().is_empty() {
            return Err("Choose a file first".into());
        }
        let path = s.recorded_path();
        let alpha = s.file_format == FileFormat::HapAlpha;
        let fps = s.fps.clamp(1, 120);
        let mut writer = crate::media::video::hap_encode::HapWriter::create(&path, width, height, fps, alpha)?;
        let (tx, rx) = std::sync::mpsc::sync_channel::<Msg>(4);
        let shared = self.shared.clone();
        std::thread::Builder::new()
            .name("phosphor-hap-record".into())
            .spawn(move || {
                let mut last: Vec<u8> = Vec::new();
                while let Ok(msg) = rx.recv() {
                    if let Msg::Frame(f) = msg {
                        last = f;
                    }
                    if last.is_empty() {
                        continue;
                    }
                    if let Err(e) = writer.write(&last) {
                        let mut s = shared.lock().unwrap();
                        s.status = format!("Recording stopped: {e}");
                        s.error = true;
                        break;
                    }
                    shared.lock().unwrap().sent += 1;
                }
                if let Err(e) = writer.finish() {
                    let mut s = shared.lock().unwrap();
                    s.status = format!("Couldn't finish the movie: {e}");
                    s.error = true;
                }
            })
            .map_err(|e| e.to_string())?;
        self.tx = Some(tx);
        self.frame_size = (width, height);
        self.pacer.reset(now);
        self.started = Some(now);
        let kind = if alpha { "Hap Alpha" } else { "Hap" };
        *self.shared.lock().unwrap() = Shared { status: format!("Recording {kind} · {path}"), ..Default::default() };
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_native(&mut self, width: u32, height: u32, now: f64) -> Result<(), String> {
        self.close();
        if self.settings.destination == Destination::File && matches!(self.settings.file_format, FileFormat::Hap | FileFormat::HapAlpha) {
            return self.start_hap(width, height, now);
        }
        let ffmpeg = ffmpeg_path().ok_or("FFmpeg not found. Install it with: brew install ffmpeg")?;
        let s = &self.settings;
        let (fmt, target) = s.target();
        if target.trim().is_empty() || (matches!(s.destination, Destination::YouTube | Destination::Twitch) && s.stream_key.is_empty()) {
            return Err("Enter the stream key / destination first".into());
        }
        let fps = s.fps.clamp(1, 120);
        let mut args: Vec<String> = vec![
            "-hide_banner", "-loglevel", "error", "-nostats", "-y",
            "-thread_queue_size", "1024",
            "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{width}x{height}"), "-framerate", &fps.to_string(), "-i", "pipe:0",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        if s.audio_device >= 0 {
            // Program audio from a capture device (a mic, or a loopback like BlackHole for the DJ feed).
            args.extend(
                ["-thread_queue_size", "1024", "-f", "avfoundation", "-i", &format!(":{}", s.audio_device), "-map", "0:v", "-map", "1:a", "-c:a", "aac", "-b:a", "160k", "-ar", "48000"]
                    .map(String::from),
            );
        } else {
            args.push("-an".into());
        }
        if s.height > 0 && s.height != height {
            args.extend(["-vf".into(), format!("scale=-2:{}", s.height)]);
        }
        let file_mov = s.destination == Destination::File && target.to_lowercase().ends_with(".mov");
        if s.destination == Destination::File && s.file_format == FileFormat::Hevc {
            // HEVC holds the same quality at about 60 % of H.264's bitrate: that's where the smaller file comes from.
            let kbps = (s.kbps as f32 * 0.6) as u32;
            args.extend(
                ["-c:v", "hevc_videotoolbox", "-b:v", &format!("{kbps}k"), "-maxrate", &format!("{kbps}k"), "-tag:v", "hvc1", "-g", &(fps * 2).to_string(), "-pix_fmt", "yuv420p"]
                    .map(String::from),
            );
        } else if file_mov {
            args.extend(["-c:v", "prores_videotoolbox", "-profile:v", "2", "-pix_fmt", "yuv422p10le"].map(String::from));
        } else {
            args.extend(
                [
                    "-c:v", "h264_videotoolbox", "-b:v", &format!("{}k", s.kbps), "-maxrate", &format!("{}k", s.kbps),
                    "-bufsize", &format!("{}k", s.kbps * 2), "-g", &(fps * 2).to_string(), "-pix_fmt", "yuv420p",
                ]
                .map(String::from),
            );
        }
        let extra = s.extra_targets();
        if extra.is_empty() {
            args.extend(fmt);
            args.push(target.clone());
        } else {
            // One encode, several outputs: FFmpeg's tee. A destination that drops out doesn't stop the others.
            let main_fmt = fmt.get(1).cloned().unwrap_or_else(|| "flv".into());
            let mut slaves = vec![format!("[f={main_fmt}:onfail=ignore]{target}")];
            for (f, url) in &extra {
                slaves.push(format!("[f={f}:onfail=ignore]{url}"));
            }
            // tee picks no streams by itself: name them (audio was mapped above when a device is on).
            if s.audio_device < 0 {
                args.extend(["-map", "0:v"].map(String::from));
            }
            args.extend(["-f", "tee", "-flags", "+global_header"].map(String::from));
            args.push(slaves.join("|"));
        }

        log::debug!("ffmpeg {}", args.join(" "));
        let mut child = Command::new(ffmpeg)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Couldn't start FFmpeg: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("FFmpeg stdin unavailable")?;
        let stderr = child.stderr.take();
        *self.child.lock().unwrap() = Some(child);

        let shared = self.shared.clone();
        if let Some(stderr) = stderr {
            let shared = shared.clone();
            std::thread::spawn(move || {
                use std::io::BufRead;
                for line in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut s = shared.lock().unwrap();
                    s.log.push(line);
                    if s.log.len() > 8 {
                        s.log.remove(0);
                    }
                }
            });
        }
        let (tx, rx) = std::sync::mpsc::sync_channel::<Msg>(4);
        std::thread::Builder::new()
            .name("phosphor-stream".into())
            .spawn(move || {
                let mut last: Vec<u8> = Vec::new();
                while let Ok(msg) = rx.recv() {
                    let data = match msg {
                        Msg::Frame(f) => {
                            last = f;
                            &last
                        }
                        Msg::Repeat if !last.is_empty() => &last,
                        Msg::Repeat => continue,
                    };
                    if let Err(e) = stdin.write_all(data) {
                        let mut s = shared.lock().unwrap();
                        s.status = format!("Stream ended: {e}");
                        s.error = true;
                        break;
                    }
                    shared.lock().unwrap().sent += 1;
                }
                let _ = stdin.flush();
            })
            .map_err(|e| e.to_string())?;
        self.tx = Some(tx);
        self.frame_size = (width, height);
        self.pacer.reset(now);
        self.started = Some(now);
        let mut sh = self.shared.lock().unwrap();
        *sh = Shared { status: format!("{} · {}", s.destination.name(), target), ..Default::default() };
        Ok(())
    }

    /// Stops for good (no reconnecting): closes the pipe so FFmpeg finishes the file / stream.
    pub fn stop(&mut self) {
        self.wanted = None;
        self.retry_at = None;
        self.close();
    }

    /// Closes the pipe and reaps FFmpeg in the background.
    fn close(&mut self) {
        self.tx = None;
        self.started = None;
        #[cfg(not(target_arch = "wasm32"))]
        let child = self.child.clone();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            if let Some(mut c) = child.lock().unwrap().take() {
                let _ = c.wait();
            }
        });
    }

    /// Blocking variant for benchmarks: waits for FFmpeg to exit.
    pub fn stop_and_wait(&mut self) {
        self.wanted = None;
        self.tx = None;
        self.started = None;
        std::thread::sleep(std::time::Duration::from_millis(50));
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.wait();
        }
    }

    /// True when the next frame at the stream's frame rate is due.
    pub fn frame_due(&mut self, now: f64) -> bool {
        self.keep_alive(now);
        self.running() && self.pacer.due(now, self.settings.fps)
    }

    pub fn push(&mut self, width: u32, height: u32, rgba: Vec<u8>) {
        if (width, height) != self.frame_size {
            return;
        }
        self.send(Msg::Frame(rgba));
    }

    /// Keeps the timeline honest when a capture slot wasn't free: resend the previous frame.
    pub fn repeat_last(&mut self) {
        self.send(Msg::Repeat);
    }

    fn send(&mut self, msg: Msg) {
        let Some(tx) = &self.tx else { return };
        match tx.try_send(msg) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.shared.lock().unwrap().dropped += 1,
            Err(TrySendError::Disconnected(_)) => {
                self.tx = None;
                self.started = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reconnects_back_off_to_every_15_seconds() {
        let d: Vec<f64> = (1..=6).map(super::reconnect_delay).collect();
        assert_eq!(d, vec![2.0, 4.0, 8.0, 15.0, 15.0, 15.0]);
    }

    /// A stream to a server that isn't there drops; Phosphor waits, tries again, and stops trying after Stop.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_dropped_stream_reconnects_until_stopped() {
        use super::*;
        if ffmpeg_path().is_none() {
            return;
        }
        let settings = StreamSettings { destination: Destination::CustomRtmp, rtmp_url: "rtmp://127.0.0.1:1/live".into(), stream_key: "test".into(), fps: 10, height: 0, audio_device: -1, ..Default::default() };
        let mut s = Streamer::new(settings);
        s.start(64, 36, 0.0).unwrap();
        let frame = vec![0u8; 64 * 36 * 4];
        // FFmpeg can't connect and exits; frames start failing.
        let mut t = 0.0;
        while s.running() && t < 10.0 {
            s.push(64, 36, frame.clone());
            std::thread::sleep(std::time::Duration::from_millis(50));
            t += 0.05;
        }
        assert!(!s.running(), "FFmpeg should have given up on a closed port");
        s.frame_due(t);
        assert!(s.status().0.contains("reconnecting"), "{}", s.status().0);
        s.frame_due(t + 2.1);
        assert!(s.running(), "restarted after 2 s");
        s.stop();
        s.frame_due(t + 100.0);
        assert!(!s.running(), "Stop means stop");
    }
}
