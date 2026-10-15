//! FFmpeg-backed movie/GIF playback. Each player decodes on its own thread into a small bounded
//! queue; `poll_frame` hands out whichever frame is due on the caller's clock. VideoToolbox is used
//! for any codec it supports (H.264, HEVC, ProRes, ...) with transparent software fallback.

use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, Once};
use std::thread::JoinHandle;

use ff::ffi;
use ffi::AVPixelFormat as Px;
use ffmpeg_next as ff;

/// Decoded frames buffered ahead of display.
const QUEUE: usize = 3;
/// Spare frame buffers kept for reuse.
const POOL: usize = 4;
/// Frames later than this (seconds) are skipped before conversion...
const LATE: f64 = 0.05;
/// ...but never more than this many in a row, so slow decoders still show something.
const MAX_SKIP: u32 = 8;
const NOPTS: i64 = i64::MIN;
/// Reverse play decodes this many frames at a time, then shows them backwards (bounds memory:
/// ~37 MB at 1080p; intra-frame codecs like ProRes and Hap reverse at no extra cost, long-GOP
/// H.264/HEVC re-decode from the keyframe for each chunk).
const REVERSE_CHUNK_FRAMES: f64 = 12.0;

pub use phosphor_engine::PlayDirection;

/// How the decoder walks the movie: direction and an optional loop region (seconds).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PlayMode {
    direction: PlayDirection,
    range: Option<(f64, f64)>,
}

pub enum VideoFrame {
    /// 4:2:0 bi-planar, video range (BT.709 assumed), tightly or stride-packed rows.
    Nv12 {
        width: u32,
        height: u32,
        luma: Vec<u8>,
        luma_stride: u32,
        chroma: Vec<u8>,
        chroma_stride: u32,
    },
    /// Straight (non-premultiplied) RGBA8, for formats with alpha or palette (GIF, ProRes 4444, Hap Alpha, PNG, VP9 alpha...).
    Rgba {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
}

impl VideoFrame {
    pub fn size(&self) -> (u32, u32) {
        match self {
            Self::Nv12 { width, height, .. } | Self::Rgba { width, height, .. } => {
                (*width, *height)
            }
        }
    }

    fn is_rgba(&self) -> bool {
        matches!(self, Self::Rgba { .. })
    }
}

/// Diagnostics for a player.
#[derive(Clone, Debug, Default)]
pub struct VideoStats {
    pub codec: String,
    /// True once a frame has come out of VideoToolbox.
    pub hardware: bool,
    pub frames_decoded: u64,
    /// Frames decoded but dropped before conversion because they were already late.
    pub frames_skipped: u64,
}

pub struct VideoPlayer {
    rx: Option<Receiver<Msg>>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    playing: bool,
    rate: f64,
    looping: bool,
    clock: f64,
    last_now: Option<f64>,
    pending: Option<(f64, f64, VideoFrame)>,
    end: Option<f64>,
    error: Option<String>,
    duration: Option<f64>,
    size: (u32, u32),
    /// Live input (camera, stream): always show the newest frame, no clock or looping.
    live: bool,
    /// Seek generation: frames decoded before the latest seek carry an older number and are dropped.
    generation: u64,
    mode: PlayMode,
    /// Media time (seconds into the movie) of the frame on screen.
    shown: f64,
}

enum Msg {
    /// (seek generation, presentation time, media time, frame)
    Frame(u64, f64, f64, VideoFrame),
    End(f64),
    Error(String),
}

struct Shared {
    ctl: Mutex<Ctl>,
    cv: Condvar,
    /// Consumer media clock (f64 bits), read by the decoder to skip late frames.
    clock: AtomicU64,
    hardware: AtomicBool,
    decoded: AtomicU64,
    skipped: AtomicU64,
    pool: Mutex<Vec<VideoFrame>>,
    codec: Mutex<String>,
}

struct Ctl {
    looping: bool,
    mode: PlayMode,
    quit: bool,
    /// Jump to this media time (seconds) and tag following frames with `generation`.
    seek: Option<(f64, u64)>,
}

impl Shared {
    fn recycle(&self, frame: VideoFrame) {
        if let Ok(mut pool) = self.pool.lock()
            && pool.len() < POOL
        {
            pool.push(frame);
        }
    }

    fn take(&self, rgba: bool, w: u32, h: u32) -> Option<VideoFrame> {
        let mut pool = self.pool.lock().ok()?;
        let i = pool
            .iter()
            .position(|f| f.is_rgba() == rgba && f.size() == (w, h))?;
        Some(pool.swap_remove(i))
    }
}

impl VideoPlayer {
    /// Opens the file and starts decoding in the background. Errors are human-readable (shown in the UI).
    pub fn open(path: &str) -> Result<Self, String> {
        init();
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let dec = Decoder::open(path).map_err(|e| format!("{name}: {e}"))?;
        Self::start(name, dec, false)
    }

    /// Opens a live input: an FFmpeg input device (`format`, e.g. "avfoundation" for cameras) or,
    /// with an empty format, a network stream URL (rtsp, srt, http…). Each `options` set is tried
    /// in turn until one opens (cameras only accept the frame rates they support).
    pub fn open_live(name: &str, format: &str, url: &str, options: &[&[(&str, &str)]]) -> Result<Self, String> {
        init();
        let mut last = String::from("no options");
        for opts in options.iter().copied().chain(std::iter::once(&[][..])) {
            match Decoder::open_live(format, url, opts) {
                Ok(dec) => return Self::start(name.to_string(), dec, true),
                Err(e) => last = e,
            }
        }
        Err(format!("{name}: {last}"))
    }

    fn start(name: String, dec: Decoder, live: bool) -> Result<Self, String> {
        let size = (dec.dec.width(), dec.dec.height());
        let duration = dec.duration;
        let shared = Arc::new(Shared {
            ctl: Mutex::new(Ctl {
                looping: true,
                mode: PlayMode::default(),
                quit: false,
                seek: None,
            }),
            cv: Condvar::new(),
            clock: AtomicU64::new(0f64.to_bits()),
            hardware: AtomicBool::new(false),
            decoded: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            pool: Mutex::new(Vec::new()),
            codec: Mutex::new(dec.codec.clone()),
        });
        let (tx, rx) = mpsc::sync_channel(QUEUE);
        let sh = shared.clone();
        let tname = name.clone();
        let thread = std::thread::Builder::new()
            .name(format!("video: {name}"))
            .spawn(move || {
                let mut dec = dec;
                if let Err(e) = dec.run(&sh, &tx) {
                    let _ = tx.send(Msg::Error(format!("{tname}: {e}")));
                }
            })
            .map_err(|e| format!("{name}: can't start decoder thread ({e})"))?;
        Ok(Self {
            rx: Some(rx),
            shared,
            thread: Some(thread),
            playing: true,
            rate: 1.0,
            looping: true,
            clock: 0.0,
            last_now: None,
            pending: None,
            end: None,
            error: None,
            duration,
            size,
            live,
            generation: 0,
            mode: PlayMode::default(),
            shown: 0.0,
        })
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    pub fn set_rate(&mut self, rate: f32) {
        self.rate = (rate as f64).clamp(0.1, 4.0);
    }

    pub fn set_looping(&mut self, looping: bool) {
        if self.looping == looping {
            return;
        }
        self.looping = looping;
        if let Ok(mut ctl) = self.shared.ctl.lock() {
            ctl.looping = looping;
        }
        self.shared.cv.notify_all();
    }

    /// Direction and loop region (seconds; None = whole movie). Changing direction restarts
    /// decoding from the frame on screen so it turns around exactly there.
    pub fn set_mode(&mut self, direction: PlayDirection, range: Option<(f64, f64)>) {
        let mode = PlayMode { direction, range };
        if self.live || self.mode == mode {
            return;
        }
        let turned = self.mode.direction != direction;
        self.mode = mode;
        if let Ok(mut ctl) = self.shared.ctl.lock() {
            ctl.mode = mode;
        }
        self.shared.cv.notify_all();
        if turned {
            self.seek(self.shown);
        }
    }

    /// Called once per rendered frame with a monotonic clock in seconds. Returns the newest frame
    /// that is due for display since the last call (None if no new frame). Must be cheap and non-blocking.
    pub fn poll_frame(&mut self, now: f64) -> Option<VideoFrame> {
        if self.live {
            return self.poll_live();
        }
        // Clamp large gaps (window hidden, debugger) so playback resumes instead of skipping ahead.
        let dt = self
            .last_now
            .map_or(0.0, |last| (now - last).clamp(0.0, 0.25));
        self.last_now = Some(now);
        if self.playing {
            self.clock += dt * self.rate;
        }
        if let (Some(end), false) = (self.end, self.looping) {
            self.clock = self.clock.min(end);
        }
        let rx = self.rx.as_ref()?;
        let mut out: Option<VideoFrame> = None;
        loop {
            if self.pending.is_none() {
                match rx.try_recv() {
                    Ok(Msg::Frame(generation, _, _, frame)) if generation != self.generation => {
                        // From before the last seek.
                        self.shared.recycle(frame);
                    }
                    Ok(Msg::Frame(_, pts, media, frame)) => {
                        self.end = None;
                        self.pending = Some((pts, media, frame));
                    }
                    Ok(Msg::End(at)) => {
                        self.end = Some(at);
                        if !self.looping {
                            self.clock = self.clock.min(at);
                        }
                        continue;
                    }
                    Ok(Msg::Error(e)) => {
                        log::warn!("{e}");
                        self.error = Some(e);
                        break;
                    }
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }
            match self.pending.take() {
                Some((pts, media, frame)) if pts <= self.clock + 1e-3 => {
                    self.shown = media;
                    if let Some(old) = out.replace(frame) {
                        self.shared.recycle(old);
                    }
                }
                other => {
                    self.pending = other;
                    break;
                }
            }
        }
        self.shared
            .clock
            .store(self.clock.to_bits(), Ordering::Relaxed);
        out
    }

    /// Live: the newest frame that arrived since the last call (older ones are dropped, so latency
    /// never builds up).
    fn poll_live(&mut self) -> Option<VideoFrame> {
        let rx = self.rx.as_ref()?;
        let mut out: Option<VideoFrame> = None;
        for msg in rx.try_iter() {
            match msg {
                Msg::Frame(_, _, _, frame) => {
                    if let Some(old) = out.replace(frame) {
                        self.shared.recycle(old);
                    }
                }
                Msg::End(_) => {}
                Msg::Error(e) => {
                    log::warn!("{e}");
                    self.error = Some(e);
                }
            }
        }
        out
    }

    /// Jumps to `t` seconds into the clip. The next frames shown are from there; anything decoded
    /// before the jump is dropped.
    pub fn seek(&mut self, t: f64) {
        if self.live {
            return;
        }
        let t = match self.duration {
            Some(d) if d > 0.0 => t.clamp(0.0, (d - 1e-3).max(0.0)),
            _ => t.max(0.0),
        };
        self.generation += 1;
        if let Ok(mut ctl) = self.shared.ctl.lock() {
            ctl.seek = Some((t, self.generation));
        }
        self.shared.cv.notify_all();
        self.clock = t;
        self.shown = t;
        self.end = None;
        if let Some((_, _, f)) = self.pending.take() {
            self.shared.recycle(f);
        }
        // Unblock the decoder if it's waiting on a full queue.
        if let Some(rx) = &self.rx {
            for msg in rx.try_iter() {
                if let Msg::Frame(_, _, _, f) = msg {
                    self.shared.recycle(f);
                }
            }
        }
        self.shared.clock.store(self.clock.to_bits(), Ordering::Relaxed);
    }

    /// Where the clip is (seconds from its start), for the scrub bar and crash recovery.
    pub fn position(&self) -> f64 {
        self.shown
    }

    /// Returns a frame's buffers for reuse by the decoder (optional; avoids reallocations).
    pub fn recycle(&self, frame: VideoFrame) {
        self.shared.recycle(frame);
    }

    pub fn duration(&self) -> Option<f64> {
        self.duration
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Decode error that stopped playback, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn stats(&self) -> VideoStats {
        VideoStats {
            codec: self
                .shared
                .codec
                .lock()
                .map(|c| c.clone())
                .unwrap_or_default(),
            hardware: self.shared.hardware.load(Ordering::Relaxed),
            frames_decoded: self.shared.decoded.load(Ordering::Relaxed),
            frames_skipped: self.shared.skipped.load(Ordering::Relaxed),
        }
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        if let Ok(mut ctl) = self.shared.ctl.lock() {
            ctl.quit = true;
        }
        self.shared.cv.notify_all();
        // Dropping the receiver unblocks a decoder waiting on a full queue.
        self.rx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = ff::init();
        unsafe { ffi::av_log_set_level(ffi::AV_LOG_ERROR) };
    });
}

fn err(e: ff::Error) -> String {
    e.to_string()
}

mod convert;
mod decoder;
#[cfg(test)]
mod tests;

use convert::{Scaler, fmt_flags};
use decoder::Decoder;
