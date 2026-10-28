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
/// Frames of the next reverse group decoded per frame shown (keeps ahead of keyframe groups up to ~4x the
/// kept window).
const REVERSE_DECODE_AHEAD: usize = 4;

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
    /// A hardware-decoded frame still in the decoder's GPU-shareable memory (macOS zero-copy).
    #[cfg(target_os = "macos")]
    Surface(surface::PixelBuffer),
    /// GPU blocks straight from a Hap movie (DXT1/DXT5 rows of 4×4 blocks); uploaded without decoding.
    Compressed {
        width: u32,
        height: u32,
        format: phosphor_engine::CompressedFormat,
        data: Vec<u8>,
        /// Hap Q Alpha: RGTC1 (BC4) blocks of the alpha plane.
        alpha: Option<Vec<u8>>,
    },
}

impl VideoFrame {
    /// A second handle to this frame when that's cheap (a GPU surface or Hap blocks), for keeping the end of a
    /// forward pass to play backwards from; None for full pictures (they'd cost a copy each).
    pub(crate) fn cheap_clone(&self) -> Option<VideoFrame> {
        match self {
            #[cfg(target_os = "macos")]
            VideoFrame::Surface(pb) => Some(VideoFrame::Surface(pb.clone())),
            VideoFrame::Compressed { width, height, format, data, alpha } => {
                Some(VideoFrame::Compressed { width: *width, height: *height, format: *format, data: data.clone(), alpha: alpha.clone() })
            }
            _ => None,
        }
    }
}

/// The GPU can take Hap's blocks directly (set once the GPU is known; off = FFmpeg decodes Hap on the CPU).
static GPU_HAP: AtomicBool = AtomicBool::new(false);

/// Hardware-decoded frames are shown straight from the decoder's memory (macOS, Metal).
static ZERO_COPY: AtomicBool = AtomicBool::new(false);

pub fn set_zero_copy(on: bool) {
    ZERO_COPY.store(on, Ordering::Relaxed);
}

fn zero_copy() -> bool {
    ZERO_COPY.load(Ordering::Relaxed)
}

pub fn set_gpu_hap(on: bool) {
    GPU_HAP.store(on, Ordering::Relaxed);
}

fn gpu_hap() -> bool {
    GPU_HAP.load(Ordering::Relaxed)
}

impl VideoFrame {
    pub fn size(&self) -> (u32, u32) {
        match self {
            Self::Nv12 { width, height, .. } | Self::Rgba { width, height, .. } | Self::Compressed { width, height, .. } => {
                (*width, *height)
            }
            #[cfg(target_os = "macos")]
            Self::Surface(pb) => pb.size(),
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
    pub frames_shown: u64,
    /// Backwards play has had to wait for decoding (few keyframes): converting the file makes it smooth.
    pub reverse_slow: bool,
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
    /// Media time (seconds into the movie) of the frame on screen, and its presentation time.
    shown: f64,
    shown_pts: f64,
    /// Times the movie reached its end (or its out point) and went round again; +1 when it stops at the end.
    passes: u64,
    /// Random frames: when the next jump is due, and whether the last jump's frame is still on its way.
    jumps: Jumps,
    /// What the file is (frame rate, bit rate, format, colour space, sound), from when it opened.
    about: String,
    /// Chapters stored in the file (start in seconds, title).
    chapters: Vec<(f64, String)>,
    /// The movie's sound (None = no audio track, or sound off).
    sound: Option<sound::ClipAudio>,
    /// A sound file with no picture: the clock runs, the sound follows it, no frames ever come.
    audio_only: bool,
    /// When it went on a layer, until its first frame shows (logged: how long a trigger takes to show).
    opened_at: Option<std::time::Instant>,
}

/// Logs how long opening took when it drops (info level: `RUST_LOG=phosphor=info`).
struct TimingLog {
    name: String,
    started: std::time::Instant,
}

impl Drop for TimingLog {
    fn drop(&mut self) {
        log::info!("{}: opened in {:.0} ms", self.name, self.started.elapsed().as_secs_f64() * 1000.0);
    }
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
    /// Frames handed on to be shown (decoded counts re-decodes too, e.g. for reverse play).
    shown: AtomicU64,
    /// Playing backwards made the picture wait (a group took long to decode): the file is slow in reverse.
    reverse_slow: AtomicBool,
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

/// Random-frames play: one jump per frame time (at the speed), each waiting for the frame before it.
#[derive(Default)]
struct Jumps {
    due: f64,
    waiting: bool,
    dice: Option<phosphor_engine::Dice>,
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
            .position(|f| matches!(f, VideoFrame::Rgba { .. } | VideoFrame::Nv12 { .. }) && f.is_rgba() == rgba && f.size() == (w, h))?;
        Some(pool.swap_remove(i))
    }
}

/// Opens a media file for FFmpeg: a plain file, or an image sequence's pattern ("…/frame_%04d.png") with its first
/// frame number and frame rate (FFmpeg alone only looks for frames 0–4). Every file open goes through here.
pub fn open_input(path: &str) -> Result<ff::format::context::Input, ff::Error> {
    open_input_with(path, true)
}

/// `open_input` for a quick look (how long a movie is) that doesn't ask for a copy of a clip from another drive.
pub fn probe_input(path: &str) -> Result<ff::format::context::Input, ff::Error> {
    open_input_with(path, false)
}

fn open_input_with(path: &str, cache: bool) -> Result<ff::format::context::Input, ff::Error> {
    if !crate::media::sequence::is_sequence(path) {
        // A clip from another drive: this Mac's copy when there is one (and a copy asked for when there isn't).
        let path = if cache { crate::media::clip_cache::resolve(path) } else { path.to_string() };
        let mut input = ff::format::input(&path)?;
        mark_hap_r(&mut input);
        return Ok(input);
    }
    let mut options = ff::Dictionary::new();
    options.set("start_number", &crate::media::sequence::first_number(path).unwrap_or(0).to_string());
    options.set("framerate", crate::media::sequence::RATE);
    ff::format::input_with_dictionary(&path, options)
}

/// Hap R's codec tag.
pub(crate) const HAP_R: [u8; 4] = *b"Hap7";

/// FFmpeg doesn't know Hap R's tag ("Hap7"), so its streams come in as an unknown codec: call them Hap, which sends
/// them down Phosphor's GPU Hap path (BC7 blocks straight to the GPU).
fn mark_hap_r(input: &mut ff::format::context::Input) {
    // SAFETY: plain fields of the opened streams' parameters, before any decoder is made from them.
    unsafe {
        let ctx = input.as_mut_ptr();
        for i in 0..(*ctx).nb_streams as usize {
            let par = (**(*ctx).streams.add(i)).codecpar;
            if (*par).codec_id == ff::ffi::AVCodecID::AV_CODEC_ID_NONE && (*par).codec_tag == u32::from_le_bytes(HAP_R) {
                (*par).codec_id = ff::ffi::AVCodecID::AV_CODEC_ID_HAP;
                (*par).codec_type = ff::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
            }
        }
    }
}

impl VideoPlayer {
    /// Opens the file and starts decoding in the background. Errors are human-readable (shown in the UI).
    pub fn open(path: &str) -> Result<Self, String> {
        init();
        if crate::media::ilda::is_ilda(path) {
            return Self::open(&crate::media::ilda::movie_for(path)?);
        }
        if crate::media::lottie::is_lottie(path) {
            return Self::open(&crate::media::lottie::movie_for(path)?);
        }
        let started = std::time::Instant::now();
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let _timing = TimingLog { name: name.clone(), started };
        let dec = match Decoder::open(path) {
            Ok(dec) => dec,
            Err(e) if e.contains("no video stream") => return Self::open_audio_only(path, &name),
            Err(e) => return Err(format!("{name}: {e}")),
        };
        let mut p = Self::start(name, dec, false)?;
        p.sound = sound::ClipAudio::open(path);
        Ok(p)
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
        let about = dec.about.clone();
        let chapters = dec.chapters.clone();
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
            shown: AtomicU64::new(0),
            reverse_slow: AtomicBool::new(false),
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
            shown_pts: 0.0,
            passes: 0,
            jumps: Jumps::default(),
            about,
            chapters,
            sound: None,
            audio_only: false,
            opened_at: Some(std::time::Instant::now()),
        })
    }

    /// A file with sound but no picture (mp3, wav, …): plays as a clip (loop, in/out, speed, beat sync, the
    /// scrub bar) whose picture is empty; its sound drives the "Clip sound" analyser like any movie's.
    fn open_audio_only(path: &str, name: &str) -> Result<Self, String> {
        let input = open_input(path).map_err(|e| format!("{name}: can't open ({e})"))?;
        let st = input.streams().best(ff::media::Type::Audio).ok_or_else(|| format!("{name}: no video or audio stream"))?;
        let tb = f64::from(st.time_base());
        let duration = if input.duration() > 0 {
            Some(input.duration() as f64 / f64::from(ff::ffi::AV_TIME_BASE))
        } else if st.duration() > 0 && tb > 0.0 {
            Some(st.duration() as f64 * tb)
        } else {
            None
        };
        drop(input);
        let sound = sound::ClipAudio::open(path);
        let shared = Arc::new(Shared {
            ctl: Mutex::new(Ctl { looping: true, mode: PlayMode::default(), quit: false, seek: None }),
            cv: Condvar::new(),
            clock: AtomicU64::new(0f64.to_bits()),
            hardware: AtomicBool::new(false),
            decoded: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            shown: AtomicU64::new(0),
            reverse_slow: AtomicBool::new(false),
            pool: Mutex::new(Vec::new()),
            codec: Mutex::new("audio".into()),
        });
        Ok(Self {
            rx: None,
            shared,
            thread: None,
            playing: true,
            rate: 1.0,
            looping: true,
            clock: 0.0,
            last_now: None,
            pending: None,
            end: None,
            error: None,
            duration,
            size: (0, 0),
            live: false,
            generation: 0,
            mode: PlayMode::default(),
            shown: 0.0,
            shown_pts: 0.0,
            passes: 0,
            jumps: Jumps::default(),
            about: String::new(),
            chapters: Vec::new(),
            sound,
            audio_only: true,
            opened_at: None,
        })
    }

    /// It's on a layer from now (for the "first frame after starting" log).
    pub fn mark_started(&mut self) {
        self.opened_at = Some(std::time::Instant::now());
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
        // Forward → ping-pong carries on forward (it turns at the end): nothing to restart. Any change into or out of
        // reverse (or out of ping-pong, which may be going backwards) restarts from the frame on screen.
        let turned = self.mode.direction != direction && !(self.mode.direction == PlayDirection::Forward && direction == PlayDirection::PingPong);
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
        if self.audio_only {
            self.tick_audio_only();
            return None;
        }
        if self.mode.direction == PlayDirection::Random {
            self.random_jump(now);
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
                        if self.end.is_none() {
                            self.passes += 1;
                        }
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
                    // Jumped back to the start (loop) or turned around (reverse / ping-pong): one pass done.
                    let wrapped = match self.mode.direction {
                        PlayDirection::Reverse => media > self.shown + 0.05,
                        _ => media < self.shown - 0.05,
                    };
                    if wrapped {
                        self.passes += 1;
                    }
                    self.shown = media;
                    self.shown_pts = pts;
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
        self.update_sound();
        if out.is_some() {
            self.jumps.waiting = false;
        }
        if out.is_some()
            && let Some(t) = self.opened_at.take()
        {
            log::info!("first frame {:.0} ms after starting ({}x{})", t.elapsed().as_secs_f64() * 1000.0, self.size.0, self.size.1);
        }
        out
    }

    /// Rendering offline: this movie hasn't got its picture for the current moment yet (still opening, or
    /// playing and out of decoded frames).
    pub fn behind(&self) -> bool {
        if self.live || self.audio_only || self.error.is_some() {
            return false;
        }
        self.opened_at.is_some() || (self.playing && self.end.is_none() && self.pending.is_none())
    }

    /// Random frames: jump somewhere in the loop range once the last jump's frame is on screen and a frame time
    /// (1/30 s, shorter when faster) has gone by. Slow-to-seek files (long-GOP H.264) just jump less often.
    fn random_jump(&mut self, now: f64) {
        if !self.playing || self.jumps.waiting || now < self.jumps.due {
            return;
        }
        let Some((lo, hi)) = self.mode.range.or(self.duration.map(|d| (0.0, d))) else { return };
        let seed = now.to_bits() ^ self.generation;
        let unit = self.jumps.dice.get_or_insert_with(|| phosphor_engine::Dice::new(seed)).unit() as f64;
        self.jumps.due = now + 1.0 / (30.0 * self.rate.abs().max(0.05));
        self.jumps.waiting = true;
        self.seek(lo + unit * (hi - lo).max(0.0));
    }

    /// Sound-only clip: the clock is the position; loop (or stop) at the out point / end, count passes.
    fn tick_audio_only(&mut self) {
        let (lo, hi) = match (self.mode.range, self.duration) {
            (Some((lo, hi)), _) => (lo, hi),
            (None, Some(d)) => (0.0, d),
            (None, None) => (0.0, f64::MAX),
        };
        if self.clock >= hi {
            if self.looping {
                self.clock = lo + (self.clock - hi).rem_euclid((hi - lo).max(0.01));
                self.passes += 1;
            } else if self.end.is_none() {
                self.clock = hi;
                self.end = Some(hi);
                self.passes += 1;
            }
        }
        self.shown = self.clock;
        self.shown_pts = self.clock;
        self.shared.clock.store(self.clock.to_bits(), Ordering::Relaxed);
        self.update_sound();
    }

    /// Tells the sound thread where the picture is (between frames too).
    fn update_sound(&mut self) {
        let Some(s) = &self.sound else { return };
        let ahead = (self.clock - self.shown_pts).clamp(0.0, 0.1);
        let stopped = self.end.is_some() && !self.looping;
        let f = sound::Follow {
            pos: self.shown + ahead,
            playing: self.playing && self.mode.direction == PlayDirection::Forward && !stopped,
            rate: self.rate,
            looping: self.looping,
            range: self.mode.range,
            generation: self.generation,
        };
        s.follow(f);
    }

    /// Another sound device this plays on (None = the main output).
    pub fn set_sound_device(&self, name: Option<&str>) {
        if let Some(s) = &self.sound {
            s.set_device(name);
        }
    }

    /// Which pair of the sound output's channels this plays on (0 = 1-2).
    pub fn set_sound_pair(&self, pair: u32) {
        if let Some(s) = &self.sound {
            s.set_pair(pair);
        }
    }

    /// Sound volume (0 = silent; 1 = as recorded).
    pub fn set_volume(&self, gain: f32) {
        if let Some(s) = &self.sound {
            s.set_gain(gain);
        }
    }

    /// A live stream that ended or failed (it won't show anything new).
    /// A live source (camera, stream) rather than a file.
    pub fn is_live(&self) -> bool {
        self.live
    }

    pub fn live_stopped(&self) -> bool {
        self.live && (self.end.is_some() || self.error.is_some())
    }

    /// Not looping and played to the end (holding its last frame).
    pub fn finished(&self) -> bool {
        !self.looping && self.end.is_some_and(|end| self.clock >= end - 1e-3)
    }

    /// How many times the movie has played through (autopilot moves on when this changes).
    pub fn passes(&self) -> u64 {
        self.passes
    }

    pub fn has_sound(&self) -> bool {
        self.sound.is_some()
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
                // (A live stream ending: the network or the far end stopped it; `live_stopped` says so.)
                Msg::End(at) => self.end = Some(at),
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
        self.shown_pts = t;
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

    /// Chapters stored in the file: (start in seconds, title).
    pub fn chapters(&self) -> &[(f64, String)] {
        &self.chapters
    }

    /// Frame rate, bit rate, pixel format, colour space and sound of the file ("" for sound-only clips).
    pub fn about(&self) -> &str {
        &self.about
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
            frames_shown: self.shared.shown.load(Ordering::Relaxed),
            reverse_slow: self.shared.reverse_slow.load(Ordering::Relaxed),
        }
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.sound = None;
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
mod bc7;
mod hap;
pub mod hap_encode;
mod eq;
pub mod sound;
#[cfg(target_os = "macos")]
pub mod surface;
#[cfg(test)]
mod tests;

use convert::{Scaler, fmt_flags};
use decoder::Decoder;
