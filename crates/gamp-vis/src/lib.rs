//! G-Drop: Gamp's MilkDrop-style visualizer, in pure Rust on the CPU,
//! so it runs the same on every machine.
//!
//! Each frame the previous image is pulled through a warp mesh (zoom,
//! rotation, stretch, a slow wobble and drift, per preset and per mesh
//! point), fades a little, and gets waveforms and shapes drawn into it.
//! The result feeds the next frame: trails, tunnels and blooms emerge.
//! Presets are small Rust functions of the music (bass, mids, treble,
//! beats, time).

mod presets;

pub use presets::PRESETS;

/// What the music is doing, normalised so that "usual" is about 1.0
/// (MilkDrop's convention).
#[derive(Clone, Copy, Debug, Default)]
pub struct Features {
    pub time: f32,
    pub frame: u64,
    pub bass: f32,
    pub mid: f32,
    pub treb: f32,
    /// Smoothed (attenuated) versions.
    pub bass_att: f32,
    pub mid_att: f32,
    pub treb_att: f32,
    pub beat: bool,
    /// Overall loudness, 0..~1.
    pub vol: f32,
}

#[derive(Default)]
struct Listener {
    long: [f32; 3],
    att: [f32; 3],
    last_beat: f32,
}

impl Listener {
    fn hear(&mut self, spectrum: &[f32], rate: u32, time: f32, dt: f32) -> ([f32; 3], [f32; 3], bool, f32) {
        let n = spectrum.len().max(1);
        let hz = rate as f32 / 2.0 / n as f32;
        let band = |lo: f32, hi: f32| {
            let (a, b) = ((lo / hz) as usize, ((hi / hz) as usize).min(n));
            spectrum[a.min(b)..b].iter().map(|v| v * v).sum::<f32>().sqrt()
        };
        let raw = [band(20.0, 250.0), band(250.0, 2000.0), band(2000.0, 16000.0)];
        let vol = (raw[0] + raw[1] + raw[2]).min(1.0);
        let mut rel = [0.0; 3];
        for i in 0..3 {
            // A slow average (about 5 s) is "normal" for this song.
            let k_long = 1.0 - (-dt / 5.0).exp();
            self.long[i] += (raw[i] - self.long[i]) * k_long;
            rel[i] = if self.long[i] > 1e-5 { raw[i] / self.long[i] } else { 0.0 };
            let k_att = 1.0 - (-dt / 0.3).exp();
            self.att[i] += (rel[i] - self.att[i]) * k_att;
        }
        let beat = rel[0] > 1.35 * self.att[0].max(0.6) && time - self.last_beat > 0.28;
        if beat {
            self.last_beat = time;
        }
        (rel, self.att, beat, vol)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub on: bool,
    pub x: f32,
    pub y: f32,
    pub rad: f32,
    pub sides: u32,
    pub ang: f32,
    pub color: [f32; 3],
    pub alpha: f32,
}

impl Default for Shape {
    fn default() -> Self {
        Shape { on: false, x: 0.5, y: 0.5, rad: 0.1, sides: 32, ang: 0.0, color: [1.0, 1.0, 1.0], alpha: 0.5 }
    }
}

/// Everything a preset sets each frame.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub zoom: f32,
    pub zoom_exp: f32,
    pub rot: f32,
    pub warp: f32,
    pub warp_speed: f32,
    pub warp_scale: f32,
    pub cx: f32,
    pub cy: f32,
    pub dx: f32,
    pub dy: f32,
    pub sx: f32,
    pub sy: f32,
    pub decay: f32,
    /// Per-channel multiplier in the feedback (colour drift).
    pub tint: [f32; 3],
    pub wave_mode: u8,
    pub wave: [f32; 3],
    pub wave_alpha: f32,
    pub wave_scale: f32,
    pub wave_x: f32,
    pub wave_y: f32,
    pub wave_thick: bool,
    /// Outer border drawn each frame: size (0..0.1) and colour.
    pub border: f32,
    pub border_color: [f32; 3],
    pub gamma: f32,
    pub invert: bool,
    pub wrap: bool,
    pub shapes: [Shape; 2],
}

impl Default for Params {
    fn default() -> Self {
        Params {
            zoom: 1.0,
            zoom_exp: 1.0,
            rot: 0.0,
            warp: 1.0,
            warp_speed: 1.0,
            warp_scale: 1.0,
            cx: 0.5,
            cy: 0.5,
            dx: 0.0,
            dy: 0.0,
            sx: 1.0,
            sy: 1.0,
            decay: 0.98,
            tint: [1.0; 3],
            wave_mode: 0,
            wave: [1.0, 1.0, 1.0],
            wave_alpha: 0.8,
            wave_scale: 1.0,
            wave_x: 0.5,
            wave_y: 0.5,
            wave_thick: false,
            border: 0.0,
            border_color: [0.0; 3],
            gamma: 1.0,
            invert: false,
            wrap: true,
            shapes: [Shape::default(); 2],
        }
    }
}

/// Per mesh point overrides (x, y in 0..1, rad from centre, ang).
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub zoom: f32,
    pub rot: f32,
    pub dx: f32,
    pub dy: f32,
    pub warp: f32,
}

pub type FrameFn = fn(&Features, &mut Params);
pub type PointFn = fn(&Features, &Params, f32, f32, f32, f32, &mut Point);

pub struct Preset {
    pub name: &'static str,
    pub frame: FrameFn,
    pub point: Option<PointFn>,
}

const GW: usize = 32;
const GH: usize = 24;
/// Seconds a soft change between presets takes.
const BLEND: f32 = 2.5;

pub struct GDrop {
    w: usize,
    h: usize,
    buf: Vec<[f32; 3]>,
    tmp: Vec<[f32; 3]>,
    uv: Vec<(f32, f32)>,
    listener: Listener,
    time: f32,
    frame: u64,
    current: usize,
    previous: Option<(usize, f32)>,
    history: Vec<usize>,
    rng: u64,
    /// Change preset by itself every so often.
    pub auto: bool,
    changed_at: f32,
    pub out: Vec<u32>,
    params: Params,
}

impl GDrop {
    pub fn new(w: usize, h: usize) -> GDrop {
        let mut g = GDrop {
            w: 0,
            h: 0,
            buf: Vec::new(),
            tmp: Vec::new(),
            uv: vec![(0.0, 0.0); (GW + 1) * (GH + 1)],
            listener: Listener::default(),
            time: 0.0,
            frame: 0,
            current: 0,
            previous: None,
            history: Vec::new(),
            rng: 0x9E37_79B9_7F4A_7C15,
            auto: true,
            changed_at: 0.0,
            out: Vec::new(),
            params: Params::default(),
        };
        g.resize(w, h);
        g
    }

    pub fn size(&self) -> (usize, usize) {
        (self.w, self.h)
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        let (w, h) = (w.max(16), h.max(12));
        if (w, h) == (self.w, self.h) {
            return;
        }
        self.w = w;
        self.h = h;
        self.buf = vec![[0.0; 3]; w * h];
        self.tmp = vec![[0.0; 3]; w * h];
        self.out = vec![0xFF00_0000; w * h];
    }

    pub fn preset_name(&self) -> &'static str {
        PRESETS[self.current].name
    }

    fn rand(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    /// Switches preset; `hard` cuts at once, otherwise it blends.
    pub fn select(&mut self, i: usize, hard: bool) {
        let i = i % PRESETS.len();
        if i == self.current {
            return;
        }
        self.history.push(self.current);
        self.previous = if hard { None } else { Some((self.current, self.time)) };
        self.current = i;
        self.changed_at = self.time;
    }

    pub fn next(&mut self, hard: bool) {
        self.select(self.current + 1, hard);
    }

    pub fn back(&mut self) {
        if let Some(p) = self.history.pop() {
            self.previous = None;
            self.current = p;
            self.changed_at = self.time;
        }
    }

    pub fn random(&mut self, hard: bool) {
        let n = PRESETS.len() as u64;
        let mut i = (self.rand() % n) as usize;
        if i == self.current {
            i = (i + 1) % n as usize;
        }
        self.select(i, hard);
    }

    fn params_for(&self, i: usize, f: &Features) -> Params {
        let mut p = Params::default();
        (PRESETS[i].frame)(f, &mut p);
        p
    }

    /// Where each mesh point pulls its colour from, for preset `pi`.
    fn mesh(&self, pi: usize, f: &Features, p: &Params, out: &mut [(f32, f32)]) {
        let aspect = self.w as f32 / self.h as f32;
        let t = f.time * p.warp_speed;
        let inv = 1.0 / p.warp_scale.max(0.01);
        let fw = [
            11.68 + 4.0 * (t * 1.413 + 10.0).cos(),
            8.77 + 3.0 * (t * 1.113 + 7.0).cos(),
            10.54 + 3.0 * (t * 1.233 + 3.0).cos(),
            11.49 + 4.0 * (t * 0.933 + 5.0).cos(),
        ];
        let point = PRESETS[pi].point;
        for j in 0..=GH {
            for i in 0..=GW {
                let x = i as f32 / GW as f32;
                let y = j as f32 / GH as f32;
                let (xo, yo) = (x * 2.0 - 1.0, y * 2.0 - 1.0);
                let rad = ((xo * aspect).powi(2) + yo * yo).sqrt() / aspect.hypot(1.0) * 2f32.sqrt();
                let ang = yo.atan2(xo * aspect);
                let mut pt = Point { zoom: p.zoom, rot: p.rot, dx: p.dx, dy: p.dy, warp: p.warp };
                if let Some(pf) = point {
                    pf(f, p, x, y, rad, ang, &mut pt);
                }
                // Zoom (stronger towards the edge with zoom_exp), rotate,
                // stretch, wobble, drift.
                let z = pt.zoom.max(0.01).powf(p.zoom_exp.powf(rad * 2.0 - 1.0));
                let (mut u, mut v) = ((x - p.cx) * aspect / z, (y - p.cy) / z);
                let (sn, cs) = pt.rot.sin_cos();
                (u, v) = (u * cs - v * sn, u * sn + v * cs);
                u /= p.sx;
                v /= p.sy;
                let wa = pt.warp * 0.0035;
                u += wa * (t * 0.333 + inv * (xo * fw[0] - yo * fw[3])).sin();
                v += wa * (t * 0.375 - inv * (xo * fw[2] + yo * fw[1])).cos();
                u += wa * (t * 0.753 - inv * (xo * fw[1] - yo * fw[2])).cos();
                v += wa * (t * 0.825 + inv * (xo * fw[0] + yo * fw[3])).sin();
                out[j * (GW + 1) + i] = (u / aspect + p.cx - pt.dx, v + p.cy - pt.dy);
            }
        }
    }

    /// One frame from the latest samples (`wave`, mono) and spectrum
    /// (magnitudes up to half of `rate`). `dt` is seconds since the last.
    pub fn render(&mut self, wave: &[f32], spectrum: &[f32], rate: u32, dt: f32) -> &[u32] {
        let dt = dt.clamp(0.0, 0.2);
        self.time += dt;
        self.frame += 1;
        let (rel, att, beat, vol) = self.listener.hear(spectrum, rate, self.time, dt);
        let f = Features { time: self.time, frame: self.frame, bass: rel[0], mid: rel[1], treb: rel[2], bass_att: att[0], mid_att: att[1], treb_att: att[2], beat, vol };
        // Change by itself every 20-40 s, preferably on a beat.
        if self.auto && self.time - self.changed_at > 20.0 && (beat || self.time - self.changed_at > 40.0) {
            self.random(false);
        }
        let p = self.params_for(self.current, &f);
        let mut uv = std::mem::take(&mut self.uv);
        self.mesh(self.current, &f, &p, &mut uv);
        // A soft change: blend the old preset's mesh and look in.
        let mut blend = 1.0;
        if let Some((prev, start)) = self.previous {
            blend = ((self.time - start) / BLEND).clamp(0.0, 1.0);
            if blend >= 1.0 {
                self.previous = None;
            } else {
                let pp = self.params_for(prev, &f);
                let mut old = vec![(0.0, 0.0); uv.len()];
                self.mesh(prev, &f, &pp, &mut old);
                let s = blend * blend * (3.0 - 2.0 * blend);
                for (a, b) in uv.iter_mut().zip(&old) {
                    a.0 = b.0 + (a.0 - b.0) * s;
                    a.1 = b.1 + (a.1 - b.1) * s;
                }
                self.params = lerp_params(&pp, &p, s);
            }
        }
        if self.previous.is_none() {
            self.params = p;
        }
        let p = self.params;
        self.warp(&uv, &p);
        self.uv = uv;
        self.draw_border(&p);
        for s in p.shapes {
            if s.on {
                self.draw_shape(&s);
            }
        }
        self.draw_wave(wave, spectrum, &p);
        let _ = blend;
        self.present(&p);
        &self.out
    }

    /// The feedback: each pixel pulls from where the mesh says, fading.
    fn warp(&mut self, uv: &[(f32, f32)], p: &Params) {
        let (w, h) = (self.w, self.h);
        let decay = [p.decay * p.tint[0], p.decay * p.tint[1], p.decay * p.tint[2]];
        for y in 0..h {
            let gy = y as f32 / (h - 1).max(1) as f32 * GH as f32;
            let j = (gy as usize).min(GH - 1);
            let fy = gy - j as f32;
            for x in 0..w {
                let gx = x as f32 / (w - 1).max(1) as f32 * GW as f32;
                let i = (gx as usize).min(GW - 1);
                let fx = gx - i as f32;
                let a = uv[j * (GW + 1) + i];
                let b = uv[j * (GW + 1) + i + 1];
                let c = uv[(j + 1) * (GW + 1) + i];
                let d = uv[(j + 1) * (GW + 1) + i + 1];
                let u = (a.0 + (b.0 - a.0) * fx) * (1.0 - fy) + (c.0 + (d.0 - c.0) * fx) * fy;
                let v = (a.1 + (b.1 - a.1) * fx) * (1.0 - fy) + (c.1 + (d.1 - c.1) * fx) * fy;
                let px = self.sample(u * w as f32 - 0.5, v * h as f32 - 0.5, p.wrap);
                self.tmp[y * w + x] = [px[0] * decay[0], px[1] * decay[1], px[2] * decay[2]];
            }
        }
        std::mem::swap(&mut self.buf, &mut self.tmp);
    }

    #[inline]
    fn sample(&self, x: f32, y: f32, wrap: bool) -> [f32; 3] {
        let (w, h) = (self.w as i32, self.h as i32);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let at = |xi: i32, yi: i32| -> [f32; 3] {
            let (xi, yi) = if wrap { (xi.rem_euclid(w), yi.rem_euclid(h)) } else { (xi.clamp(0, w - 1), yi.clamp(0, h - 1)) };
            self.buf[(yi * w + xi) as usize]
        };
        let (xi, yi) = (x0 as i32, y0 as i32);
        let (a, b, c, d) = (at(xi, yi), at(xi + 1, yi), at(xi, yi + 1), at(xi + 1, yi + 1));
        let mut o = [0.0; 3];
        for k in 0..3 {
            let top = a[k] + (b[k] - a[k]) * fx;
            let bot = c[k] + (d[k] - c[k]) * fx;
            o[k] = top + (bot - top) * fy;
        }
        o
    }

    fn add(&mut self, x: i32, y: i32, c: [f32; 3], a: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let px = &mut self.buf[y as usize * self.w + x as usize];
        for k in 0..3 {
            px[k] = (px[k] + c[k] * a).min(1.5);
        }
    }

    /// An anti-aliased-ish line (with a soft second pixel when thick).
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: [f32; 3], a: f32, thick: bool) {
        let steps = ((x1 - x0).abs().max((y1 - y0).abs()) as usize).max(1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            let (xi, yi) = (x.round() as i32, y.round() as i32);
            self.add(xi, yi, c, a);
            if thick {
                self.add(xi + 1, yi, c, a * 0.5);
                self.add(xi, yi + 1, c, a * 0.5);
            }
        }
    }

    fn draw_wave(&mut self, wave: &[f32], spectrum: &[f32], p: &Params) {
        if wave.is_empty() || p.wave_alpha <= 0.0 {
            return;
        }
        let (w, h) = (self.w as f32, self.h as f32);
        let (cx, cy) = (p.wave_x * w, (1.0 - p.wave_y) * h);
        let n = 128usize;
        let samp = |i: usize| wave[i * wave.len() / n].clamp(-1.0, 1.0) * p.wave_scale;
        let c = p.wave;
        let a = p.wave_alpha;
        let pts: Vec<(f32, f32)> = match p.wave_mode {
            // A circle that breathes with the waveform.
            0 => (0..=n)
                .map(|i| {
                    let ang = i as f32 / n as f32 * std::f32::consts::TAU;
                    let r = h * (0.25 + 0.12 * samp(i % n));
                    (cx + r * ang.cos(), cy + r * ang.sin())
                })
                .collect(),
            // A horizontal oscilloscope.
            1 => (0..n).map(|i| (i as f32 / (n - 1) as f32 * w, cy + samp(i) * h * 0.3)).collect(),
            // Spectrum mirrored from the centre line.
            2 => {
                let m = spectrum.len().min(n).max(1);
                let mut v: Vec<(f32, f32)> = (0..m).map(|i| (cx + i as f32 / m as f32 * w * 0.5, cy - spectrum[i].sqrt() * h * 0.9 * p.wave_scale)).collect();
                let mirror: Vec<(f32, f32)> = v.iter().rev().map(|(x, y)| (2.0 * cx - x, *y)).collect();
                let mut out = mirror;
                out.append(&mut v);
                out
            }
            // Dots: each sample against the next (a Lissajous cloud).
            3 => {
                for i in 0..n - 4 {
                    let (x, y) = (cx + samp(i) * h * 0.4, cy + samp(i + 3) * h * 0.4);
                    self.add(x as i32, y as i32, c, a);
                    self.add(x as i32 + 1, y as i32, c, a * 0.5);
                }
                Vec::new()
            }
            // A vertical oscilloscope.
            4 => (0..n).map(|i| (cx + samp(i) * w * 0.25, i as f32 / (n - 1) as f32 * h)).collect(),
            // Two mirrored scopes (a butterfly).
            _ => {
                let one: Vec<(f32, f32)> = (0..n).map(|i| (i as f32 / (n - 1) as f32 * w, cy + samp(i).abs() * h * 0.35)).collect();
                for win in one.windows(2) {
                    let (a0, b0) = (win[0], win[1]);
                    self.line(a0.0, 2.0 * cy - a0.1, b0.0, 2.0 * cy - b0.1, c, a, p.wave_thick);
                }
                one
            }
        };
        for win in pts.windows(2) {
            self.line(win[0].0, win[0].1, win[1].0, win[1].1, c, a, p.wave_thick);
        }
    }

    fn draw_shape(&mut self, s: &Shape) {
        let (w, h) = (self.w as f32, self.h as f32);
        let (cx, cy) = (s.x * w, (1.0 - s.y) * h);
        let r = s.rad * h;
        let x0 = (cx - r).floor().max(0.0) as i32;
        let x1 = (cx + r).ceil().min(w - 1.0) as i32;
        let y0 = (cy - r).floor().max(0.0) as i32;
        let y1 = (cy + r).ceil().min(h - 1.0) as i32;
        let sides = s.sides.max(3) as f32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                let d = dx.hypot(dy);
                // Polygon edge: distance to the nearest side.
                let a = dy.atan2(dx) - s.ang;
                let sector = std::f32::consts::TAU / sides;
                let edge = r * (std::f32::consts::PI / sides).cos() / ((a.rem_euclid(sector) - sector / 2.0).cos());
                if d < edge {
                    let fall = 1.0 - d / edge;
                    self.add(x, y, s.color, s.alpha * fall);
                }
            }
        }
    }

    fn draw_border(&mut self, p: &Params) {
        if p.border <= 0.0 {
            return;
        }
        let (w, h) = (self.w as i32, self.h as i32);
        let b = ((p.border * h as f32) as i32).max(1);
        for y in 0..h {
            for x in 0..w {
                if x < b || y < b || x >= w - b || y >= h - b {
                    self.buf[(y * w + x) as usize] = p.border_color;
                }
            }
        }
    }

    fn present(&mut self, p: &Params) {
        let g = 1.0 / p.gamma.max(0.1);
        for (o, px) in self.out.iter_mut().zip(&self.buf) {
            let mut c = [0u32; 3];
            for k in 0..3 {
                let mut v = px[k].clamp(0.0, 1.0);
                if g != 1.0 {
                    v = v.powf(g);
                }
                if p.invert {
                    v = 1.0 - v;
                }
                c[k] = (v * 255.0) as u32;
            }
            *o = 0xFF00_0000 | c[0] << 16 | c[1] << 8 | c[2];
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

/// Numbers blend; switches flip half-way.
fn lerp_params(a: &Params, b: &Params, t: f32) -> Params {
    let mut o = if t < 0.5 { *a } else { *b };
    o.decay = lerp(a.decay, b.decay, t);
    o.tint = lerp3(a.tint, b.tint, t);
    o.wave = lerp3(a.wave, b.wave, t);
    o.wave_alpha = lerp(a.wave_alpha, b.wave_alpha, t);
    o.wave_scale = lerp(a.wave_scale, b.wave_scale, t);
    o.border = lerp(a.border, b.border, t);
    o.border_color = lerp3(a.border_color, b.border_color, t);
    o.gamma = lerp(a.gamma, b.gamma, t);
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn music(t: f32) -> (Vec<f32>, Vec<f32>) {
        let wave: Vec<f32> = (0..512).map(|i| ((i as f32 * 0.07 + t * 10.0).sin() * 0.6)).collect();
        let mut spec = vec![0.01; 256];
        // A kick every half second.
        if (t * 2.0).fract() < 0.1 {
            spec[2] = 0.8;
            spec[3] = 0.6;
        }
        spec[40] = 0.2;
        (wave, spec)
    }

    #[test]
    fn every_preset_renders_something() {
        for (i, p) in PRESETS.iter().enumerate() {
            let mut g = GDrop::new(96, 72);
            g.auto = false;
            g.select(i, true);
            let mut lit = 0;
            for f in 0..90 {
                let (w, s) = music(f as f32 / 30.0);
                let out = g.render(&w, &s, 44100, 1.0 / 30.0);
                lit = out.iter().filter(|p| [**p >> 16, **p >> 8, **p].iter().any(|c| c & 0xFF > 16)).count();
            }
            assert!(lit > 96 * 72 / 50, "preset {} ({}) is too dark: {lit}", i, p.name);
            assert!(g.out.iter().all(|p| p >> 24 == 0xFF));
        }
    }

    #[test]
    fn presets_change_softly_and_back() {
        let mut g = GDrop::new(64, 48);
        g.auto = false;
        let first = g.preset_name();
        g.next(false);
        assert_ne!(g.preset_name(), first);
        g.back();
        assert_eq!(g.preset_name(), first);
        for f in 0..10 {
            let (w, s) = music(f as f32 / 30.0);
            g.render(&w, &s, 44100, 1.0 / 30.0);
        }
        g.resize(128, 96);
        assert_eq!(g.out.len(), 128 * 96);
    }
}
