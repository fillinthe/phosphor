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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamSettings {
    pub destination: Destination,
    pub rtmp_url: String,
    pub stream_key: String,
    pub srt_url: String,
    pub file_path: String,
    /// 0 = same as canvas.
    pub height: u32,
    pub fps: u32,
    pub kbps: u32,
    /// AVFoundation audio device index to include (-1 = no audio).
    pub audio_device: i32,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            destination: Destination::YouTube,
            rtmp_url: "rtmp://live.example.com/app".into(),
            stream_key: String::new(),
            srt_url: "srt://host:9000?mode=caller".into(),
            file_path: String::new(),
            height: 1080,
            fps: 30,
            kbps: 6000,
            audio_device: -1,
        }
    }
}

impl StreamSettings {
    fn target(&self) -> (Vec<String>, String) {
        match self.destination {
            Destination::YouTube => (vec!["-f".into(), "flv".into()], format!("rtmp://a.rtmp.youtube.com/live2/{}", self.stream_key)),
            Destination::Twitch => (vec!["-f".into(), "flv".into()], format!("rtmp://live.twitch.tv/app/{}", self.stream_key)),
            Destination::CustomRtmp => {
                let url = self.rtmp_url.trim_end_matches('/');
                let full = if self.stream_key.is_empty() { url.to_string() } else { format!("{url}/{}", self.stream_key) };
                (vec!["-f".into(), "flv".into()], full)
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

pub fn ffmpeg_path() -> Option<String> {
    ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/opt/local/bin/ffmpeg"]
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|p| p.to_string())
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
        self.start_native(width, height, now)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_native(&mut self, width: u32, height: u32, now: f64) -> Result<(), String> {
        self.stop();
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
        if file_mov {
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
        args.extend(fmt);
        args.push(target.clone());

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

    /// Closes the pipe so FFmpeg finishes the file / stream, and reaps it in the background.
    pub fn stop(&mut self) {
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
        self.tx = None;
        self.started = None;
        std::thread::sleep(std::time::Duration::from_millis(50));
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.wait();
        }
    }

    /// True when the next frame at the stream's frame rate is due.
    pub fn frame_due(&mut self, now: f64) -> bool {
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
