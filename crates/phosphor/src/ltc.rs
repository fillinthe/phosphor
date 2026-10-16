//! LTC (linear / SMPTE timecode as audio) decoding for the cue list: zero crossings → biphase-mark bits →
//! 80-bit frames (sync word 0011 1111 1111 1101) → hours:minutes:seconds:frames. Runs on the audio thread;
//! the time runs on between frames like MIDI timecode.

use std::time::Instant;

/// Decodes one channel of samples.
pub struct LtcDecoder {
    sample_rate: f32,
    /// Samples per bit (adapts to the incoming speed).
    bit: f32,
    since: f32,
    last_sign: bool,
    /// Running signal level, for the zero-crossing hysteresis.
    peak: f32,
    half_pending: bool,
    bits: u128,
    /// A decoded frame: (timecode seconds at the end of the frame, frames per second).
    pub frame: Option<(f64, f64)>,
}

impl LtcDecoder {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(8000) as f32;
        // Start from 30 fps (80 bits a frame); it settles on the real rate within a few bits.
        Self { sample_rate: sr, bit: sr / 2400.0, since: 0.0, last_sign: false, peak: 0.0, half_pending: false, bits: 0, frame: None }
    }

    pub fn push(&mut self, samples: impl Iterator<Item = f32>) {
        for s in samples {
            self.peak = (self.peak * 0.9995).max(s.abs());
            self.since += 1.0;
            let threshold = self.peak * 0.1;
            let sign = if s > threshold {
                true
            } else if s < -threshold {
                false
            } else {
                continue;
            };
            if sign == self.last_sign {
                continue;
            }
            self.last_sign = sign;
            let interval = self.since;
            self.since = 0.0;
            if self.peak < 0.01 {
                continue;
            }
            self.transition(interval);
        }
    }

    fn transition(&mut self, interval: f32) {
        if interval < self.bit * 0.75 {
            // Half a bit: two halves make a 1.
            self.bit = self.bit * 0.95 + interval * 2.0 * 0.05;
            if self.half_pending {
                self.half_pending = false;
                self.bit_in(true);
            } else {
                self.half_pending = true;
            }
        } else if interval < self.bit * 1.6 {
            self.bit = self.bit * 0.95 + interval * 0.05;
            self.half_pending = false;
            self.bit_in(false);
        } else {
            // A gap (signal dropped): start over.
            self.half_pending = false;
        }
        self.bit = self.bit.clamp(self.sample_rate / 4000.0, self.sample_rate / 1500.0);
    }

    fn bit_in(&mut self, one: bool) {
        // Newest bit at the top: after 80 bits, bit 0 of the frame sits at position 48 of the u128.
        self.bits = (self.bits >> 1) | ((one as u128) << 127);
        let frame = (self.bits >> 48) as u128 & ((1u128 << 80) - 1);
        let sync = (frame >> 64) as u16;
        if sync != 0b1011_1111_1111_1100 {
            return;
        }
        let field = |start: u32, len: u32| ((frame >> start) & ((1 << len) - 1)) as u32;
        let frames = field(0, 4) + 10 * field(8, 2);
        let secs = field(16, 4) + 10 * field(24, 3);
        let mins = field(32, 4) + 10 * field(40, 3);
        let hours = field(48, 4) + 10 * field(56, 2);
        let fps_raw = self.sample_rate as f64 / (80.0 * self.bit as f64);
        let fps = [24.0, 25.0, 29.97, 30.0].into_iter().min_by(|a: &f64, b: &f64| (a - fps_raw).abs().total_cmp(&(b - fps_raw).abs())).unwrap_or(30.0);
        if frames < 30 && secs < 60 && mins < 60 && hours < 24 {
            // The frame just ended: its time plus one frame.
            let tc = hours as f64 * 3600.0 + mins as f64 * 60.0 + secs as f64 + (frames as f64 + 1.0) / fps;
            self.frame = Some((tc, fps));
        }
    }
}

/// The newest LTC from any input channel, and when it arrived.
#[derive(Default)]
pub struct LtcClock {
    base: Option<(f64, Instant)>,
    last: Option<Instant>,
    pub fps: f64,
}

impl LtcClock {
    pub fn update(&mut self, decoder: &mut LtcDecoder) {
        if let Some((tc, fps)) = decoder.frame.take() {
            let now = Instant::now();
            self.base = Some((tc, now));
            self.last = Some(now);
            self.fps = fps;
        }
    }

    /// The timecode now, running on between frames; None when no LTC has come in for two seconds.
    pub fn time(&self) -> Option<f64> {
        let (tc, at) = self.base?;
        let quiet = self.last?.elapsed().as_secs_f64();
        if quiet > 2.0 {
            return None;
        }
        Some(if quiet < 0.2 { tc + at.elapsed().as_secs_f64() } else { tc })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An LTC signal for `frames` frames from the given start, at `fps`, as samples.
    fn encode(start: (u32, u32, u32, u32), frames: u32, fps: u32, sample_rate: u32) -> Vec<f32> {
        let bit_len = sample_rate as f32 / (fps as f32 * 80.0);
        let mut out = Vec::new();
        let mut level = 1.0f32;
        let mut pos = 0.0f32;
        let (mut h, mut m, mut s, mut f) = start;
        for _ in 0..frames {
            let mut bits = [false; 80];
            let mut put = |start: usize, len: usize, v: u32| {
                for i in 0..len {
                    bits[start + i] = (v >> i) & 1 == 1;
                }
            };
            put(0, 4, f % 10);
            put(8, 2, f / 10);
            put(16, 4, s % 10);
            put(24, 3, s / 10);
            put(32, 4, m % 10);
            put(40, 3, m / 10);
            put(48, 4, h % 10);
            put(56, 2, h / 10);
            put(64, 16, 0b1011_1111_1111_1100);
            for b in bits {
                // Biphase mark: flip at every bit start; ones flip again half way.
                level = -level;
                let half = pos + bit_len / 2.0;
                let end = pos + bit_len;
                while (out.len() as f32) < half {
                    out.push(level * 0.5);
                }
                if b {
                    level = -level;
                }
                while (out.len() as f32) < end {
                    out.push(level * 0.5);
                }
                pos = end;
            }
            f += 1;
            if f == fps {
                f = 0;
                s += 1;
                if s == 60 {
                    s = 0;
                    m += 1;
                }
            }
        }
        out
    }

    #[test]
    fn decodes_ltc_audio() {
        let samples = encode((1, 2, 3, 4), 30, 25, 48_000);
        let mut d = LtcDecoder::new(48_000);
        let mut last = None;
        for chunk in samples.chunks(480) {
            d.push(chunk.iter().copied());
            if let Some(f) = d.frame.take() {
                last = Some(f);
            }
        }
        let (tc, fps) = last.expect("frames decoded");
        assert_eq!(fps, 25.0);
        // 30 frames from 03:04 end at 04:08, but a bit only completes at the next transition, so the
        // signal's very last frame can't finish: the last whole one is 04:07, which ends at 04:08.
        let want = 3600.0 + 120.0 + 4.0 + 8.0 / 25.0;
        assert!((tc - want).abs() < 1e-6, "{tc} vs {want}");
    }
}
