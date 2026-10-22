//! Sound EQ on the movie sound mix: low shelf (250 Hz), mid peak (1 kHz), high shelf (4 kHz), each ±dB. RBJ
//! cookbook biquads, stereo. Does nothing while every band is at 0 dB.

/// Band centres (Hz): low shelf, mid peak, high shelf.
const BANDS: [f32; 3] = [250.0, 1000.0, 4000.0];

#[derive(Clone, Copy, Default)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    /// Per channel (left, right): the last two inputs and outputs.
    x: [[f32; 2]; 2],
    y: [[f32; 2]; 2],
}

impl Biquad {
    /// Kind 0 = low shelf, 1 = peak, 2 = high shelf.
    fn new(kind: usize, hz: f32, db: f32, rate: f32) -> Self {
        let a = 10f32.powf(db / 40.0);
        let w = std::f32::consts::TAU * hz / rate;
        let (sin, cos) = w.sin_cos();
        let q = 0.707;
        let alpha = sin / (2.0 * q);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            0 => {
                let s = 2.0 * a.sqrt() * alpha;
                (a * ((a + 1.0) - (a - 1.0) * cos + s), 2.0 * a * ((a - 1.0) - (a + 1.0) * cos), a * ((a + 1.0) - (a - 1.0) * cos - s), (a + 1.0) + (a - 1.0) * cos + s, -2.0 * ((a - 1.0) + (a + 1.0) * cos), (a + 1.0) + (a - 1.0) * cos - s)
            }
            2 => {
                let s = 2.0 * a.sqrt() * alpha;
                (a * ((a + 1.0) + (a - 1.0) * cos + s), -2.0 * a * ((a - 1.0) + (a + 1.0) * cos), a * ((a + 1.0) + (a - 1.0) * cos - s), (a + 1.0) - (a - 1.0) * cos + s, 2.0 * ((a - 1.0) - (a + 1.0) * cos), (a + 1.0) - (a - 1.0) * cos - s)
            }
            _ => (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a),
        };
        Self { b: [b0 / a0, b1 / a0, b2 / a0], a: [a1 / a0, a2 / a0], ..Default::default() }
    }

    fn run(&mut self, ch: usize, v: f32) -> f32 {
        let (x, y) = (&mut self.x[ch], &mut self.y[ch]);
        let out = self.b[0] * v + self.b[1] * x[0] + self.b[2] * x[1] - self.a[0] * y[0] - self.a[1] * y[1];
        *x = [v, x[0]];
        *y = [out, y[0]];
        out
    }
}

/// Three bands, rebuilt when the gains or the sample rate change (the filters' memory carries over).
#[derive(Default)]
pub struct Eq {
    db: [f32; 3],
    rate: f32,
    filters: [Biquad; 3],
}

impl Eq {
    /// Runs the EQ over stereo interleaved `samples` at `rate`, with these band gains (dB).
    pub fn process(&mut self, samples: &mut [f32], db: [f32; 3], rate: f32) {
        if db.iter().all(|d| d.abs() < 0.05) {
            self.db = [0.0; 3];
            return;
        }
        if db != self.db || rate != self.rate {
            for k in 0..3 {
                let memory = (self.filters[k].x, self.filters[k].y);
                self.filters[k] = Biquad::new(k, BANDS[k], db[k], rate);
                (self.filters[k].x, self.filters[k].y) = memory;
            }
            (self.db, self.rate) = (db, rate);
        }
        for frame in samples.chunks_exact_mut(2) {
            for (ch, s) in frame.iter_mut().enumerate() {
                *s = self.filters.iter_mut().fold(*s, |v, f| f.run(ch, v));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How loud a sine at `hz` comes out compared with going in (RMS, after the filters settle).
    fn gain_at(db: [f32; 3], hz: f32) -> f32 {
        let rate = 48_000.0;
        let mut eq = Eq::default();
        let input: Vec<f32> = (0..9600)
            .flat_map(|i| {
                let v = (std::f32::consts::TAU * hz * i as f32 / rate).sin();
                [v, v]
            })
            .collect();
        let mut out = input.clone();
        eq.process(&mut out, db, rate);
        let rms = |s: &[f32]| (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt();
        rms(&out[9600..]) / rms(&input[9600..])
    }

    #[test]
    fn bands_cut_and_boost_their_own_frequencies() {
        assert!((gain_at([0.0; 3], 100.0) - 1.0).abs() < 0.01, "flat = untouched");
        assert!(gain_at([-12.0, 0.0, 0.0], 60.0) < 0.35, "bass cut");
        assert!((gain_at([-12.0, 0.0, 0.0], 8000.0) - 1.0).abs() < 0.1, "highs untouched by the bass cut");
        assert!(gain_at([0.0, 6.0, 0.0], 1000.0) > 1.8, "mid boost");
        assert!(gain_at([0.0, 0.0, -12.0], 12_000.0) < 0.35, "treble cut");
    }
}
