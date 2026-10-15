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

/// Decoder state owned by the background thread.
struct Decoder {
    path: String,
    input: ff::format::context::Input,
    stream: usize,
    tb: f64,
    start_ts: i64,
    default_dur: f64,
    duration: Option<f64>,
    codec: String,
    dec: ff::decoder::Video,
    /// VideoToolbox was attached; cleared if it fails before the first frame.
    hw: bool,
    frame: ff::frame::Video,
    mapped: ff::frame::Video,
    scaler: Scaler,
    eof_sent: bool,
    /// First pts of the file, subtracted so playback starts at 0.
    origin: Option<i64>,
    last_rel: f64,
    /// Longest media time seen (the movie's length when the container doesn't say).
    loop_len: f64,
    /// Presentation time of the next frame sent. Grows by each frame's duration whichever way the
    /// movie plays, so loops, reverse and ping-pong all look like one continuous stream to the player.
    out_clock: Option<f64>,
    /// Times the movie has looped or turned around (0 = first pass).
    passes: u32,
    /// Media time of the last frame sent.
    last_sent: f64,
    /// Playing backwards: frames before `rev_end` are decoded a chunk at a time into `rev_buf`
    /// and sent from the back.
    reversing: bool,
    rev_end: f64,
    rev_buf: Vec<(f64, f64, VideoFrame)>,
    frames_in_loop: u64,
    skips: u32,
    errors: u32,
    live: bool,
    /// Tag for frames sent (the seek they belong to).
    generation: u64,
    /// After a seek: decode and drop frames until this media time.
    seek_target: Option<f64>,
    /// Live pacing: (first pts, when it arrived). Inputs that run faster than real time (test
    /// sources, recorded streams) are slowed to their timestamps; cameras already are.
    live_start: Option<(f64, std::time::Instant)>,
}

impl Decoder {
    fn open(path: &str) -> Result<Self, String> {
        let input = ff::format::input(&path).map_err(|e| format!("can't open ({e})"))?;
        Self::from_input(path, input, false)
    }

    fn open_live(format: &str, url: &str, options: &[(&str, &str)]) -> Result<Self, String> {
        static DEVICES: Once = Once::new();
        DEVICES.call_once(ff::device::register_all);
        let mut dict = ff::Dictionary::new();
        for (k, v) in options {
            dict.set(k, v);
        }
        let ctx = if format.is_empty() {
            ff::format::input_with_dictionary(&url, dict).map_err(|e| format!("can't open ({e})"))?
        } else {
            let fmt = ff::device::input::video()
                .find(|f| match f {
                    ff::Format::Input(i) => i.name() == format,
                    _ => false,
                })
                .ok_or_else(|| format!("input type \"{format}\" isn't available"))?;
            match ff::format::open_with(&url, &fmt, dict).map_err(|e| format!("can't open ({e})"))? {
                ff::format::context::Context::Input(i) => i,
                _ => return Err("not an input".into()),
            }
        };
        Self::from_input(url, ctx, true)
    }

    fn from_input(path: &str, input: ff::format::context::Input, live: bool) -> Result<Self, String> {
        let st = input
            .streams()
            .best(ff::media::Type::Video)
            .ok_or("no video stream")?;
        let stream = st.index();
        let tb = f64::from(st.time_base());
        let tb = if tb > 0.0 { tb } else { 1.0 / 90_000.0 };
        let fps = f64::from(st.avg_frame_rate());
        let default_dur = if fps > 0.0 && fps < 1000.0 {
            1.0 / fps
        } else {
            1.0 / 30.0
        };
        let duration = if input.duration() > 0 {
            Some(input.duration() as f64 / f64::from(ff::ffi::AV_TIME_BASE))
        } else if st.duration() > 0 {
            Some(st.duration() as f64 * tb)
        } else {
            None
        };
        let start_ts = if st.start_time() != NOPTS {
            st.start_time()
        } else {
            0
        };
        let (dec, codec, hw) = open_decoder(&input, stream, true)?;
        Ok(Self {
            path: path.to_string(),
            input,
            stream,
            tb,
            start_ts,
            default_dur,
            duration,
            codec,
            dec,
            hw,
            frame: ff::frame::Video::empty(),
            mapped: ff::frame::Video::empty(),
            scaler: Scaler::default(),
            eof_sent: false,
            origin: None,
            last_rel: -1.0,
            loop_len: 0.0,
            out_clock: None,
            passes: 0,
            last_sent: 0.0,
            reversing: false,
            rev_end: 0.0,
            rev_buf: Vec::new(),
            frames_in_loop: 0,
            skips: 0,
            errors: 0,
            live,
            live_start: None,
            generation: 0,
            seek_target: None,
        })
    }

    fn run(&mut self, sh: &Shared, tx: &SyncSender<Msg>) -> Result<(), String> {
        loop {
            let (seek, mode) = match sh.ctl.lock() {
                Ok(mut c) if !c.quit => (c.seek.take(), c.mode),
                _ => return Ok(()),
            };
            if let Some((t, generation)) = seek {
                self.generation = generation;
                self.out_clock = None;
                self.drop_reverse(sh);
                self.reversing = mode.direction == PlayDirection::Reverse && !self.live;
                if self.reversing {
                    // Include the frame on screen at `t`.
                    self.rev_end = t + 1e-4;
                } else {
                    self.seek_to(t)?;
                }
            }
            let (lo, hi) = self.bounds(mode);
            if self.reversing {
                if !self.reverse_step(sh, tx, mode, lo, hi)? {
                    return Ok(());
                }
                continue;
            }
            let more = match self.next_frame() {
                Ok(more) => more,
                Err(e) if self.hw && self.origin.is_none() => {
                    self.fallback_to_software(&e, sh)?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            if !more && self.live {
                return Err("the input stopped (unplugged or closed?)".into());
            }
            if !more {
                if self.frames_in_loop == 0 && self.hw && self.origin.is_none() {
                    self.fallback_to_software("no frames", sh)?;
                    continue;
                }
                if self.frames_in_loop == 0 {
                    return Err("no decodable video frames".into());
                }
                // Single-frame files (stills) never need re-decoding.
                let still = self.frames_in_loop == 1 && self.passes == 0 && mode == PlayMode::default();
                if !self.at_end(sh, tx, mode, lo, hi, still)? {
                    return Ok(());
                }
                continue;
            }
            sh.decoded.fetch_add(1, Ordering::Relaxed);
            let (rel, dur) = self.timing();
            // After a seek, frames before the target only lead up to it.
            if let Some(target) = self.seek_target {
                // Drop frames whose display time ends at or before the target (the one shown at
                // `target` is the one whose interval contains it).
                if rel + dur <= target + 1e-6 {
                    continue;
                }
                self.seek_target = None;
            }
            if !self.live && mode.range.is_some() {
                if rel >= hi - 1e-6 {
                    if !self.at_end(sh, tx, mode, lo, hi, false)? {
                        return Ok(());
                    }
                    continue;
                }
                if rel + dur <= lo + 1e-6 {
                    // Before the in point (it moved, or a seek landed there).
                    self.jump(lo)?;
                    continue;
                }
            }
            let pts = self.next_pts(rel, dur);
            if self.late(sh, pts, dur) {
                continue;
            }
            if self.live {
                let (rel0, t0) = *self.live_start.get_or_insert((rel, std::time::Instant::now()));
                let ahead = (rel - rel0) - t0.elapsed().as_secs_f64();
                if ahead > 0.002 {
                    std::thread::sleep(std::time::Duration::from_secs_f64(ahead.min(0.25)));
                } else if ahead < -1.0 {
                    // Fell far behind (stall, clock jump): re-anchor instead of racing to catch up.
                    self.live_start = Some((rel, std::time::Instant::now()));
                }
            }
            let src = self.download(sh)?;
            let out = self.scaler.convert(src, sh)?;
            self.last_sent = rel;
            if tx.send(Msg::Frame(self.generation, pts, rel, out)).is_err() {
                return Ok(());
            }
        }
    }

    /// Loop region in seconds (the whole movie when none is set).
    fn bounds(&self, mode: PlayMode) -> (f64, f64) {
        let len = match self.duration {
            Some(d) if d > 0.0 => d,
            _ if self.loop_len > 0.0 => self.loop_len,
            _ => f64::INFINITY,
        };
        match mode.range {
            Some((a, b)) => (a.clamp(0.0, len), b.min(len)),
            None => (0.0, len),
        }
    }

    fn next_pts(&mut self, rel: f64, dur: f64) -> f64 {
        let pts = *self.out_clock.get_or_insert(rel);
        self.out_clock = Some(pts + dur);
        pts
    }

    /// True if a frame at `pts` is already too late to show (skipped, but never too many in a row).
    fn late(&mut self, sh: &Shared, pts: f64, dur: f64) -> bool {
        let clock = f64::from_bits(sh.clock.load(Ordering::Relaxed));
        if !self.live && pts + dur < clock - LATE && self.skips < MAX_SKIP {
            self.skips += 1;
            sh.skipped.fetch_add(1, Ordering::Relaxed);
            return true;
        }
        self.skips = 0;
        false
    }

    /// Reached the end of the loop region (or of the movie) going either way: loop, turn around,
    /// or stop until looping is switched back on. Returns false when the player is gone.
    fn at_end(&mut self, sh: &Shared, tx: &SyncSender<Msg>, mode: PlayMode, lo: f64, hi: f64, still: bool) -> Result<bool, String> {
        if still || !sh.ctl.lock().is_ok_and(|c| c.looping) {
            if tx.send(Msg::End(self.out_clock.unwrap_or(0.0))).is_err() {
                return Ok(false);
            }
            let Ok(mut ctl) = sh.ctl.lock() else {
                return Ok(false);
            };
            while !ctl.quit && ctl.seek.is_none() && (still || !ctl.looping) {
                ctl = match sh.cv.wait(ctl) {
                    Ok(c) => c,
                    Err(_) => return Ok(false),
                };
            }
            if ctl.quit {
                return Ok(false);
            }
            if ctl.seek.is_some() {
                // Handled at the top of the loop.
                return Ok(true);
            }
        }
        self.passes += 1;
        match (self.reversing, mode.direction) {
            (false, PlayDirection::PingPong) => {
                self.reversing = true;
                self.rev_end = self.last_sent;
            }
            (false, _) => self.jump(lo)?,
            (true, PlayDirection::PingPong) => {
                self.reversing = false;
                self.jump(lo)?;
                // Don't show the turning frame twice.
                self.seek_target = Some(self.last_sent);
            }
            (true, _) => self.rev_end = hi,
        }
        Ok(true)
    }

    /// Continues forward from `t` (loop jump: the presentation clock keeps running).
    fn jump(&mut self, t: f64) -> Result<(), String> {
        if t <= 1e-6 { self.rewind() } else { self.seek_to(t) }
    }

    fn drop_reverse(&mut self, sh: &Shared) {
        for (_, _, f) in self.rev_buf.drain(..) {
            sh.recycle(f);
        }
    }

    /// One step of reverse play: send the next buffered frame, or decode the chunk before the
    /// last one. Returns false when the player is gone.
    fn reverse_step(&mut self, sh: &Shared, tx: &SyncSender<Msg>, mode: PlayMode, lo: f64, hi: f64) -> Result<bool, String> {
        if let Some((rel, dur, frame)) = self.rev_buf.pop() {
            let pts = self.next_pts(rel, dur);
            if self.late(sh, pts, dur) {
                sh.recycle(frame);
                return Ok(true);
            }
            self.last_sent = rel;
            return Ok(tx.send(Msg::Frame(self.generation, pts, rel, frame)).is_ok());
        }
        if !hi.is_finite() {
            // Unknown length: can't start from the end.
            self.reversing = false;
            return Ok(true);
        }
        self.rev_end = self.rev_end.min(hi);
        if self.rev_end <= lo + 1e-6 {
            return self.at_end(sh, tx, mode, lo, hi, false);
        }
        // Decode from the keyframe before the chunk; keep (convert) only the chunk's frames.
        let t0 = (self.rev_end - REVERSE_CHUNK_FRAMES * self.default_dur).max(lo);
        self.seek_to(t0)?;
        self.seek_target = None;
        loop {
            if !self.next_frame()? {
                break;
            }
            sh.decoded.fetch_add(1, Ordering::Relaxed);
            let (rel, dur) = self.timing();
            if rel >= self.rev_end - 1e-6 {
                break;
            }
            if rel + dur <= t0 + 1e-6 {
                continue;
            }
            let src = self.download(sh)?;
            let out = self.scaler.convert(src, sh)?;
            self.rev_buf.push((rel, dur, out));
        }
        self.rev_end = self.rev_buf.first().map_or(t0, |f| f.0);
        Ok(true)
    }

    /// Decodes the next frame into `self.frame`. Returns false at end of stream.
    fn next_frame(&mut self) -> Result<bool, String> {
        loop {
            match self.dec.receive_frame(&mut self.frame) {
                Ok(()) => {
                    self.errors = 0;
                    return Ok(true);
                }
                Err(ff::Error::Eof) => return Ok(false),
                Err(ff::Error::Other { errno }) if errno == libc::EAGAIN => {}
                // Tolerate isolated corrupt frames; give up on a stream that never recovers.
                Err(e) if (self.hw && self.origin.is_none()) || self.errors > 100 => {
                    return Err(format!("decode failed ({e})"));
                }
                Err(e) => {
                    self.errors += 1;
                    log::debug!("{}: decode error ({e})", self.path);
                }
            }
            if self.eof_sent {
                return Ok(false);
            }
            loop {
                let mut pkt = ff::Packet::empty();
                match pkt.read(&mut self.input) {
                    Ok(()) if pkt.stream() == self.stream => {
                        if let Err(e) = self.dec.send_packet(&pkt) {
                            if self.hw && self.origin.is_none() {
                                return Err(format!("decode failed ({e})"));
                            }
                            log::debug!("{}: skipping bad packet ({e})", self.path);
                            continue;
                        }
                        break;
                    }
                    Ok(()) => {}
                    Err(ff::Error::Other { errno }) if errno == libc::EAGAIN => {}
                    Err(_) => {
                        let _ = self.dec.send_eof();
                        self.eof_sent = true;
                        break;
                    }
                }
            }
        }
    }

    /// Frame time relative to the loop start, and frame duration, in seconds.
    fn timing(&mut self) -> (f64, f64) {
        let f = unsafe { &*self.frame.as_ptr() };
        let ts = if f.best_effort_timestamp != NOPTS {
            f.best_effort_timestamp
        } else {
            f.pts
        };
        let dur = if f.duration > 0 {
            f.duration as f64 * self.tb
        } else {
            self.default_dur
        };
        let mut rel = if ts == NOPTS {
            self.last_rel + dur
        } else {
            let origin = *self.origin.get_or_insert(ts);
            (ts - origin) as f64 * self.tb
        };
        // Keep time strictly increasing within a loop even with broken timestamps.
        if rel <= self.last_rel {
            rel = self.last_rel + 1e-4;
        }
        rel = rel.max(0.0);
        self.last_rel = rel;
        self.loop_len = self.loop_len.max(rel + dur);
        self.frames_in_loop += 1;
        (rel, dur)
    }

    fn fallback_to_software(&mut self, why: &str, sh: &Shared) -> Result<(), String> {
        log::info!(
            "{}: hardware decode failed ({why}), using software",
            self.path
        );
        (self.dec, self.codec, self.hw) = open_decoder(&self.input, self.stream, false)?;
        if let Ok(mut c) = sh.codec.lock() {
            c.clone_from(&self.codec);
        }
        self.rewind()
    }

    /// Jumps to `t` seconds (the keyframe before it; `seek_target` drops frames up to `t`).
    fn seek_to(&mut self, t: f64) -> Result<(), String> {
        let ts = self.start_ts + (t / self.tb) as i64;
        // SAFETY: seeking the demuxer we own, on this thread.
        let ok = unsafe { ffi::av_seek_frame(self.input.as_mut_ptr(), self.stream as i32, ts, ffi::AVSEEK_FLAG_BACKWARD) >= 0 };
        if !ok {
            self.input = ff::format::input(&self.path).map_err(|e| format!("can't reopen ({e})"))?;
        }
        self.dec.flush();
        self.eof_sent = false;
        self.last_rel = -1.0;
        self.frames_in_loop = 0;
        self.seek_target = Some(t);
        Ok(())
    }

    fn rewind(&mut self) -> Result<(), String> {
        let ok = unsafe {
            ffi::av_seek_frame(
                self.input.as_mut_ptr(),
                self.stream as i32,
                self.start_ts,
                ffi::AVSEEK_FLAG_BACKWARD,
            ) >= 0
        };
        if !ok {
            self.input =
                ff::format::input(&self.path).map_err(|e| format!("can't reopen ({e})"))?;
        }
        self.dec.flush();
        self.eof_sent = false;
        self.last_rel = -1.0;
        self.loop_len = self.loop_len.max(1e-3);
        self.frames_in_loop = 0;
        Ok(())
    }

    /// Returns a CPU-accessible frame, mapping VideoToolbox surfaces when needed.
    fn download(&mut self, sh: &Shared) -> Result<*const ffi::AVFrame, String> {
        unsafe {
            let f = self.frame.as_ptr();
            if (*f).format != Px::AV_PIX_FMT_VIDEOTOOLBOX as i32 {
                return Ok(f);
            }
            sh.hardware.store(true, Ordering::Relaxed);
            let m = self.mapped.as_mut_ptr();
            ffi::av_frame_unref(m);
            if ffi::av_hwframe_map(m, f, ffi::AV_HWFRAME_MAP_READ as i32) < 0 {
                ffi::av_frame_unref(m);
                let r = ffi::av_hwframe_transfer_data(m, f, 0);
                if r < 0 {
                    return Err(format!(
                        "hardware frame download failed ({})",
                        ff::Error::from(r)
                    ));
                }
            }
            (*m).color_range = (*f).color_range;
            (*m).colorspace = (*f).colorspace;
            Ok(m)
        }
    }
}

/// Opens a decoder for `stream`, attaching VideoToolbox when `try_hw` and the codec supports it.
fn open_decoder(
    input: &ff::format::context::Input,
    stream: usize,
    try_hw: bool,
) -> Result<(ff::decoder::Video, String, bool), String> {
    use ff::codec::Id;
    let st = input.stream(stream).ok_or("bad stream")?;
    let alpha_mode = st.metadata().get("alpha_mode") == Some("1");
    let params = st.parameters();
    let id = params.id();
    let src_fmt = unsafe { (*params.as_ptr()).format };
    let has_alpha = alpha_mode || fmt_flags(src_fmt) & ffi::AV_PIX_FMT_FLAG_ALPHA as u64 != 0;
    // Codecs VideoToolbox handles on Apple Silicon; others (MPEG-4 Part 2...) only log init failures.
    let try_hw =
        try_hw && !has_alpha && matches!(id, Id::H264 | Id::HEVC | Id::PRORES | Id::VP9 | Id::AV1);

    let codec = match id {
        // libavcodec's native VP8/VP9 decoders drop the alpha side channel; libvpx keeps it.
        Id::VP9 if alpha_mode => ff::decoder::find_by_name("libvpx-vp9"),
        Id::VP8 if alpha_mode => ff::decoder::find_by_name("libvpx"),
        // The native AV1 decoder is hwaccel-only; libdav1d (the default) is the software path.
        Id::AV1 if try_hw => ff::decoder::find_by_name("av1"),
        _ => None,
    }
    .or_else(|| ff::decoder::find(id))
    .ok_or_else(|| format!("unsupported codec ({id:?})"))?;

    let mut ctx = ff::codec::Context::from_parameters(params).map_err(err)?;
    let hw = try_hw && unsafe { attach_videotoolbox(ctx.as_mut_ptr(), codec.as_ptr()) };
    unsafe { (*ctx.as_mut_ptr()).pkt_timebase = (*st.as_ptr()).time_base };
    if !hw {
        let threads = std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .min(8);
        ctx.set_threading(ff::codec::threading::Config {
            kind: ff::codec::threading::Type::Frame,
            count: threads,
        });
    }
    let dec = ctx
        .decoder()
        .open_as(codec)
        .and_then(|o| o.video())
        .map_err(|e| format!("can't open {} decoder ({e})", codec.name()))?;
    if dec.width() == 0 || dec.height() == 0 {
        return Err("video has no size".into());
    }
    Ok((dec, codec.name().to_string(), hw))
}

/// Enables VideoToolbox on `ctx` if the decoder supports it. FFmpeg falls back to software
/// automatically if the hwaccel can't handle a particular stream.
unsafe fn attach_videotoolbox(ctx: *mut ffi::AVCodecContext, codec: *const ffi::AVCodec) -> bool {
    unsafe {
        let mut supported = false;
        for i in 0.. {
            let cfg = ffi::avcodec_get_hw_config(codec, i);
            if cfg.is_null() {
                break;
            }
            if (*cfg).device_type == ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX
                && (*cfg).methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32 != 0
            {
                supported = true;
                break;
            }
        }
        if !supported {
            return false;
        }
        let mut dev: *mut ffi::AVBufferRef = ptr::null_mut();
        let r = ffi::av_hwdevice_ctx_create(
            &mut dev,
            ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            ptr::null(),
            ptr::null_mut(),
            0,
        );
        if r < 0 || dev.is_null() {
            return false;
        }
        (*ctx).hw_device_ctx = dev;
        (*ctx).get_format = Some(pick_videotoolbox);
        true
    }
}

unsafe extern "C" fn pick_videotoolbox(ctx: *mut ffi::AVCodecContext, fmts: *const Px) -> Px {
    unsafe {
        let mut p = fmts;
        while *p != Px::AV_PIX_FMT_NONE {
            if *p == Px::AV_PIX_FMT_VIDEOTOOLBOX {
                return *p;
            }
            p = p.add(1);
        }
        ffi::avcodec_default_get_format(ctx, fmts)
    }
}

fn fmt_flags(fmt: i32) -> u64 {
    if fmt < 0 {
        return 0;
    }
    // SAFETY: `fmt` came from FFmpeg, so it is a valid AVPixelFormat discriminant.
    let desc = unsafe { ffi::av_pix_fmt_desc_get(std::mem::transmute::<i32, Px>(fmt)) };
    if desc.is_null() {
        0
    } else {
        unsafe { (*desc).flags }
    }
}

/// Cached swscale context plus the direct NV12 copy path.
#[derive(Default)]
struct Scaler {
    ctx: Option<ptr::NonNull<ffi::SwsContext>>,
    key: (i32, i32, i32, bool, i32, i32),
}

unsafe impl Send for Scaler {}

impl Drop for Scaler {
    fn drop(&mut self) {
        if let Some(c) = self.ctx.take() {
            unsafe { ffi::sws_freeContext(c.as_ptr()) };
        }
    }
}

impl Scaler {
    fn convert(&mut self, f: *const ffi::AVFrame, sh: &Shared) -> Result<VideoFrame, String> {
        let f = unsafe { &*f };
        let (w, h) = (f.width.max(0) as u32, f.height.max(0) as u32);
        if w == 0 || h == 0 {
            return Err("decoded frame has no size".into());
        }
        let flags = fmt_flags(f.format);
        let rgba = f.format == Px::AV_PIX_FMT_PAL8 as i32
            || flags
                & (ffi::AV_PIX_FMT_FLAG_ALPHA | ffi::AV_PIX_FMT_FLAG_PAL | ffi::AV_PIX_FMT_FLAG_RGB)
                    as u64
                != 0;
        let full_range = f.color_range == ffi::AVColorRange::AVCOL_RANGE_JPEG;
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));

        let mut out = sh.take(rgba, w, h).unwrap_or_else(|| {
            if rgba {
                VideoFrame::Rgba {
                    width: w,
                    height: h,
                    data: vec![0; (w * h * 4) as usize],
                }
            } else {
                VideoFrame::Nv12 {
                    width: w,
                    height: h,
                    luma: vec![0; (w * h) as usize],
                    luma_stride: w,
                    chroma: vec![0; (cw * 2 * ch) as usize],
                    chroma_stride: cw * 2,
                }
            }
        });

        // NV12 straight from the decoder (VideoToolbox): plain row copy, no swscale.
        if !rgba
            && !full_range
            && f.format == Px::AV_PIX_FMT_NV12 as i32
            && let VideoFrame::Nv12 { luma, chroma, .. } = &mut out
            && copy_plane(luma, w as usize, f.data[0], f.linesize[0], h as usize)
            && copy_plane(
                chroma,
                (cw * 2) as usize,
                f.data[1],
                f.linesize[1],
                ch as usize,
            )
        {
            return Ok(out);
        }

        let dst_fmt = if rgba {
            Px::AV_PIX_FMT_RGBA
        } else {
            Px::AV_PIX_FMT_NV12
        };
        let space = f.colorspace as i32;
        let key = (
            f.format,
            f.width,
            f.height,
            full_range,
            space,
            dst_fmt as i32,
        );
        if self.ctx.is_none() || self.key != key {
            if let Some(c) = self.ctx.take() {
                unsafe { ffi::sws_freeContext(c.as_ptr()) };
            }
            let c = unsafe {
                ffi::sws_getContext(
                    f.width,
                    f.height,
                    std::mem::transmute::<i32, Px>(f.format),
                    f.width,
                    f.height,
                    dst_fmt,
                    ffi::SwsFlags::SWS_BILINEAR as i32,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null(),
                )
            };
            let c = ptr::NonNull::new(c).ok_or("unsupported pixel format")?;
            let cs = match f.colorspace {
                ffi::AVColorSpace::AVCOL_SPC_BT709 => ffi::SWS_CS_ITU709,
                ffi::AVColorSpace::AVCOL_SPC_BT2020_NCL
                | ffi::AVColorSpace::AVCOL_SPC_BT2020_CL => ffi::SWS_CS_BT2020,
                ffi::AVColorSpace::AVCOL_SPC_BT470BG | ffi::AVColorSpace::AVCOL_SPC_SMPTE170M => {
                    ffi::SWS_CS_ITU601
                }
                _ if f.height >= 720 => ffi::SWS_CS_ITU709,
                _ => ffi::SWS_CS_ITU601,
            };
            unsafe {
                let coeffs = ffi::sws_getCoefficients(cs);
                ffi::sws_setColorspaceDetails(
                    c.as_ptr(),
                    coeffs,
                    full_range as i32,
                    coeffs,
                    rgba as i32,
                    0,
                    1 << 16,
                    1 << 16,
                );
            }
            self.ctx = Some(c);
            self.key = key;
        }
        let Some(c) = self.ctx else {
            return Err("no scaler".into());
        };
        let (dst, stride): ([*mut u8; 4], [i32; 4]) = match &mut out {
            VideoFrame::Rgba { data, .. } => (
                [
                    data.as_mut_ptr(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                ],
                [(w * 4) as i32, 0, 0, 0],
            ),
            VideoFrame::Nv12 { luma, chroma, .. } => (
                [
                    luma.as_mut_ptr(),
                    chroma.as_mut_ptr(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                ],
                [w as i32, (cw * 2) as i32, 0, 0],
            ),
        };
        let r = unsafe {
            ffi::sws_scale(
                c.as_ptr(),
                f.data.as_ptr() as *const *const u8,
                f.linesize.as_ptr(),
                0,
                f.height,
                dst.as_ptr(),
                stride.as_ptr(),
            )
        };
        if r <= 0 {
            return Err("pixel format conversion failed".into());
        }
        Ok(out)
    }
}

/// Copies `rows` rows of `width` bytes from a strided FFmpeg plane into a tightly packed buffer.
fn copy_plane(dst: &mut [u8], width: usize, src: *const u8, linesize: i32, rows: usize) -> bool {
    if src.is_null() || linesize < width as i32 || dst.len() < width * rows {
        return false;
    }
    for y in 0..rows {
        // SAFETY: FFmpeg planes hold `rows` lines of `linesize` bytes, and linesize >= width.
        let line = unsafe { std::slice::from_raw_parts(src.add(y * linesize as usize), width) };
        dst[y * width..(y + 1) * width].copy_from_slice(line);
    }
    true
}

#[cfg(test)]
mod seek_tests {
    use super::{VideoFrame, VideoPlayer};

    fn luma(f: &VideoFrame) -> Vec<u8> {
        match f {
            VideoFrame::Nv12 { luma, .. } => luma.clone(),
            VideoFrame::Rgba { data, .. } => data.clone(),
        }
    }

    /// Plays `p` on a fake clock from `from` to `to` seconds and returns the last frame shown.
    fn play(p: &mut VideoPlayer, from: f64, to: f64) -> Option<Vec<u8>> {
        let mut last = None;
        let mut t = from;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while t <= to + 1e-9 && std::time::Instant::now() < deadline {
            // Give the decoder a moment, then advance the clock a frame.
            for _ in 0..200 {
                if let Some(f) = p.poll_frame(t) {
                    last = Some(luma(&f));
                    p.recycle(f);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            t += 1.0 / 30.0;
        }
        last
    }

    /// Jumping to 1.5 s shows the same picture as playing up to 1.5 s (the clip is 2.5 s).
    #[test]
    fn seek_lands_on_the_right_frame() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../media/test/h264.mp4");
        if !std::path::Path::new(path).exists() {
            return;
        }
        let mut played = VideoPlayer::open(path).unwrap();
        let reference = play(&mut played, 0.0, 1.5).expect("frames");
        let mut jumped = VideoPlayer::open(path).unwrap();
        jumped.poll_frame(0.0);
        jumped.seek(1.5);
        assert!((jumped.position() - 1.5).abs() < 1e-6);
        let landed = play(&mut jumped, 0.0, 0.0).expect("a frame after seeking");
        let diff: u64 = reference.iter().zip(&landed).map(|(a, b)| a.abs_diff(*b) as u64).sum::<u64>() / reference.len().max(1) as u64;
        assert!(diff <= 1, "picture after seek differs from playing to 1.5 s (mean diff {diff})");
    }

    /// Media times of the frames shown while playing `secs` on a fake clock.
    fn positions(p: &mut VideoPlayer, secs: f64) -> Vec<f64> {
        let mut out = Vec::new();
        let mut t = 0.0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while t <= secs && std::time::Instant::now() < deadline {
            for _ in 0..300 {
                if let Some(f) = p.poll_frame(t) {
                    out.push(p.position());
                    p.recycle(f);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            t += 1.0 / 30.0;
        }
        out
    }

    fn clip() -> Option<&'static str> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../media/test/h264.mp4");
        std::path::Path::new(path).exists().then_some(path)
    }

    /// Reverse play shows earlier and earlier frames, wrapping from the start to the end.
    #[test]
    fn reverse_plays_backwards_and_loops() {
        let Some(path) = clip() else { return };
        let mut p = VideoPlayer::open(path).unwrap();
        p.poll_frame(0.0);
        p.set_mode(super::PlayDirection::Reverse, None);
        let pos = positions(&mut p, 4.0);
        assert!(pos.len() > 60, "too few frames ({})", pos.len());
        let down = pos.windows(2).filter(|w| w[1] < w[0]).count();
        let wraps = pos.windows(2).filter(|w| w[1] > w[0] + 1.0).count();
        assert!(down >= pos.len() * 9 / 10 - 2, "not playing backwards: {pos:?}");
        assert!(wraps >= 1, "never wrapped to the end: {pos:?}");
    }

    /// A loop region keeps playback inside it; ping-pong turns around at both ends.
    #[test]
    fn loop_region_and_ping_pong() {
        let Some(path) = clip() else { return };
        let mut p = VideoPlayer::open(path).unwrap();
        p.set_mode(super::PlayDirection::Forward, Some((0.5, 1.0)));
        let pos = positions(&mut p, 2.0);
        let late = &pos[pos.len() / 4..];
        assert!(late.iter().all(|t| (0.45..1.0).contains(t)), "left the loop region: {pos:?}");
        let mut q = VideoPlayer::open(path).unwrap();
        q.set_mode(super::PlayDirection::PingPong, Some((0.5, 1.0)));
        let pos = positions(&mut q, 3.0);
        let late = &pos[pos.len() / 6..];
        assert!(late.iter().all(|t| (0.45..1.0).contains(t)), "left the loop region: {pos:?}");
        let turns = late.windows(3).filter(|w| (w[1] - w[0]).signum() != (w[2] - w[1]).signum()).count();
        assert!(turns >= 2, "never turned around: {pos:?}");
        let jumps = late.windows(2).filter(|w| (w[1] - w[0]).abs() > 0.2).count();
        assert_eq!(jumps, 0, "ping-pong jumped instead of turning: {pos:?}");
    }
}
