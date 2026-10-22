//! Movie sound. Each movie with an audio track decodes it on its own thread, following the picture's
//! position (seeks, loops, in/out points, speed = tape-style pitch), into a short buffer. One output
//! stream mixes every clip; it opens with the first clip that has sound and closes after the last one.
//! Reverse play and pauses are silent.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use ff::ffi;
use ffmpeg_next as ff;

/// Sound decoded ahead of what's heard (seconds).
const AHEAD: f64 = 0.12;
/// Picture and sound further apart than this (seconds) → the sound jumps to the picture.
const DRIFT: f64 = 0.15;

static ENABLED: AtomicBool = AtomicBool::new(true);
/// The output device by name (`None` = the Mac's default) and a counter that moves when it changes.
static OUT_DEVICE: Mutex<Option<String>> = Mutex::new(None);
static OUT_CHANGED: AtomicU32 = AtomicU32::new(0);
/// Played on this Mac's output (off = mixed for streams only, silent here).
static LOCAL: AtomicBool = AtomicBool::new(true);
/// Even out the clips' loudness (the project's "Even out volume levels").
static AUTO_LEVEL: AtomicBool = AtomicBool::new(false);
/// Sound EQ band gains in dB (low, mid, high), as f32 bits.
static EQ_DB: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

/// The movie sound EQ: low / mid / high gains in dB (0 = flat).
pub fn set_eq(db: [f32; 3]) {
    for (slot, v) in EQ_DB.iter().zip(db) {
        slot.store(v.to_bits(), Ordering::Relaxed);
    }
}
/// The level every clip is brought towards (RMS, about -20 dBFS).
const LEVEL_TARGET: f32 = 0.1;
/// Quieter than this (about -50 dBFS) is silence: it doesn't count towards a clip's loudness.
const LEVEL_GATE: f32 = 0.003;
/// How far levelling may turn a clip up or down (×4 = 12 dB), so quiet clips don't become noise.
const LEVEL_MAX: f32 = 4.0;
/// Seconds the loudness measurement averages over.
const LEVEL_SECS: f32 = 3.0;
/// Output sample rate once the device is open (0 = not open).
static OUT_RATE: AtomicU32 = AtomicU32::new(0);
/// The output device's channel count once open (0 = not open): pairs beyond 1-2 can carry layers of their own.
static OUT_CHANNELS: AtomicU32 = AtomicU32::new(0);

/// How many channels the sound output has (0 before it opens).
pub fn output_channels() -> u32 {
    OUT_CHANNELS.load(Ordering::Relaxed)
}
static OUTPUT_RUNNING: AtomicBool = AtomicBool::new(false);
static CLIPS: Mutex<Vec<Weak<Clip>>> = Mutex::new(Vec::new());
/// Sample frames the output has taken from clips (diagnostics: bench prints it).
static PLAYED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// A copy of the mix for someone else (only kept while they want it, at most `max` samples, oldest dropped).
struct Tap {
    on: AtomicBool,
    buf: Mutex<VecDeque<f32>>,
    max: usize,
}

impl Tap {
    const fn new(max: usize) -> Self {
        Tap { on: AtomicBool::new(false), buf: Mutex::new(VecDeque::new()), max }
    }

    fn set(&self, on: bool) {
        if self.on.swap(on, Ordering::Relaxed) && !on
            && let Ok(mut t) = self.buf.lock()
        {
            t.clear();
        }
    }

    fn take(&self) -> Vec<f32> {
        self.buf.lock().map(|mut t| t.drain(..).collect()).unwrap_or_default()
    }

    /// From the sound thread: never waits for the lock (a missed block is better than a glitch).
    fn push(&self, samples: impl Iterator<Item = f32>) {
        if self.on.load(Ordering::Relaxed)
            && let Ok(mut t) = self.buf.try_lock()
        {
            t.extend(samples);
            while t.len() > self.max {
                t.pop_front();
            }
        }
    }
}

/// The mix, stereo interleaved, for the audio analyser while "Clip sound" is its source.
static TAP: Tap = Tap::new(2 * 48_000);
/// The mix, stereo, for streams and recordings ("Movie sound" in the Stream panel).
static PROGRAM: Tap = Tap::new(2 * 48_000);
/// The mix, stereo, for NDI (its own copy: a stream and NDI can both run).
static NDI_TAP: Tap = Tap::new(2 * 48_000);

/// Keep (or stop keeping) the mix for NDI.
pub fn set_ndi_tap(on: bool) {
    NDI_TAP.set(on);
}

/// The stereo mix for NDI since the last call (interleaved L R, at `stats().0` Hz).
pub fn take_ndi() -> Vec<f32> {
    NDI_TAP.take()
}

/// Keep (or stop keeping) the mix for the analyser.
pub fn set_tap(on: bool) {
    TAP.set(on);
}

/// Everything mixed since the last call (stereo interleaved, at `stats().0` Hz).
pub fn take_tap() -> Vec<f32> {
    TAP.take()
}

/// Plays movie sound on this output (by name; `None` = the default). A running output moves over at once.
pub fn set_output_device(name: Option<String>) {
    if let Ok(mut d) = OUT_DEVICE.lock() {
        *d = name.filter(|n| !n.is_empty());
    }
    OUT_CHANGED.fetch_add(1, Ordering::SeqCst);
}

pub fn output_device() -> Option<String> {
    OUT_DEVICE.lock().ok().and_then(|d| d.clone())
}

/// The outputs that can play sound (asks the system: call when a menu opens, not every frame).
pub fn output_devices() -> Vec<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    cpal::default_host().output_devices().map(|ds| ds.filter_map(|d| d.description().ok().map(|n| n.name().to_string())).collect()).unwrap_or_default()
}

/// Play the mix on this Mac (on) or only feed it to streams and the analyser (off, silent here).
pub fn set_local(on: bool) {
    LOCAL.store(on, Ordering::Relaxed);
}

/// Keep (or stop keeping) the stereo mix for a stream or recording.
pub fn set_program_tap(on: bool) {
    PROGRAM.set(on);
}

/// The stereo mix since the last call (interleaved L R, at `stats().0` Hz).
pub fn take_program() -> Vec<f32> {
    PROGRAM.take()
}

/// (output sample rate, 0 = closed; sample frames played from clips so far).
pub fn stats() -> (u32, u64) {
    (OUT_RATE.load(Ordering::Relaxed), PLAYED.load(Ordering::Relaxed))
}

/// Turns loudness levelling on or off for every clip.
pub fn set_auto_level(on: bool) {
    AUTO_LEVEL.store(on, Ordering::Relaxed);
}

/// The gain that brings a clip measured at `mean_square` to the target level (1 until something was heard).
fn level_gain(mean_square: f32) -> f32 {
    if mean_square <= 0.0 { 1.0 } else { (LEVEL_TARGET / mean_square.sqrt()).clamp(1.0 / LEVEL_MAX, LEVEL_MAX) }
}

/// Off = movies open without sound (bench runs, tests).
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Where the picture is, so the sound can follow it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Follow {
    /// Media time (seconds) being shown now.
    pub pos: f64,
    /// Forward and running (paused / reverse = silent).
    pub playing: bool,
    pub rate: f64,
    pub looping: bool,
    pub range: Option<(f64, f64)>,
    /// Bumped by every seek: the sound jumps straight away.
    pub generation: u64,
}

struct Clip {
    /// Stereo, interleaved, at the output rate.
    buf: Mutex<VecDeque<f32>>,
    gain: AtomicU32,
    /// Gain last applied by the mixer (ramped towards `gain` so changes never click).
    applied: AtomicU32,
    /// How loud the clip is (mean square, averaged over `LEVEL_SECS`; 0 = nothing heard yet), for levelling.
    loudness: AtomicU32,
    /// Which pair of the output's channels it plays on (0 = 1-2, 1 = 3-4…).
    pair: AtomicU32,
    follow: Mutex<Follow>,
    quit: AtomicBool,
}

impl Clip {
    /// Folds a block of decoded sound (stereo, at `rate`) into the clip's loudness; silence doesn't count.
    fn measure(&self, samples: &[f32], rate: f32) {
        if samples.is_empty() {
            return;
        }
        let ms = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
        if ms.sqrt() < LEVEL_GATE {
            return;
        }
        let old = f32::from_bits(self.loudness.load(Ordering::Relaxed));
        let secs = samples.len() as f32 / 2.0 / rate.max(1.0);
        let new = if old <= 0.0 { ms } else { old + (ms - old) * (secs / LEVEL_SECS).min(1.0) };
        self.loudness.store(new.to_bits(), Ordering::Relaxed);
    }
}

pub(super) struct ClipAudio {
    clip: Arc<Clip>,
    thread: Option<JoinHandle<()>>,
}

/// The audio input heard through the sound output (monitoring): a clip in the mix whose sound is pushed in as it
/// arrives, resampled to the output's rate. Kept short so it stays close to live.
pub struct PlayThrough {
    clip: Arc<Clip>,
}

/// At most this much input waits to be heard (seconds): more is dropped, so the delay can't grow.
const THROUGH_MAX_SECS: f32 = 0.15;

impl PlayThrough {
    pub fn start() -> Option<Self> {
        let clip = Arc::new(Clip {
            buf: Mutex::new(VecDeque::new()),
            gain: AtomicU32::new(1f32.to_bits()),
            applied: AtomicU32::new(0f32.to_bits()),
            loudness: AtomicU32::new(0),
            pair: AtomicU32::new(0),
            follow: Mutex::new(Follow::default()),
            quit: AtomicBool::new(false),
        });
        CLIPS.lock().ok()?.push(Arc::downgrade(&clip));
        ensure_output();
        Some(Self { clip })
    }

    pub fn set_gain(&self, g: f32) {
        self.clip.gain.store(g.clamp(0.0, 4.0).to_bits(), Ordering::Relaxed);
    }

    /// New input (left, right at `rate` Hz) to be heard.
    pub fn push(&self, frames: &[[f32; 2]], rate: u32) {
        let out = match OUT_RATE.load(Ordering::Relaxed) {
            0 => rate,
            r => r,
        };
        let samples = resample(frames, rate, out);
        let Ok(mut buf) = self.clip.buf.lock() else { return };
        buf.extend(samples.iter().flatten());
        let max = (out as f32 * THROUGH_MAX_SECS) as usize * 2;
        while buf.len() > max {
            buf.pop_front();
        }
    }
}

/// Linear resampling of (left, right) frames from `from` Hz to `to` Hz (monitoring quality).
pub fn resample(frames: &[[f32; 2]], from: u32, to: u32) -> Vec<[f32; 2]> {
    if from == to || from == 0 || to == 0 || frames.len() < 2 {
        return frames.to_vec();
    }
    let n = (frames.len() as u64 * to as u64 / from as u64) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 * from as f64 / to as f64;
            let (k, f) = (t as usize, (t.fract()) as f32);
            let (a, b) = (frames[k.min(frames.len() - 1)], frames[(k + 1).min(frames.len() - 1)]);
            [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]
        })
        .collect()
}

impl ClipAudio {
    /// Starts the sound of `path`, or None when it has no audio track (or sound is off).
    pub fn open(path: &str) -> Option<Self> {
        if !ENABLED.load(Ordering::Relaxed) {
            return None;
        }
        let track = Track::open(path)?;
        let clip = Arc::new(Clip {
            buf: Mutex::new(VecDeque::new()),
            gain: AtomicU32::new(1f32.to_bits()),
            applied: AtomicU32::new(0f32.to_bits()),
            loudness: AtomicU32::new(0),
            pair: AtomicU32::new(0),
            follow: Mutex::new(Follow { rate: 1.0, looping: true, ..Default::default() }),
            quit: AtomicBool::new(false),
        });
        CLIPS.lock().ok()?.push(Arc::downgrade(&clip));
        ensure_output();
        let c = clip.clone();
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let thread = std::thread::Builder::new().name(format!("sound: {name}")).spawn(move || track.run(&c)).ok()?;
        Some(Self { clip, thread: Some(thread) })
    }

    pub fn follow(&self, f: Follow) {
        if let Ok(mut cur) = self.clip.follow.lock() {
            *cur = f;
        }
    }

    pub fn set_gain(&self, g: f32) {
        self.clip.gain.store(g.clamp(0.0, 4.0).to_bits(), Ordering::Relaxed);
    }

    /// Plays on this pair of the output's channels (0 = 1-2, 1 = 3-4…; past the last pair = the last pair).
    pub fn set_pair(&self, pair: u32) {
        self.clip.pair.store(pair, Ordering::Relaxed);
    }
}

impl Drop for ClipAudio {
    fn drop(&mut self) {
        self.clip.quit.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The audio track of a file and its decoder.
struct Track {
    path: String,
    input: ff::format::context::Input,
    stream: usize,
    tb: f64,
    /// The picture's start time (seconds): sound times are measured from it, like the picture's.
    origin: f64,
    dec: ff::decoder::Audio,
    frame: ff::frame::Audio,
}

impl Track {
    fn open(path: &str) -> Option<Self> {
        let input = super::open_input(path).ok()?;
        let st = input.streams().best(ff::media::Type::Audio)?;
        let stream = st.index();
        let tb = f64::from(st.time_base());
        let origin = input
            .streams()
            .best(ff::media::Type::Video)
            .map(|v| if v.start_time() != i64::MIN { v.start_time() as f64 * f64::from(v.time_base()) } else { 0.0 })
            .unwrap_or(0.0);
        let ctx = ff::codec::Context::from_parameters(st.parameters()).ok()?;
        let dec = ctx.decoder().audio().ok()?;
        Some(Self { path: path.to_string(), input, stream, tb: if tb > 0.0 { tb } else { 1.0 / 48_000.0 }, origin, dec, frame: ff::frame::Audio::empty() })
    }

    fn run(mut self, clip: &Clip) {
        // Wait for the output to open (it tells us its rate).
        let mut out_rate = 0;
        for _ in 0..200 {
            out_rate = OUT_RATE.load(Ordering::Relaxed);
            if out_rate > 0 || clip.quit.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if out_rate == 0 {
            return;
        }
        let out_rate_f = out_rate as f64;
        let mut res: Option<(Resampler, f64)> = None;
        // Media time of the next sample pushed; None = jump to the picture first.
        let mut next_t: Option<f64> = None;
        // After a jump: output samples to drop until the exact target time.
        let mut trim_to: Option<f64> = None;
        let mut generation = 0;
        let mut at_end = false;
        while !clip.quit.load(Ordering::Relaxed) {
            let f = clip.follow.lock().map(|f| *f).unwrap_or_default();
            if !f.playing || f.rate <= 0.0 {
                clip.clear();
                next_t = None;
                std::thread::sleep(Duration::from_millis(15));
                continue;
            }
            let queued = clip.queued() as f64 / out_rate_f;
            let heard = next_t.map(|t| t - queued * f.rate);
            let drifted = heard.is_some_and(|h| (h - f.pos).abs() > DRIFT);
            if next_t.is_none() || drifted || f.generation != generation {
                generation = f.generation;
                clip.clear();
                let target = f.pos;
                if self.seek(target).is_err() {
                    return;
                }
                next_t = Some(target);
                trim_to = Some(target);
                res = None;
                at_end = false;
                continue;
            }
            if queued > AHEAD {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            // In/out points: back to the in point.
            if let (Some((lo, hi)), Some(t)) = (f.range, next_t)
                && t >= hi
            {
                let _ = self.seek(lo);
                next_t = Some(lo);
                trim_to = Some(lo);
                continue;
            }
            match self.next_frame() {
                Some(()) => at_end = false,
                None => {
                    // End of the track: loop with the picture, or wait for it.
                    if f.looping && !at_end {
                        let lo = f.range.map_or(0.0, |r| r.0);
                        let _ = self.seek(lo);
                        next_t = Some(lo);
                        trim_to = Some(lo);
                        at_end = true;
                    } else {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    continue;
                }
            }
            let ts = self.frame.pts().unwrap_or(i64::MIN);
            let start = if ts != i64::MIN { ts as f64 * self.tb - self.origin } else { next_t.unwrap_or(0.0) };
            let src_rate = self.frame.rate().max(1);
            let dur = self.frame.samples() as f64 / src_rate as f64;
            // Speed changes pitch, like tape: resample to out_rate / rate.
            let dst_rate = (out_rate_f / f.rate).round().clamp(4000.0, 768_000.0);
            if res.as_ref().is_none_or(|(r, d)| *d != dst_rate || !r.matches(&self.frame)) {
                res = Resampler::new(&self.frame, dst_rate as u32).map(|r| (r, dst_rate));
            }
            let Some((r, _)) = res.as_mut() else {
                log::warn!("{}: can't play this sound format", self.path);
                return;
            };
            let Some(samples) = r.run(&self.frame) else { continue };
            let mut skip = 0;
            if let Some(target) = trim_to {
                if start + dur <= target {
                    next_t = Some(start + dur);
                    continue;
                }
                skip = (((target - start).max(0.0)) * dst_rate) as usize * 2;
                trim_to = None;
            }
            let kept = &samples[skip.min(samples.len())..];
            clip.measure(kept, dst_rate as f32);
            if let Ok(mut b) = clip.buf.lock() {
                b.extend(kept.iter());
            }
            next_t = Some(start + dur);
        }
    }

    /// Next decoded frame into `self.frame`; None at the end of the track.
    fn next_frame(&mut self) -> Option<()> {
        loop {
            match self.dec.receive_frame(&mut self.frame) {
                Ok(()) => return Some(()),
                Err(ff::Error::Eof) => return None,
                Err(_) => {}
            }
            loop {
                let mut pkt = ff::Packet::empty();
                match pkt.read(&mut self.input) {
                    Ok(()) if pkt.stream() == self.stream => {
                        let _ = self.dec.send_packet(&pkt);
                        break;
                    }
                    Ok(()) => {}
                    Err(ff::Error::Other { errno }) if errno == libc::EAGAIN => {}
                    Err(e) => {
                        log::debug!("{}: packet read ended: {e}", self.path);
                        let _ = self.dec.send_eof();
                        // Drain what's left; then the next receive says Eof.
                        return match self.dec.receive_frame(&mut self.frame) {
                            Ok(()) => Some(()),
                            Err(_) => None,
                        };
                    }
                }
            }
        }
    }

    fn seek(&mut self, t: f64) -> Result<(), String> {
        let ts = ((t + self.origin).max(0.0) / self.tb) as i64;
        // SAFETY: seeking the demuxer this thread owns.
        let ok = unsafe { ffi::av_seek_frame(self.input.as_mut_ptr(), self.stream as i32, ts, ffi::AVSEEK_FLAG_BACKWARD) >= 0 };
        if !ok {
            self.input = super::open_input(&self.path).map_err(|e| e.to_string())?;
        }
        self.dec.flush();
        Ok(())
    }
}

/// Any decoded sound → stereo f32 at the output rate: samples are read in whatever format the decoder gives
/// (8/16/32-bit ints, floats, packed or planar), mixed to stereo and resampled by linear interpolation. Our own,
/// because FFmpeg's resampler rejects PCM frames that come without a channel layout (wav, aiff).
struct Resampler {
    src: (ff::format::Sample, u32, i32),
    dst_rate: u32,
    /// Position in the source (in frames) of the next output sample.
    pos: f64,
    /// The last source frame, for interpolating across frame boundaries.
    carry: Option<[f32; 2]>,
}

impl Resampler {
    fn new(f: &ff::frame::Audio, dst_rate: u32) -> Option<Self> {
        Some(Self { src: (f.format(), f.rate(), f.channels() as i32), dst_rate, pos: 0.0, carry: None })
    }

    fn matches(&self, f: &ff::frame::Audio) -> bool {
        self.src == (f.format(), f.rate(), f.channels() as i32)
    }

    /// Converted samples (stereo interleaved).
    fn run(&mut self, f: &ff::frame::Audio) -> Option<Vec<f32>> {
        let stereo = to_stereo(f)?;
        let n = stereo.len() / 2;
        let src_rate = self.src.1.max(1) as f64;
        let step = src_rate / self.dst_rate.max(1) as f64;
        if (step - 1.0).abs() < 1e-6 {
            return Some(stereo);
        }
        // Source frames available: carry (index -1) then this frame's 0..n.
        let at = |i: i64| -> [f32; 2] {
            if i < 0 {
                self.carry.unwrap_or([stereo[0], stereo[1]])
            } else {
                let k = (i as usize).min(n - 1) * 2;
                [stereo[k], stereo[k + 1]]
            }
        };
        let mut out = Vec::with_capacity((n as f64 / step) as usize + 2);
        let mut pos = self.pos - 1.0; // relative to the carry frame at -1
        while pos < (n - 1) as f64 {
            let i = pos.floor();
            let t = (pos - i) as f32;
            let (a, b) = (at(i as i64), at(i as i64 + 1));
            out.push(a[0] + (b[0] - a[0]) * t);
            out.push(a[1] + (b[1] - a[1]) * t);
            pos += step;
        }
        self.pos = pos - (n - 1) as f64;
        self.carry = Some(at(n as i64 - 1));
        Some(out)
    }
}

/// A decoded frame as stereo interleaved f32 (mono doubled, more channels averaged in pairs).
fn to_stereo(f: &ff::frame::Audio) -> Option<Vec<f32>> {
    use ff::format::Sample as S;
    use ff::format::sample::Type as T;
    let ch = f.channels().max(1) as usize;
    let n = f.samples();
    let planar = matches!(f.format(), S::U8(T::Planar) | S::I16(T::Planar) | S::I32(T::Planar) | S::I64(T::Planar) | S::F32(T::Planar) | S::F64(T::Planar));
    let bytes_per = match f.format() {
        S::U8(_) => 1,
        S::I16(_) => 2,
        S::I32(_) | S::F32(_) => 4,
        S::I64(_) | S::F64(_) => 8,
        S::None => return None,
    };
    let sample = |plane: usize, idx: usize| -> f32 {
        // Packed: everything in plane 0, interleaved; planar: one plane per channel.
        let (data, off) = if planar { (f.data(plane), idx * bytes_per) } else { (f.data(0), (idx * ch + plane) * bytes_per) };
        if off + bytes_per > data.len() {
            return 0.0;
        }
        let b = &data[off..off + bytes_per];
        match f.format() {
            S::U8(_) => (b[0] as f32 - 128.0) / 128.0,
            S::I16(_) => i16::from_ne_bytes([b[0], b[1]]) as f32 / 32768.0,
            S::I32(_) => i32::from_ne_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
            S::I64(_) => i64::from_ne_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32 / 9.223_372e18,
            S::F32(_) => f32::from_ne_bytes([b[0], b[1], b[2], b[3]]),
            S::F64(_) => f64::from_ne_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32,
            S::None => 0.0,
        }
    };
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let (l, r) = match ch {
            1 => {
                let v = sample(0, i);
                (v, v)
            }
            2 => (sample(0, i), sample(1, i)),
            _ => {
                let (mut l, mut r, mut nl, mut nr) = (0.0, 0.0, 0, 0);
                for c in 0..ch {
                    if c % 2 == 0 {
                        l += sample(c, i);
                        nl += 1;
                    } else {
                        r += sample(c, i);
                        nr += 1;
                    }
                }
                (l / nl.max(1) as f32, r / nr.max(1) as f32)
            }
        };
        out.push(l);
        out.push(r);
    }
    Some(out)
}

impl Clip {
    fn clear(&self) {
        if let Ok(mut b) = self.buf.lock() {
            b.clear();
        }
    }

    /// Frames waiting to be heard.
    fn queued(&self) -> usize {
        self.buf.lock().map_or(0, |b| b.len() / 2)
    }
}

/// Opens the output stream on its own thread (cpal streams stay on the thread that made them). It closes
/// itself a couple of seconds after the last clip with sound goes away.
fn ensure_output() {
    if OUTPUT_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let started = std::thread::Builder::new().name("sound out".into()).spawn(|| {
        // Runs until no clip has sound; starts again on the new device when the output is changed.
        loop {
            match run_output() {
                Ok(true) => continue,
                Ok(false) => break,
                Err(e) => {
                    log::warn!("sound output: {e}");
                    break;
                }
            }
        }
        OUT_RATE.store(0, Ordering::Relaxed);
        OUTPUT_RUNNING.store(false, Ordering::SeqCst);
    });
    if started.is_err() {
        OUTPUT_RUNNING.store(false, Ordering::SeqCst);
    }
}

/// Plays until no clip has sound (false) or the output device is changed (true = start again).
fn run_output() -> Result<bool, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let changed_at = OUT_CHANGED.load(Ordering::SeqCst);
    let wanted = output_device();
    let named = wanted.as_ref().and_then(|w| host.output_devices().ok()?.find(|d| d.description().ok().is_some_and(|n| n.name() == w.as_str())));
    let device = named.or_else(|| host.default_output_device()).ok_or("no sound output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let channels = supported.channels().max(1) as usize;
    let rate = supported.sample_rate();
    let config: cpal::StreamConfig = supported.config();
    // One stereo block per pair of output channels (layers can be sent to 3-4, 5-6… on a multichannel interface).
    let pairs = (channels / 2).max(1);
    let mut blocks: Vec<f32> = Vec::new();
    let mut sum: Vec<f32> = Vec::new();
    let mut eqs: Vec<super::eq::Eq> = (0..pairs).map(|_| super::eq::Eq::default()).collect();
    let mut mix = move |out: &mut [f32]| {
        let frames = out.len() / channels;
        blocks.clear();
        blocks.resize(frames * 2 * pairs, 0.0);
        mix_clips(&mut blocks, pairs);
        let db = EQ_DB.each_ref().map(|d| f32::from_bits(d.load(Ordering::Relaxed)));
        for (block, eq) in blocks.chunks_mut(frames * 2).zip(&mut eqs) {
            eq.process(block, db, rate as f32);
        }
        // Streams, recordings, NDI and the analyser hear everything.
        sum.clear();
        sum.resize(frames * 2, 0.0);
        for block in blocks.chunks(frames * 2) {
            sum.iter_mut().zip(block).for_each(|(s, b)| *s += b);
        }
        TAP.push(sum.iter().copied());
        PROGRAM.push(sum.iter().copied());
        NDI_TAP.push(sum.iter().copied());
        if !LOCAL.load(Ordering::Relaxed) {
            blocks.iter_mut().for_each(|s| *s = 0.0);
        }
        to_device(&blocks, frames, out, channels);
    };
    let err_fn = |e: cpal::Error| log::warn!("sound output: {e}");
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(config, move |d: &mut [f32], _| mix(d), err_fn, None),
        cpal::SampleFormat::I16 => {
            let mut tmp = Vec::new();
            device.build_output_stream(
                config,
                move |d: &mut [i16], _| {
                    tmp.resize(d.len(), 0.0);
                    mix(&mut tmp);
                    for (o, s) in d.iter_mut().zip(&tmp) {
                        *o = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                    }
                },
                err_fn,
                None,
            )
        }
        other => return Err(format!("unsupported output format {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    OUT_RATE.store(rate, Ordering::Relaxed);
    OUT_CHANNELS.store(channels as u32, Ordering::Relaxed);
    log::info!("sound output: {} at {rate} Hz", device.description().map(|d| d.name().to_string()).unwrap_or_default());
    let mut idle = 0;
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let alive = CLIPS.lock().map_or(0, |mut c| {
            c.retain(|w| w.strong_count() > 0);
            c.len()
        });
        idle = if alive == 0 { idle + 1 } else { 0 };
        if idle >= 8 {
            return Ok(false);
        }
        if OUT_CHANGED.load(Ordering::SeqCst) != changed_at {
            return Ok(true);
        }
    }
}

/// The pair blocks (each `frames` stereo frames) into the device's interleaved channels; a mono device gets the
/// first pair folded down, channels past the pairs stay silent.
fn to_device(blocks: &[f32], frames: usize, out: &mut [f32], channels: usize) {
    for (i, frame) in out.chunks_mut(channels).enumerate().take(frames) {
        if frame.len() == 1 {
            frame[0] = 0.5 * (blocks[i * 2] + blocks[i * 2 + 1]);
            continue;
        }
        for (c, s) in frame.iter_mut().enumerate() {
            let (pair, side) = (c / 2, c % 2);
            *s = blocks.get(pair * frames * 2 + i * 2 + side).copied().unwrap_or(0.0);
        }
    }
}

/// Adds every clip's next samples into its pair's block of `out` (`pairs` stereo blocks), ramping gain changes.
fn mix_clips(out: &mut [f32], pairs: usize) {
    let clips: Vec<Arc<Clip>> = match CLIPS.try_lock() {
        Ok(c) => c.iter().filter_map(Weak::upgrade).collect(),
        Err(_) => return,
    };
    let frames = out.len() / 2 / pairs.max(1);
    let level = AUTO_LEVEL.load(Ordering::Relaxed);
    for clip in clips {
        let mut target = f32::from_bits(clip.gain.load(Ordering::Relaxed));
        if level {
            target *= level_gain(f32::from_bits(clip.loudness.load(Ordering::Relaxed)));
        }
        let from = f32::from_bits(clip.applied.load(Ordering::Relaxed));
        let Ok(mut buf) = clip.buf.lock() else { continue };
        let n = frames.min(buf.len() / 2);
        let base = (clip.pair.load(Ordering::Relaxed) as usize).min(pairs.max(1) - 1) * frames * 2;
        for i in 0..n {
            let g = from + (target - from) * (i as f32 / frames.max(1) as f32);
            out[base + i * 2] += buf.pop_front().unwrap_or(0.0) * g;
            out[base + i * 2 + 1] += buf.pop_front().unwrap_or(0.0) * g;
        }
        clip.applied.store(target.to_bits(), Ordering::Relaxed);
        PLAYED.fetch_add(n as u64, Ordering::Relaxed);
    }
    for s in out.iter_mut() {
        *s = s.clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod level_tests {
    use super::*;

    #[test]
    fn input_is_resampled_for_the_output() {
        let frames: Vec<[f32; 2]> = (0..441).map(|i| [i as f32, -(i as f32)]).collect();
        let out = resample(&frames, 44_100, 48_000);
        assert_eq!(out.len(), 480);
        assert!((out[240][0] - 220.5).abs() < 0.6 && out[240][1] < 0.0, "{:?}", out[240]);
        assert_eq!(resample(&frames, 48_000, 48_000).len(), 441);
    }

    /// Pair blocks land on their channels: pair 0 on 1-2, pair 1 on 3-4; a 6-channel device's 5-6 stay silent.
    #[test]
    fn pairs_go_to_their_channels() {
        // Two frames each: pair 0 = (1, 2), (3, 4); pair 1 = (5, 6), (7, 8).
        let blocks = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut out = [9.0; 12];
        to_device(&blocks, 2, &mut out, 6);
        assert_eq!(out, [1.0, 2.0, 5.0, 6.0, 0.0, 0.0, 3.0, 4.0, 7.0, 8.0, 0.0, 0.0]);
        let mut mono = [0.0; 2];
        to_device(&blocks[..4], 2, &mut mono, 1);
        assert_eq!(mono, [1.5, 3.5]);
    }

    /// A loud clip is turned down and a quiet one up, to the same level; silence and the limits hold.
    #[test]
    fn levelling_meets_in_the_middle() {
        let loud = level_gain(0.3 * 0.3);
        let quiet = level_gain(0.04 * 0.04);
        assert!((0.3 * loud - LEVEL_TARGET).abs() < 1e-4, "{loud}");
        assert!((0.04 * quiet - LEVEL_TARGET).abs() < 1e-4, "{quiet}");
        assert_eq!(level_gain(0.0), 1.0);
        assert_eq!(level_gain(0.0001 * 0.0001), LEVEL_MAX);
        let clip = Clip { buf: Mutex::new(VecDeque::new()), gain: AtomicU32::new(0), applied: AtomicU32::new(0), loudness: AtomicU32::new(0), pair: AtomicU32::new(0), follow: Mutex::new(Follow::default()), quit: AtomicBool::new(false) };
        clip.measure(&[0.0001; 4800], 48_000.0);
        assert_eq!(f32::from_bits(clip.loudness.load(Ordering::Relaxed)), 0.0, "silence doesn't count");
        clip.measure(&[0.2; 4800], 48_000.0);
        assert!((f32::from_bits(clip.loudness.load(Ordering::Relaxed)) - 0.04).abs() < 1e-4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sound follows the picture: after a jump to 0.9 s the test beep (1.0–1.1 s) arrives ~0.1 s into the buffer.
    /// The movie isn't in git; make it with the FFmpeg line in .github/workflows/windows.yml (App tests).
    #[test]
    fn sound_starts_where_the_picture_is() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../media/test/av.mp4");
        let track = Track::open(path).expect("test movie has sound");
        // No device in tests: pretend one is open at 48 kHz and read the buffer directly.
        OUT_RATE.store(48_000, Ordering::Relaxed);
        let clip = Arc::new(Clip {
            buf: Mutex::new(VecDeque::new()),
            gain: AtomicU32::new(1f32.to_bits()),
            applied: AtomicU32::new(0),
            loudness: AtomicU32::new(0),
            pair: AtomicU32::new(0),
            follow: Mutex::new(Follow { pos: 0.9, playing: true, rate: 1.0, looping: true, range: None, generation: 1 }),
            quit: AtomicBool::new(false),
        });
        let c = clip.clone();
        let t = std::thread::spawn(move || track.run(&c));
        let mut samples = Vec::new();
        for _ in 0..200 {
            std::thread::sleep(Duration::from_millis(5));
            if let Ok(mut b) = clip.buf.lock() {
                samples.extend(b.drain(..));
            }
            if samples.len() >= 48_000 / 2 * 2 {
                break;
            }
        }
        clip.quit.store(true, Ordering::Relaxed);
        t.join().unwrap();
        let first_loud = samples.chunks(2).position(|f| f[0].abs() > 0.1).expect("beep heard");
        let at = first_loud as f64 / 48_000.0;
        assert!((at - 0.1).abs() < 0.02, "beep at {at:.3} s after the jump, expected 0.100");
    }
}

#[cfg(test)]
mod resample_tests {
    use super::*;

    fn i16_frame(samples: usize, rate: u32) -> ff::frame::Audio {
        let mut f = ff::frame::Audio::new(ff::format::Sample::I16(ff::format::sample::Type::Packed), samples, ff::ChannelLayout::STEREO);
        f.set_rate(rate);
        let data = f.data_mut(0);
        for i in 0..samples {
            // Left: a ramp; right: its negative.
            let v = ((i as f32 / samples as f32) * 32767.0) as i16;
            data[i * 4..i * 4 + 2].copy_from_slice(&v.to_ne_bytes());
            data[i * 4 + 2..i * 4 + 4].copy_from_slice(&(-v).to_ne_bytes());
        }
        f
    }

    #[test]
    fn packed_i16_becomes_stereo_f32() {
        let f = i16_frame(100, 48_000);
        let s = to_stereo(&f).unwrap();
        assert_eq!(s.len(), 200);
        assert!((s[2 * 50] - 0.5).abs() < 0.01 && (s[2 * 50 + 1] + 0.5).abs() < 0.01, "{:?}", &s[100..102]);
    }

    #[test]
    fn half_speed_doubles_the_samples_and_keeps_the_ramp() {
        let f = i16_frame(1000, 48_000);
        let mut r = Resampler::new(&f, 96_000).unwrap();
        let out = r.run(&f).unwrap();
        let n = out.len() / 2;
        assert!((1990..=2002).contains(&n), "{n}");
        // Still a ramp: monotonic on the left channel.
        assert!(out.chunks_exact(2).map(|c| c[0]).zip(out.chunks_exact(2).map(|c| c[0]).skip(1)).all(|(a, b)| b >= a - 1e-4));
        // Same rate: samples pass straight through.
        let mut same = Resampler::new(&f, 48_000).unwrap();
        assert_eq!(same.run(&f).unwrap().len(), 2000);
    }
}
