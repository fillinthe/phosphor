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
    /// WebRTC ingest (WHIP): sub-second latency to Cloudflare Stream, Dolby.io / Millicast, OBS, MediaMTX…
    Whip,
    File,
}

impl Destination {
    pub const ALL: [Destination; 6] =
        [Destination::YouTube, Destination::Twitch, Destination::CustomRtmp, Destination::Srt, Destination::Whip, Destination::File];

    pub fn name(self) -> &'static str {
        match self {
            Destination::YouTube => "YouTube Live",
            Destination::Twitch => "Twitch",
            Destination::CustomRtmp => "Custom RTMP",
            Destination::Srt => "SRT",
            Destination::Whip => "WebRTC (WHIP)",
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
    /// WHIP endpoint (https://…/whip); the bearer token, if the service wants one, is `stream_key`.
    pub whip_url: String,
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
    /// YouTube / Twitch over RTMPS (TLS) instead of plain RTMP.
    pub secure: bool,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            destination: Destination::YouTube,
            rtmp_url: "rtmp://live.example.com/app".into(),
            stream_key: String::new(),
            srt_url: "srt://host:9000?mode=caller".into(),
            whip_url: String::new(),
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
            secure: true,
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
    fn rtmp_target(dest: Destination, url: &str, key: &str, secure: bool) -> Option<String> {
        Some(match dest {
            Destination::YouTube if secure => format!("rtmps://a.rtmps.youtube.com:443/live2/{key}"),
            Destination::YouTube => format!("rtmp://a.rtmp.youtube.com/live2/{key}"),
            Destination::Twitch if secure => format!("rtmps://live.twitch.tv:443/app/{key}"),
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
        if self.destination == Destination::Whip {
            return out; // WebRTC goes out on its own (FFmpeg's tee can't carry it)
        }
        if self.second_enabled && let Some(url) = Self::rtmp_target(self.second_destination, &self.second_url, &self.second_key, self.secure) {
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
                (vec!["-f".into(), "flv".into()], Self::rtmp_target(self.destination, &self.rtmp_url, &self.stream_key, self.secure).unwrap_or_default())
            }
            Destination::Srt => (vec!["-f".into(), "mpegts".into()], self.srt_url.clone()),
            Destination::Whip => {
                let mut fmt = vec!["-f".into(), "whip".into()];
                if !self.stream_key.trim().is_empty() {
                    fmt.extend(["-authorization".into(), self.stream_key.trim().to_string()]);
                }
                (fmt, self.whip_url.trim().to_string())
            }
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
    /// One sample a second from FFmpeg's progress report, newest last (two minutes kept, across reconnects).
    history: std::collections::VecDeque<Sample>,
}

/// One second of a stream's life: what FFmpeg reported plus Phosphor's own dropped-frame count at that moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    /// Average bitrate so far (kbit/s).
    pub kbps: f32,
    /// Encoding speed against real time (1.0 = keeping up).
    pub speed: f32,
    /// Frames Phosphor had to drop so far (FFmpeg / the network couldn't take them).
    pub dropped: u64,
}

pub const HISTORY: usize = 120;
/// Samples at the start of a stream left out of the verdict and the red marks.
pub const WARM_UP: usize = 3;

/// How the last ten seconds went.
#[derive(Clone, Debug, PartialEq)]
pub enum Health {
    /// Not enough data yet.
    Starting,
    Good,
    /// Frames dropped or falling behind real time: the connection (or the computer) can't keep up.
    Struggling { dropped: u64, speed: f32 },
}

impl Health {
    pub fn of(history: &std::collections::VecDeque<Sample>) -> Health {
        // FFmpeg's first seconds run below real time while it warms up: they say nothing about the connection.
        let n = history.len().saturating_sub(WARM_UP).min(10);
        if n < 3 {
            return Health::Starting;
        }
        let recent: Vec<&Sample> = history.iter().rev().take(n).collect();
        let dropped = recent[0].dropped.saturating_sub(recent[n - 1].dropped);
        let speed = recent.iter().map(|s| s.speed).sum::<f32>() / n as f32;
        if dropped > 0 || speed < 0.95 { Health::Struggling { dropped, speed } } else { Health::Good }
    }
}

/// Reads FFmpeg's `-progress` report (key=value lines, a block per second ending in `progress=…`).
#[derive(Default)]
struct Progress {
    kbps: f32,
    speed: f32,
}

impl Progress {
    /// Feeds one line; returns a finished sample (without the drop count) at the end of a block.
    fn line(&mut self, line: &str) -> Option<(f32, f32)> {
        let (k, v) = line.trim().split_once('=')?;
        let v = v.trim();
        match k {
            "bitrate" => self.kbps = v.trim_end_matches("kbits/s").trim().parse().unwrap_or(self.kbps),
            "speed" => self.speed = v.trim_end_matches('x').trim().parse().unwrap_or(self.speed),
            "progress" => return Some((self.kbps, self.speed)),
            _ => {}
        }
        None
    }
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
    /// Mute all sound is on: streams and recordings start without the audio device.
    pub mute_audio: bool,
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
            mute_audio: false,
        }
    }

    pub fn running(&self) -> bool {
        self.tx.is_some()
    }

    pub fn status(&self) -> (String, bool, u64, u64, Vec<String>) {
        let s = self.shared.lock().unwrap();
        (s.status.clone(), s.error, s.sent, s.dropped, s.log.clone())
    }

    /// The last two minutes, one sample a second.
    pub fn history(&self) -> std::collections::VecDeque<Sample> {
        self.shared.lock().unwrap().history.clone()
    }

    pub fn health(&self) -> Health {
        Health::of(&self.shared.lock().unwrap().history)
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
        if s.destination == Destination::Whip && !s.whip_url.trim().starts_with("http") {
            return Err("Enter the WHIP address (https://…) from your WebRTC service".into());
        }
        if target.trim().is_empty() || (matches!(s.destination, Destination::YouTube | Destination::Twitch) && s.stream_key.is_empty()) {
            return Err("Enter the stream key / destination first".into());
        }
        let fps = s.fps.clamp(1, 120);
        let mut args: Vec<String> = vec![
            "-hide_banner", "-loglevel", "error", "-nostats", "-progress", "pipe:1", "-stats_period", "1", "-y",
            "-thread_queue_size", "1024",
            "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{width}x{height}"), "-framerate", &fps.to_string(), "-i", "pipe:0",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let whip = s.destination == Destination::Whip;
        let audio_device = if self.mute_audio { -1 } else { s.audio_device };
        if audio_device >= 0 {
            // Program audio from a capture device (a mic, or a loopback like BlackHole for the DJ feed).
            args.extend(["-thread_queue_size", "1024", "-f", "avfoundation", "-i", &format!(":{audio_device}"), "-map", "0:v", "-map", "1:a"].map(String::from));
            // WebRTC carries Opus (FFmpeg's own encoder, marked experimental); everything else AAC.
            let codec: &[&str] = if whip { &["-c:a", "opus", "-strict", "-2", "-b:a", "128k", "-ac", "2"] } else { &["-c:a", "aac", "-b:a", "160k"] };
            args.extend(codec.iter().map(|a| a.to_string()));
            args.extend(["-ar", "48000"].map(String::from));
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
            if whip {
                // WebRTC players take H.264 Baseline only (no B-frames); a keyframe every second for fast joins.
                args.extend(["-profile:v", "baseline", "-bf", "0", "-g", &fps.to_string()].map(String::from));
            }
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
            if audio_device < 0 {
                args.extend(["-map", "0:v"].map(String::from));
            }
            args.extend(["-f", "tee", "-flags", "+global_header"].map(String::from));
            args.push(slaves.join("|"));
        }

        log::debug!("ffmpeg {}", args.join(" "));
        let mut child = Command::new(ffmpeg)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Couldn't start FFmpeg: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("FFmpeg stdin unavailable")?;
        let stderr = child.stderr.take();
        let progress = child.stdout.take();
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
        if let Some(out) = progress {
            let shared = shared.clone();
            std::thread::spawn(move || {
                use std::io::BufRead;
                let mut p = Progress::default();
                for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                    if let Some((kbps, speed)) = p.line(&line) {
                        let mut s = shared.lock().unwrap();
                        let dropped = s.dropped;
                        s.history.push_back(Sample { kbps, speed, dropped });
                        while s.history.len() > HISTORY {
                            s.history.pop_front();
                        }
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
        // A reconnect keeps the graph (the gap shows); a fresh Start begins a new one.
        let history = if self.retries > 0 { std::mem::take(&mut sh.history) } else { Default::default() };
        *sh = Shared { status: format!("{} · {}", s.destination.name(), target), history, ..Default::default() };
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
    use super::*;

    #[test]
    fn whip_goes_out_alone_with_its_token() {
        let s = StreamSettings { destination: Destination::Whip, whip_url: " https://ingest.example.com/whip ".into(), stream_key: "tok".into(), second_enabled: true, copy_enabled: true, copy_path: "/tmp/x.mp4".into(), ..Default::default() };
        let (fmt, target) = s.target();
        assert_eq!(fmt, vec!["-f", "whip", "-authorization", "tok"]);
        assert_eq!(target, "https://ingest.example.com/whip");
        assert!(s.extra_targets().is_empty(), "no tee for WebRTC");
    }

    #[test]
    fn progress_blocks_become_samples() {
        let mut p = Progress::default();
        let block = "frame=60\nfps=30.00\nbitrate=2048.5kbits/s\ntotal_size=123\nspeed=0.98x\nprogress=continue";
        let got: Vec<_> = block.lines().filter_map(|l| p.line(l)).collect();
        assert_eq!(got, vec![(2048.5, 0.98)]);
        // FFmpeg writes N/A before it knows: keep the last value.
        assert_eq!(["bitrate=N/A", "speed=N/A", "progress=continue"].iter().filter_map(|l| p.line(l)).collect::<Vec<_>>(), vec![(2048.5, 0.98)]);
    }

    #[test]
    fn health_looks_at_the_last_ten_seconds() {
        let mk = |speed: f32, dropped: u64| Sample { kbps: 3000.0, speed, dropped };
        // Warm-up seconds (slow) don't count.
        let mut h: std::collections::VecDeque<Sample> = [mk(0.5, 0), mk(0.7, 0), mk(0.9, 0), mk(1.0, 0), mk(1.0, 0)].into();
        assert_eq!(Health::of(&h), Health::Starting);
        h.extend((0..10).map(|_| mk(1.0, 5)));
        assert_eq!(Health::of(&h), Health::Good, "old drops don't count");
        h.push_back(mk(1.0, 9));
        assert_eq!(Health::of(&h), Health::Struggling { dropped: 4, speed: 1.0 });
        let slow: std::collections::VecDeque<Sample> = (0..14).map(|_| mk(0.8, 0)).collect();
        assert!(matches!(Health::of(&slow), Health::Struggling { dropped: 0, .. }));
    }

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

#[cfg(test)]
mod target_tests {
    use super::*;

    #[test]
    fn youtube_and_twitch_use_rtmps_when_secure() {
        assert_eq!(StreamSettings::rtmp_target(Destination::YouTube, "", "KEY", true).unwrap(), "rtmps://a.rtmps.youtube.com:443/live2/KEY");
        assert_eq!(StreamSettings::rtmp_target(Destination::Twitch, "", "KEY", true).unwrap(), "rtmps://live.twitch.tv:443/app/KEY");
        assert_eq!(StreamSettings::rtmp_target(Destination::YouTube, "", "KEY", false).unwrap(), "rtmp://a.rtmp.youtube.com/live2/KEY");
        assert_eq!(StreamSettings::rtmp_target(Destination::CustomRtmp, "rtmp://h/app/", "k", true).unwrap(), "rtmp://h/app/k");
        assert!(StreamSettings::rtmp_target(Destination::File, "", "", true).is_none());
    }

    #[test]
    fn extra_targets_follow_the_switches() {
        let mut s = StreamSettings { second_enabled: true, second_destination: Destination::Twitch, second_key: "T".into(), copy_enabled: true, copy_path: "/tmp/c.mp4".into(), ..Default::default() };
        let extra = s.extra_targets();
        assert_eq!(extra.len(), 2);
        assert_eq!(extra[0].0, "flv");
        assert_eq!(extra[1], ("mp4", "/tmp/c.mp4".to_string()));
        s.destination = Destination::File;
        assert_eq!(s.extra_targets().len(), 1, "no copy of a file recording");
    }
}
