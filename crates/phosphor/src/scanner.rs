//! Stage scanner (MadMapper's Spatial Scanner): the projector shows black, white and Gray-code stripes
//! (`Engine::set_scan`), a camera watches the stage (any layer showing a camera, read back like a preview window),
//! and each camera pixel learns which projector pixel lit it. The camera's picture is then redrawn as the
//! projector sees the stage, and becomes that output's stage photo: draw slices straight onto the real objects.
//!
//! One pattern at a time: show it, wait `settle` for the projector and camera to catch up, read one camera frame,
//! next. Decoding runs on a worker thread. `decode` is plain arithmetic (tested with a simulated camera).

use std::sync::mpsc;

/// Monitor-readback tag for the scanner's camera frames.
pub const MONITOR_KEY: u8 = 252;
/// Width the camera picture is read back at (the photo is this wide at most).
const READ_WIDTH: u32 = 960;
/// A camera pixel needs this much difference between the white and black frames to count (0–255).
const MIN_CONTRAST: i32 = 24;

/// One picture the projector shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pattern {
    Black,
    White,
    /// Bit `bit` of the Gray code of each pixel's column (`rows` = false) or row, normal or inverse.
    Bit { rows: bool, bit: u32, inverse: bool },
}

impl Pattern {
    /// `fs_scan`'s parameters.
    pub fn uniforms(self) -> [f32; 4] {
        match self {
            Pattern::Black => [0.0; 4],
            Pattern::White => [1.0, 0.0, 0.0, 0.0],
            Pattern::Bit { rows, bit, inverse } => [2.0, bit as f32, rows as u32 as f32, inverse as u32 as f32],
        }
    }
}

/// Bits worth showing for `len` projector pixels when the camera sees about `seen` across: the finest stripes the
/// camera can't tell apart are left out. Returns (bits, finest bit shown).
fn bits_for(len: u32, seen: u32) -> (u32, u32) {
    let bits = 32 - (len.max(2) - 1).leading_zeros();
    let skip = (len / seen.max(1)).max(1).ilog2().min(bits - 1);
    (bits, skip)
}

/// Every pattern for a projector of `size`, seen by a camera `cam_width` wide: black, white, then each Gray bit
/// (coarse to fine) with its inverse, columns then rows.
pub fn patterns(size: (u32, u32), cam_width: u32) -> Vec<Pattern> {
    let mut list = vec![Pattern::Black, Pattern::White];
    for (rows, len) in [(false, size.0), (true, size.1)] {
        let (bits, skip) = bits_for(len, cam_width * len / size.0.max(1));
        for bit in (skip..bits).rev() {
            list.push(Pattern::Bit { rows, bit, inverse: false });
            list.push(Pattern::Bit { rows, bit, inverse: true });
        }
    }
    list
}

/// What a scan found: for each camera pixel, the projector pixel that lit it (None = not lit / unsure), and the
/// stage as the projector sees it.
pub struct Decoded {
    pub map: Vec<Option<(u32, u32)>>,
    /// The stage photo in projector space (`photo_size`, RGBA), lit by the white frame.
    pub photo: image::RgbaImage,
    /// Share of camera pixels that saw the projector.
    pub coverage: f32,
}

/// Decodes the frames (one luma image per pattern, in `patterns` order; `white_rgba` = the white frame in colour)
/// into projector coordinates, and redraws the camera's picture in projector space at `photo_size`.
pub fn decode(patterns: &[Pattern], frames: &[Vec<u8>], white_rgba: &[u8], cam: (u32, u32), projector: (u32, u32), photo_size: (u32, u32)) -> Decoded {
    let n = (cam.0 * cam.1) as usize;
    let at = |p: Pattern| patterns.iter().position(|q| *q == p).and_then(|i| frames.get(i)).filter(|f| f.len() >= n);
    let (Some(black), Some(white)) = (at(Pattern::Black), at(Pattern::White)) else {
        return Decoded { map: vec![None; n], photo: image::RgbaImage::new(photo_size.0.max(1), photo_size.1.max(1)), coverage: 0.0 };
    };
    // Per axis: each bit shown, with its normal and inverse frame; the finest bit shown.
    let axes: Vec<(Vec<(u32, &Vec<u8>, &Vec<u8>)>, u32)> = [false, true]
        .into_iter()
        .map(|rows| {
            let bits: Vec<(u32, &Vec<u8>, &Vec<u8>)> = patterns
                .iter()
                .filter_map(|p| match *p {
                    Pattern::Bit { rows: r, bit, inverse: false } if r == rows => Some((bit, at(*p)?, at(Pattern::Bit { rows, bit, inverse: true })?)),
                    _ => None,
                })
                .collect();
            let finest = bits.iter().map(|b| b.0).min().unwrap_or(0);
            (bits, finest)
        })
        .collect();
    let mut map = vec![None; n];
    let mut lit = 0usize;
    for (i, slot) in map.iter_mut().enumerate() {
        if (white[i] as i32 - black[i] as i32) < MIN_CONTRAST {
            continue;
        }
        let mut coords = [0u32; 2];
        let mut sure = true;
        for (axis, (bits, finest)) in axes.iter().enumerate() {
            let mut gray = 0u32;
            for (bit, on, off) in bits {
                let d = on[i] as i32 - off[i] as i32;
                // Too close to call on a coarse stripe: this pixel can't be trusted (the finest ones may blur).
                if d.abs() < 4 && *bit >= finest + 2 {
                    sure = false;
                }
                if d > 0 {
                    gray |= 1 << bit;
                }
            }
            // Gray → binary, then the middle of the finest stripe that was shown.
            let mut v = gray;
            let mut shift = gray >> 1;
            while shift != 0 {
                v ^= shift;
                shift >>= 1;
            }
            coords[axis] = v + if *finest == 0 { 0 } else { (1 << finest) / 2 };
        }
        if sure && coords[0] < projector.0 && coords[1] < projector.1 {
            *slot = Some((coords[0], coords[1]));
            lit += 1;
        }
    }
    let photo = redraw(&map, white_rgba, projector, photo_size);
    Decoded { map, photo, coverage: lit as f32 / n.max(1) as f32 }
}

/// The camera's picture moved into projector space: each lit camera pixel lands where it was lit from (averaged),
/// and the small gaps between are filled from their neighbours. Unlit areas stay black.
fn redraw(map: &[Option<(u32, u32)>], rgba: &[u8], projector: (u32, u32), (pw, ph): (u32, u32)) -> image::RgbaImage {
    let (pw, ph) = (pw.max(1), ph.max(1));
    let mut sum = vec![[0u32; 4]; (pw * ph) as usize];
    for (i, m) in map.iter().enumerate() {
        let (Some((x, y)), Some(c)) = (m, rgba.get(i * 4..i * 4 + 4)) else { continue };
        let (px, py) = ((*x as u64 * pw as u64 / projector.0.max(1) as u64) as u32, (*y as u64 * ph as u64 / projector.1.max(1) as u64) as u32);
        let s = &mut sum[(py.min(ph - 1) * pw + px.min(pw - 1)) as usize];
        for k in 0..3 {
            s[k] += c[k] as u32;
        }
        s[3] += 1;
    }
    // Fill holes a few pixels wide from lit neighbours (camera pixels are sparser than the photo's in places).
    for _ in 0..3 {
        let before = sum.clone();
        for y in 0..ph as i32 {
            for x in 0..pw as i32 {
                let i = (y as u32 * pw + x as u32) as usize;
                if before[i][3] > 0 {
                    continue;
                }
                let mut acc = [0u32; 4];
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx >= 0 && ny >= 0 && nx < pw as i32 && ny < ph as i32 {
                        let n = before[(ny as u32 * pw + nx as u32) as usize];
                        if n[3] > 0 {
                            for k in 0..3 {
                                acc[k] += n[k] / n[3];
                            }
                            acc[3] += 1;
                        }
                    }
                }
                if acc[3] >= 2 {
                    sum[i] = acc;
                }
            }
        }
    }
    image::RgbaImage::from_fn(pw, ph, |x, y| {
        let s = sum[(y * pw + x) as usize];
        if s[3] == 0 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([(s[0] / s[3]) as u8, (s[1] / s[3]) as u8, (s[2] / s[3]) as u8, 255]) }
    })
}

fn luma(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4).map(|p| ((p[0] as u32 * 54 + p[1] as u32 * 183 + p[2] as u32 * 19) >> 8) as u8).collect()
}

/// Where a scan is.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    /// Showing pattern `step` of `of`.
    Scanning { step: usize, of: usize },
    Decoding,
    /// The photo was saved and set on the output; how much of the camera picture saw the projector, and (for the
    /// test that scans an output with itself) the worst error in projector pixels.
    Done { coverage: f32, path: String, error: Option<f32> },
    Failed(String),
}

/// Showing pictures one at a time and reading the camera once each has settled: the stepping both scanners share
/// (the stage scanner's stripes, the LED scanner's lights). Frames are kept as luma, in step order.
pub struct Steps {
    /// Monitor source id the camera picture is read from (a camera layer, or an output for a test).
    pub camera: u64,
    /// Size the camera picture is read back at.
    pub cam: (u32, u32),
    pub count: usize,
    pub step: usize,
    shown_at: f64,
    asked: bool,
    pub frames: Vec<Vec<u8>>,
}

impl Steps {
    /// `count` pictures, read from monitor source `camera` whose picture is `aspect` (width / height).
    pub fn new(camera: u64, aspect: f32, count: usize) -> Self {
        let cam = (READ_WIDTH, ((READ_WIDTH as f32 / aspect.max(0.1)).round() as u32).clamp(16, 2048));
        Steps { camera, cam, count, step: 0, shown_at: f64::NAN, asked: false, frames: Vec::new() }
    }

    /// Read the camera this frame? (Once per picture, `settle` seconds after it went up.)
    pub fn due(&mut self, now: f64, settle: f64) -> bool {
        if self.shown_at.is_nan() {
            self.shown_at = now;
        }
        if self.asked || self.done() || now - self.shown_at < settle {
            return false;
        }
        self.asked = true;
        true
    }

    /// A camera frame came back: keep it and move to the next picture. False when it wasn't asked for (or is the
    /// wrong size).
    pub fn take(&mut self, rgba: &[u8], width: u32, height: u32) -> bool {
        if !self.asked || (width, height) != self.cam {
            return false;
        }
        self.frames.push(luma(rgba));
        self.step += 1;
        self.asked = false;
        self.shown_at = f64::NAN;
        true
    }

    pub fn done(&self) -> bool {
        self.step >= self.count
    }
}

/// What feeds the scan.
pub struct Scan {
    pub output: u64,
    projector: (u32, u32),
    patterns: Vec<Pattern>,
    steps: Steps,
    white_rgba: Vec<u8>,
}

/// The stage scanner: one scan at a time.
pub struct Scanner {
    pub scan: Option<Scan>,
    pub status: Status,
    /// Seconds each pattern is shown before the camera frame is taken (projector + camera delay).
    pub settle: f64,
    decoding: Option<mpsc::Receiver<Result<(f32, String, Option<f32>), String>>>,
    /// The output being decoded.
    pending_output: Option<u64>,
    /// The finished photo for an output: (output id, path), for the app to set.
    pub finished: Option<(u64, String)>,
}

impl Default for Scanner {
    fn default() -> Self {
        Scanner { scan: None, status: Status::Idle, settle: 0.4, decoding: None, pending_output: None, finished: None }
    }
}

impl Scanner {
    /// Starts scanning output `output` (`projector` = its size in pixels) with the camera picture from monitor
    /// source `camera` (`aspect` = its width / height).
    pub fn start(&mut self, output: u64, projector: (u32, u32), camera: u64, aspect: f32) {
        let steps = Steps::new(camera, aspect, 0);
        let patterns = patterns(projector, steps.cam.0);
        self.status = Status::Scanning { step: 0, of: patterns.len() };
        self.scan = Some(Scan { output, projector, steps: Steps { count: patterns.len(), ..steps }, patterns, white_rgba: Vec::new() });
    }

    pub fn cancel(&mut self) {
        self.scan = None;
        self.status = Status::Idle;
    }

    /// This frame: the pattern to show (for `Engine::set_scan`).
    pub fn pattern(&self) -> Option<(u64, [f32; 4])> {
        let s = self.scan.as_ref()?;
        Some((s.output, s.patterns.get(s.steps.step)?.uniforms()))
    }

    /// The camera picture to read back (monitor source, width, height) while scanning.
    pub fn picture(&self) -> Option<(u64, u32, u32)> {
        self.scan.as_ref().map(|s| (s.steps.camera, s.steps.cam.0, s.steps.cam.1))
    }

    /// Read the camera this frame? (Once per pattern, after it has settled.)
    pub fn due(&mut self, now: f64) -> bool {
        let settle = self.settle;
        self.scan.as_mut().is_some_and(|s| s.steps.due(now, settle))
    }

    /// A camera frame came back: keep it, show the next pattern; after the last one, decode (worker thread).
    pub fn give(&mut self, rgba: Vec<u8>, width: u32, height: u32) {
        let Some(s) = self.scan.as_mut() else { return };
        let white = s.patterns.get(s.steps.step) == Some(&Pattern::White);
        if !s.steps.take(&rgba, width, height) {
            return;
        }
        if white {
            s.white_rgba = rgba;
        }
        if !s.steps.done() {
            self.status = Status::Scanning { step: s.steps.step, of: s.patterns.len() };
            return;
        }
        let s = self.scan.take().unwrap();
        self.status = Status::Decoding;
        let (tx, rx) = mpsc::channel();
        let output = s.output;
        let spawned = std::thread::Builder::new().name("stage scan".into()).spawn(move || {
            let cam = s.steps.cam;
            let photo_size = (cam.0.min(s.projector.0), (cam.0.min(s.projector.0) * s.projector.1 / s.projector.0.max(1)).max(1));
            let d = decode(&s.patterns, &s.steps.frames, &s.white_rgba, cam, s.projector, photo_size);
            // Scanning an output with itself: every camera pixel must land on the projector pixel under it.
            let error = (s.steps.camera == s.output).then(|| {
                let sx = s.projector.0 as f32 / cam.0 as f32;
                let sy = s.projector.1 as f32 / cam.1 as f32;
                d.map
                    .iter()
                    .enumerate()
                    .filter_map(|(i, m)| {
                        let (x, y) = (*m)?;
                        let (cx, cy) = ((i as u32 % cam.0) as f32 + 0.5, (i as u32 / cam.0) as f32 + 0.5);
                        Some((x as f32 - cx * sx).abs().max((y as f32 - cy * sy).abs()))
                    })
                    .fold(0.0f32, f32::max)
            });
            if d.coverage < 0.01 {
                let _ = tx.send(Err("the camera didn't see the projector's light (is it pointed at the stage, and the room dark enough?)".to_string()));
                return;
            }
            let result = (|| {
                let dir = crate::show::storage_dir().ok_or("no folder to save the photo in")?.join("scans");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let path = dir.join(format!("output-{output}-{when}.png"));
                d.photo.save(&path).map_err(|e| e.to_string())?;
                Ok::<_, String>(path.to_string_lossy().to_string())
            })();
            let _ = tx.send(result.map(|p| (d.coverage, p, error)));
        });
        match spawned {
            Ok(_) => {
                self.decoding = Some(rx);
                self.pending_output = Some(output);
            }
            Err(e) => self.status = Status::Failed(e.to_string()),
        }
    }

    /// Picks up the decoded photo (call every frame; cheap).
    pub fn poll(&mut self) {
        let Some(rx) = &self.decoding else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.decoding = None;
        match result {
            Ok((coverage, path, error)) => {
                self.status = Status::Done { coverage, path: path.clone(), error };
                self.finished = self.pending_output.take().map(|o| (o, path));
            }
            Err(e) => self.status = Status::Failed(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A camera looking at a 640×360 projector picture through a perspective (and a blur), 320×240 pixels.
    fn camera_view(pattern: Pattern, proj: (u32, u32), cam: (u32, u32)) -> Vec<u8> {
        let draw = |x: f32, y: f32| -> f32 {
            if x < 0.0 || y < 0.0 || x >= proj.0 as f32 || y >= proj.1 as f32 {
                return 10.0; // room light outside the projection
            }
            let (px, py) = (x as u32, y as u32);
            let on = match pattern {
                Pattern::Black => false,
                Pattern::White => true,
                Pattern::Bit { rows, bit, inverse } => {
                    let v = if rows { py } else { px };
                    (((v ^ (v >> 1)) >> bit) & 1 == 1) != inverse
                }
            };
            if on { 220.0 } else { 15.0 }
        };
        let mut out = Vec::new();
        for cy in 0..cam.1 {
            for cx in 0..cam.0 {
                // Camera → projector: a keystoned view, the projection filling most of the frame.
                let (u, v) = (cx as f32 / cam.0 as f32, cy as f32 / cam.1 as f32);
                let w = 1.0 + 0.25 * v;
                let x = ((u - 0.1) * 1.25 / w * 1.1 + 0.02) * proj.0 as f32;
                let y = (v - 0.05) * 1.15 * proj.1 as f32;
                // A little blur: average of four samples a quarter pixel apart.
                let s = (draw(x - 0.3, y) + draw(x + 0.3, y) + draw(x, y - 0.3) + draw(x, y + 0.3)) / 4.0;
                out.push(s as u8);
            }
        }
        out
    }

    #[test]
    fn a_simulated_camera_decodes_back_to_the_projector_pixels() {
        let (proj, cam) = ((640, 360), (320, 240));
        let pats = patterns(proj, cam.0);
        assert_eq!(pats[0], Pattern::Black);
        assert!(pats.len() < 40, "the finest stripes the camera can't see are skipped");
        let frames: Vec<Vec<u8>> = pats.iter().map(|p| camera_view(*p, proj, cam)).collect();
        let white: Vec<u8> = frames[1].iter().flat_map(|l| [*l, *l, *l, 255]).collect();
        let d = decode(&pats, &frames, &white, cam, proj, (320, 180));
        assert!(d.coverage > 0.6, "most of the frame sees the projection ({})", d.coverage);
        // Every decoded pixel lands within a couple of projector pixels of where the simulation says.
        let mut worst = 0.0f32;
        for cy in 0..cam.1 {
            for cx in 0..cam.0 {
                let Some((x, y)) = d.map[(cy * cam.0 + cx) as usize] else { continue };
                let (u, v) = (cx as f32 / cam.0 as f32, cy as f32 / cam.1 as f32);
                let w = 1.0 + 0.25 * v;
                let ex = ((u - 0.1) * 1.25 / w * 1.1 + 0.02) * proj.0 as f32;
                let ey = (v - 0.05) * 1.15 * proj.1 as f32;
                worst = worst.max((x as f32 - ex).abs().max((y as f32 - ey).abs()));
            }
        }
        assert!(worst <= 3.0, "decoded within {worst} px");
        // Outside the projection nothing is decoded.
        assert!(d.map[0].is_none(), "top-left corner is outside the projection");
        assert_eq!(d.photo.dimensions(), (320, 180));
    }

    #[test]
    fn the_scanner_steps_through_its_patterns() {
        let mut s = Scanner { settle: 0.0, ..Default::default() };
        s.start(7, (64, 32), 1, 2.0);
        let n = s.scan.as_ref().unwrap().patterns.len();
        assert_eq!(s.pattern().unwrap().0, 7);
        for step in 0..n {
            assert!(s.due(step as f64), "a frame is wanted once the pattern has settled");
            assert!(!s.due(step as f64), "only one per pattern");
            let (_, w, h) = s.picture().unwrap();
            s.give(vec![0; (w * h * 4) as usize], w, h);
        }
        assert!(s.scan.is_none());
        assert_eq!(s.status, Status::Decoding);
    }
}
