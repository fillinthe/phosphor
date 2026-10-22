//! LTC out: the show clock (or the time of day) as timecode audio (25 fps) on a pair of the sound output's channels,
//! for lighting desks, video servers and recorders that read LTC. Frames are written as the time reaches them.

use crate::ltc::LtcEncoder;
use crate::media::video::sound::{self, PlayThrough};

/// LTC frames a second.
const FPS: u32 = 25;

/// Frame number `k` (at 25 fps, wrapping at 24 hours) as hours, minutes, seconds, frames.
fn hmsf(k: i64) -> [u32; 4] {
    let fps = FPS as i64;
    [((k / (fps * 3600)) % 24) as u32, ((k / (fps * 60)) % 60) as u32, ((k / fps) % 60) as u32, (k % fps) as u32]
}

pub struct LtcOut {
    through: PlayThrough,
    enc: LtcEncoder,
    rate: u32,
    /// The last frame written (frames since midnight / the show's start).
    last: Option<i64>,
}

impl LtcOut {
    pub fn start(pair: u8) -> Option<Self> {
        let through = PlayThrough::start()?;
        through.set_pair(pair as u32);
        let rate = match sound::stats().0 {
            0 => 48_000,
            r => r,
        };
        Some(Self { through, enc: LtcEncoder::new(rate), rate, last: None })
    }

    pub fn set_pair(&self, pair: u8) {
        self.through.set_pair(pair as u32);
    }

    /// Writes the frames the time has reached since the last call (`running` false = silence, held).
    pub fn tick(&mut self, secs: f64, running: bool) {
        let rate = match sound::stats().0 {
            0 => self.rate,
            r => r,
        };
        if rate != self.rate {
            (self.rate, self.enc) = (rate, LtcEncoder::new(rate));
        }
        if !running {
            self.last = None;
            return;
        }
        let now = (secs.max(0.0) * FPS as f64).floor() as i64;
        // Started, or the time jumped (a cue): carry on from here.
        let from = match self.last {
            Some(l) if now > l && now - l <= 3 => l + 1,
            Some(l) if now == l => return,
            _ => now,
        };
        for k in from..=now {
            let [h, m, s, f] = hmsf(k);
            let samples = self.enc.frame(h, m, s, f, FPS);
            let frames: Vec<[f32; 2]> = samples.iter().map(|v| [*v, *v]).collect();
            self.through.push(&frames, rate);
        }
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    /// Frames written for a time read back as that time.
    #[test]
    fn ltc_frames_carry_the_time() {
        let start = (3600 + 120 + 4) * 25 + 3;
        assert_eq!(super::hmsf(start), [1, 2, 4, 3]);
        let mut enc = crate::ltc::LtcEncoder::new(48_000);
        let mut dec = crate::ltc::LtcDecoder::new(48_000);
        let mut last = None;
        for k in start..start + 10 {
            let [h, m, s, f] = super::hmsf(k);
            dec.push(enc.frame(h, m, s, f, 25).into_iter());
            if let Some(fr) = dec.frame.take() {
                last = Some(fr.0);
            }
        }
        // The last whole frame decoded (start + 8) ends at start + 9.
        let want = (start + 9) as f64 / 25.0;
        assert!((last.expect("decoded") - want).abs() < 1e-6);
    }
}
