//! Visual beat: scan a movie's motion, find the rhythm it was cut or animated to (period and the
//! moment of its first hit), then stretch the clip so those hits land on the music's beats.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use ffmpeg_next as ff;

/// What a clip's motion says about its tempo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rhythm {
    pub bpm: f32,
    /// Seconds between visual hits.
    pub period: f32,
    /// Seconds into the clip of the first hit.
    pub phase: f32,
    pub duration: f32,
    /// How regular the hits are (0 = none, 1 = metronome).
    pub confidence: f32,
}

/// Below this steadiness the pulse is noise (hand-held phone footage scores 0.1; cuts on the beat score 0.3+).
pub const STEADY: f32 = 0.2;

impl Rhythm {
    /// A rhythm worth locking to; otherwise the clip is left as it is.
    pub fn steady(&self) -> bool {
        self.confidence >= STEADY
    }
}

/// What to set once the scan is done.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatchTarget {
    Layer(u64),
    Clip(u64, usize),
}

/// Scans in the background, one movie at a time; results are kept per path.
pub struct Analyzer {
    tx: Sender<(String, MatchTarget)>,
    rx: Receiver<(String, MatchTarget, Result<Rhythm, String>)>,
    pub results: HashMap<String, Result<Rhythm, String>>,
    pending: Vec<String>,
}

impl Default for Analyzer {
    fn default() -> Self {
        let (tx, jobs) = channel::<(String, MatchTarget)>();
        let (done, rx) = channel();
        std::thread::Builder::new()
            .name("phosphor-rhythm".into())
            .spawn(move || {
                for (path, target) in jobs {
                    let r = analyze(&path, 120.0);
                    if done.send((path, target, r)).is_err() {
                        break;
                    }
                }
            })
            .ok();
        Self { tx, rx, results: HashMap::new(), pending: Vec::new() }
    }
}

impl Analyzer {
    pub fn request(&mut self, path: &str, target: MatchTarget) {
        self.pending.push(path.to_string());
        let _ = self.tx.send((path.to_string(), target));
    }

    pub fn is_pending(&self, path: &str) -> bool {
        self.pending.iter().any(|p| p == path)
    }

    /// Finished scans since the last call.
    pub fn poll(&mut self) -> Vec<(String, MatchTarget, Result<Rhythm, String>)> {
        let mut out = Vec::new();
        while let Ok((path, target, r)) = self.rx.try_recv() {
            if let Some(i) = self.pending.iter().position(|p| *p == path) {
                self.pending.remove(i);
            }
            self.results.insert(path.clone(), r.clone());
            out.push((path, target, r));
        }
        out
    }
}

/// How many beats one visual hit should take at `music_bpm`, so the clip plays near its natural speed:
/// a clip cut every two beats at 128 BPM reads as 64 BPM and gets 2 beats per hit (never faster than 2× or slower than ½×).
pub fn beats_per_hit(r: &Rhythm, music_bpm: f32) -> f32 {
    if !(music_bpm > 0.0) || !(r.bpm > 0.0) {
        return 1.0;
    }
    let ratio = music_bpm / r.bpm;
    let k = ratio.log2().round().clamp(-1.0, 1.0);
    2f32.powf(k)
}

/// The settings that make the hits land on beats: the loop starts at the first hit and the clip (from there)
/// is stretched to a whole number of beats (`beats_per_hit` per hit). Returns (loop_in as a fraction, beats).
pub fn settings_for(r: &Rhythm, loop_out: f32, music_bpm: f32) -> (f32, f32) {
    let end = loop_out.clamp(0.05, 1.0) * r.duration;
    let phase = if r.phase < end - r.period { r.phase } else { 0.0 };
    let hits = ((end - phase) / r.period.max(0.01)).round().max(1.0);
    (phase / r.duration.max(0.01), (hits * beats_per_hit(r, music_bpm)).round().max(1.0))
}

/// Decodes the movie small and grey, measuring how much each frame differs from the last.
/// Returns the motion series and its frame rate.
fn motion(path: &str, max_secs: f32) -> Result<(Vec<f32>, f32, f32), String> {
    let mut input = crate::media::video::open_input(path).map_err(|e| format!("can't open ({e})"))?;
    let stream = input.streams().best(ff::media::Type::Video).ok_or("no video")?;
    let index = stream.index();
    let rate = stream.avg_frame_rate();
    let fps = if rate.denominator() > 0 { rate.numerator() as f32 / rate.denominator() as f32 } else { 30.0 };
    let fps = if fps.is_finite() && fps > 1.0 { fps } else { 30.0 };
    let duration = input.duration() as f32 / ff::ffi::AV_TIME_BASE as f32;
    let ctx = ff::codec::Context::from_parameters(stream.parameters()).map_err(|e| e.to_string())?;
    let mut dec = ctx.decoder().video().map_err(|e| e.to_string())?;
    // Fewer pixels than a thumbnail: motion, not detail, is what counts.
    let (w, h) = (48u32, 27u32);
    let mut scaler = ff::software::scaling::Context::get(dec.format(), dec.width(), dec.height(), ff::format::Pixel::GRAY8, w, h, ff::software::scaling::Flags::AREA)
        .map_err(|e| e.to_string())?;
    let mut prev: Vec<u8> = Vec::new();
    let mut series: Vec<f32> = Vec::new();
    let max_frames = (max_secs * fps) as usize;
    let mut frame = ff::frame::Video::empty();
    let mut small = ff::frame::Video::empty();
    let mut push = |dec: &mut ff::decoder::Video, series: &mut Vec<f32>| {
        while dec.receive_frame(&mut frame).is_ok() {
            if series.len() >= max_frames {
                continue;
            }
            if scaler.run(&frame, &mut small).is_err() {
                continue;
            }
            let stride = small.stride(0);
            let data = small.data(0);
            let mut cur = Vec::with_capacity((w * h) as usize);
            for y in 0..h as usize {
                cur.extend_from_slice(&data[y * stride..y * stride + w as usize]);
            }
            let e = if prev.len() == cur.len() { cur.iter().zip(&prev).map(|(a, b)| (*a as f32 - *b as f32).abs()).sum::<f32>() / cur.len() as f32 } else { 0.0 };
            series.push(e);
            prev = cur;
        }
    };
    for (s, packet) in input.packets() {
        if s.index() != index {
            continue;
        }
        if series.len() >= max_frames {
            break;
        }
        if dec.send_packet(&packet).is_ok() {
            push(&mut dec, &mut series);
        }
    }
    let _ = dec.send_eof();
    push(&mut dec, &mut series);
    let duration = if duration.is_finite() && duration > 0.0 { duration } else { series.len() as f32 / fps };
    Ok((series, fps, duration))
}

/// Scans a movie (the first `max_secs`) for its visual beat.
pub fn analyze(path: &str, max_secs: f32) -> Result<Rhythm, String> {
    let (series, fps, duration) = motion(path, max_secs)?;
    rhythm_of(&series, fps, duration).ok_or_else(|| "no steady rhythm in the motion".to_string())
}

/// Finds the strongest regular pulse in a motion series: 50–200 BPM, preferring 70–160.
pub fn rhythm_of(series: &[f32], fps: f32, duration: f32) -> Option<Rhythm> {
    let n = series.len();
    if n < (fps * 2.0) as usize || fps <= 0.0 {
        return None;
    }
    // Onsets: how much each frame's motion rises above the recent average.
    let win = (fps * 0.4).max(2.0) as usize;
    let mut onset = vec![0.0f32; n];
    for i in 0..n {
        let lo = i.saturating_sub(win);
        let avg = series[lo..i.max(lo + 1)].iter().sum::<f32>() / (i - lo).max(1) as f32;
        onset[i] = (series[i] - avg).max(0.0);
    }
    // Spread each hit over a few frames so a beat that falls between frames still correlates.
    let blur = (fps * 0.06).round().max(1.0) as usize;
    let onset: Vec<f32> = (0..n).map(|i| {
        let lo = i.saturating_sub(blur);
        let hi = (i + blur + 1).min(n);
        onset[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
    }).collect();
    let mean = onset.iter().sum::<f32>() / n as f32;
    if mean <= 1e-6 {
        return None;
    }
    let onset: Vec<f32> = onset.iter().map(|o| o - mean).collect();
    // Autocorrelation over the beat lags.
    let (min_lag, max_lag) = (((fps * 60.0 / 200.0) as usize).max(2), ((fps * 60.0 / 50.0) as usize).min(n / 2));
    if max_lag <= min_lag {
        return None;
    }
    let corr = |lag: usize| -> f32 { (0..n - lag).map(|i| onset[i] * onset[i + lag]).sum::<f32>() / (n - lag) as f32 };
    let c0 = corr(0).max(1e-9);
    let mut best = (0usize, f32::MIN);
    let mut sum = 0.0;
    for lag in min_lag..=max_lag {
        let bpm = 60.0 * fps / lag as f32;
        // Mild preference for the usual dance range so octave errors fall the right way.
        let weight = if (70.0..=160.0).contains(&bpm) { 1.0 } else { 0.85 };
        let c = corr(lag) / c0 * weight;
        sum += c;
        if c > best.1 {
            best = (lag, c);
        }
    }
    let (mut lag, mut peak) = best;
    if lag == 0 || peak <= 0.0 {
        return None;
    }
    let _ = sum;
    // A pulse every beat also correlates at two and three beats: take the shortest lag that's nearly as strong.
    for div in [3usize, 2] {
        let cand = lag / div;
        if cand >= min_lag {
            let c = corr(cand).max(corr(cand + 1)) / c0;
            if c >= 0.7 * peak {
                lag = if corr(cand + 1) > corr(cand) { cand + 1 } else { cand };
                peak = c;
            }
        }
    }
    let confidence = (corr(lag) / c0).clamp(0.0, 1.0);
    // Refine the lag between frames with the neighbours (parabolic).
    let (cm, cp) = (corr(lag - 1) / c0, corr(lag + 1) / c0);
    let denom = cm - 2.0 * (peak) + cp;
    let shift = if denom.abs() > 1e-6 { 0.5 * (cm - cp) / denom } else { 0.0 };
    let period = (lag as f32 + shift.clamp(-0.5, 0.5)) / fps;
    // Phase: the offset whose comb (stepping by the exact, fractional period) catches the most onset energy.
    let step = period * fps;
    let mut best_off = (0.0f32, f32::MIN);
    let mut off = 0.0f32;
    while off < step {
        let mut s = 0.0;
        let mut t = off;
        while (t as usize) < n {
            s += onset[t.round() as usize % n].max(0.0);
            t += step;
        }
        if s > best_off.1 {
            best_off = (off, s);
        }
        off += 0.25;
    }
    let mut phase = best_off.0 / fps;
    // A first hit right at one period in is the same as a hit at the very start.
    if phase > period - 2.0 / fps {
        phase = 0.0;
    }
    Some(Rhythm { bpm: 60.0 / period, period, phase, duration, confidence })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clip with a flash every half second, the first one 0.2 s in: 120 BPM, phase 0.2.
    #[test]
    fn finds_a_regular_pulse() {
        let fps = 25.0;
        let n = 25 * 12;
        let series: Vec<f32> = (0..n).map(|i| {
            let t = i as f32 / fps;
            let since = (t - 0.2).rem_euclid(0.5);
            (if since < 0.05 && t >= 0.2 { 20.0 } else { 1.0 }) + ((i * 7919) % 13) as f32 * 0.1
        }).collect();
        let r = rhythm_of(&series, fps, 12.0).expect("rhythm");
        assert!((r.bpm - 120.0).abs() < 3.0, "{r:?}");
        assert!((r.phase - 0.2).abs() < 0.08, "{r:?}");
        assert!(r.confidence > 0.3, "{r:?}");
        let (loop_in, beats) = settings_for(&r, 1.0, 120.0);
        assert!((loop_in - 0.2 / 12.0).abs() < 0.01, "{loop_in}");
        assert_eq!(beats, 24.0);
        // Against 240 BPM music each hit is two beats; against 60 BPM, half a beat.
        assert_eq!(settings_for(&r, 1.0, 240.0).1, 48.0);
        assert_eq!(settings_for(&r, 1.0, 60.0).1, 12.0);
        // Far-off tempos are capped at 2 beats / half a beat per hit.
        assert_eq!(beats_per_hit(&r, 1000.0), 2.0);
        assert_eq!(beats_per_hit(&r, 10.0), 0.5);
    }

    #[test]
    fn noise_has_no_rhythm_or_low_confidence() {
        let mut x: u32 = 0x9E3779B9;
        let series: Vec<f32> = (0..600)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x % 1000) as f32 * 0.01
            })
            .collect();
        if let Some(r) = rhythm_of(&series, 25.0, 24.0) {
            assert!(r.confidence < 0.5, "{r:?}");
        }
    }
}
