//! LED scanner (MadMapper's): the lights themselves show coded on / off pictures over DMX while a camera watches,
//! and each light is found where the camera saw it. Picture 0 has every light off, picture 1 every light on, then
//! for each bit of a light's number a picture where the lights with that bit set are on, and its inverse, and last
//! all off again (a camera pixel that changed between the two all-off pictures saw something move, not a light). A
//! camera pixel that a light brightens reads that light's number back bit by bit; each light lands at the middle of
//! its brightest pixels. The fixtures then sample the picture at those places ("Found by camera"), so the picture
//! lands on the lights the way the camera sees them.
//!
//! The stepping (show, settle, read one camera frame) is the stage scanner's `Steps`; decoding is plain arithmetic
//! on a worker thread (tested with a simulated camera).

use std::collections::BTreeMap;
use std::sync::mpsc;

use phosphor_engine::{DmxSettings, Fixture, FixtureShape};

use crate::scanner::Steps;

/// Monitor-readback tag for the LED scanner's camera frames.
pub const MONITOR_KEY: u8 = 249;
/// A camera pixel needs this much more light with every light on than with all off to count (0–255).
const MIN_CONTRAST: i32 = 24;

/// Bits that number `total` lights.
pub fn bits(total: usize) -> u32 {
    (usize::BITS - total.max(2).saturating_sub(1).leading_zeros()).max(1)
}

/// Pictures a scan of `total` lights shows.
pub fn picture_count(total: usize) -> usize {
    3 + 2 * bits(total) as usize
}

/// Whether light `index` (of a scan numbering `index < 2^bits`) is on in picture `step`. The last picture's "bit" is
/// past every number: all off.
pub fn lit(step: usize, index: usize) -> bool {
    match step {
        0 => false,
        1 => true,
        k => {
            let (bit, inverse) = ((k - 2) / 2, (k - 2) % 2 == 1);
            ((index >> bit) & 1 == 1) != inverse
        }
    }
}

/// One fixture being scanned: its lights are numbered `first..first + count`.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub fixture: u64,
    pub first: usize,
    pub count: usize,
}

/// The fixtures a scan covers (`only` = one fixture, else every LED fixture that's on; moving heads aren't scanned).
pub fn targets(dmx: &DmxSettings, only: Option<u64>) -> Vec<Target> {
    let mut first = 0;
    dmx.fixtures
        .iter()
        .filter(|f| f.enabled && f.shape != FixtureShape::MovingHead && only.is_none_or(|id| id == f.id))
        .map(|f| {
            let t = Target { fixture: f.id, first, count: f.pixel_count() };
            first += t.count;
            t
        })
        .collect()
}

/// The DMX for picture `step`: the scanned lights full white or off, every other LED fixture off.
pub fn universes(dmx: &DmxSettings, targets: &[Target], step: usize) -> BTreeMap<u16, [u8; 512]> {
    let mut out = BTreeMap::new();
    for f in dmx.fixtures.iter().filter(|f| f.enabled && f.shape != FixtureShape::MovingHead) {
        let t = targets.iter().find(|t| t.fixture == f.id);
        let colors: Vec<[u8; 4]> = (0..f.pixel_count()).map(|i| if t.is_some_and(|t| lit(step, t.first + i)) { [255; 4] } else { [0, 0, 0, 255] }).collect();
        // Full white whatever the fixture's brightness and gamma (a dimmed light is harder to see).
        Fixture { brightness: 1.0, gamma: 1.0, positions: Vec::new(), ..f.clone() }.write(&colors, &mut out, 0.0);
    }
    out
}

/// Where the camera saw each of `total` lights (0..1 of the camera picture, y down), from the frames in picture
/// order (luma, `cam` sized). None = not seen.
pub fn decode(frames: &[Vec<u8>], cam: (u32, u32), total: usize) -> Vec<Option<[f32; 2]>> {
    let n = (cam.0 * cam.1) as usize;
    if total == 0 || frames.len() < picture_count(total) || frames.iter().any(|f| f.len() < n) {
        return vec![None; total];
    }
    let (black, white, black_again) = (&frames[0], &frames[1], &frames[2 + 2 * bits(total) as usize]);
    // Every camera pixel a light brightens, with the light's number and how much brighter.
    let mut hits: Vec<(usize, u32, i32)> = Vec::new();
    'pixels: for i in 0..n {
        let c = white[i] as i32 - black[i] as i32;
        // Not brightened by the lights, or something moved here during the scan.
        if c < MIN_CONTRAST || (black_again[i] as i32 - black[i] as i32).abs() > (c / 4).max(12) {
            continue;
        }
        let mut index = 0usize;
        for b in 0..bits(total) as usize {
            let (on, off) = (frames[2 + 2 * b][i] as i32, frames[3 + 2 * b][i] as i32);
            // Too close to call (between two lights, or flicker): leave this pixel out.
            if (on - off).abs() < c / 4 {
                continue 'pixels;
            }
            if on > off {
                index |= 1 << b;
            }
        }
        if index < total {
            hits.push((index, i as u32, c));
        }
    }
    // Each light: its brightest pixels (half its best or more: the LED, not its glow).
    let mut best = vec![0i32; total];
    for &(k, _, c) in &hits {
        best[k] = best[k].max(c);
    }
    let mut pixels: Vec<Vec<(f64, f64, f64)>> = vec![Vec::new(); total];
    for &(k, i, c) in &hits {
        if c * 2 >= best[k] {
            pixels[k].push(((i % cam.0) as f64 + 0.5, (i / cam.0) as f64 + 0.5, c as f64));
        }
    }
    let middle = |px: &[(f64, f64, f64)]| {
        let w: f64 = px.iter().map(|p| p.2).sum();
        (px.iter().map(|p| p.0 * p.2).sum::<f64>() / w, px.iter().map(|p| p.1 * p.2).sum::<f64>() / w)
    };
    // A light is a tight cluster of pixels; pixels that read a light's number by chance (noise, something moving)
    // are scattered. Middle of all, then of the pixels near it (a stray pixel elsewhere doesn't pull it away);
    // too few pixels, or spread too wide, isn't a light.
    let widest = 0.03 * cam.0 as f64;
    pixels
        .iter()
        .map(|px| {
            if px.len() < 2 {
                return None;
            }
            let m = middle(px);
            let mut d: Vec<f64> = px.iter().map(|p| (p.0 - m.0).hypot(p.1 - m.1)).collect();
            d.sort_by(f64::total_cmp);
            let median = d[d.len() / 2];
            if median > widest {
                return None;
            }
            let near: Vec<(f64, f64, f64)> = px.iter().copied().filter(|p| (p.0 - m.0).hypot(p.1 - m.1) <= 3.0 * median + 1.5).collect();
            if near.len() < 2 {
                return None;
            }
            let (x, y) = middle(&near);
            Some([(x / cam.0 as f64) as f32, (y / cam.1 as f64) as f32])
        })
        .collect()
}

/// A fixture's positions from what was found: lights not seen are placed evenly between the seen ones either side,
/// or carry on the line past the last one seen. None when none were seen.
pub fn fill(found: &[Option<[f32; 2]>]) -> Option<Vec<[f32; 2]>> {
    let seen: Vec<usize> = (0..found.len()).filter(|&i| found[i].is_some()).collect();
    if seen.is_empty() {
        return None;
    }
    let at = |i: usize| found[i].unwrap_or_default();
    let lerp = |a: [f32; 2], b: [f32; 2], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    Some(
        (0..found.len())
            .map(|i| {
                if let Some(p) = found[i] {
                    return p;
                }
                let before = seen.iter().rev().find(|&&k| k < i).copied();
                let after = seen.iter().find(|&&k| k > i).copied();
                // Two seen lights to go by: between them, or the line through the nearest two at an end.
                let (a, b) = match (before, after) {
                    (Some(a), Some(b)) => (a, b),
                    (Some(b), None) => (seen.iter().rev().find(|&&k| k < b).copied().unwrap_or(b), b),
                    (None, Some(a)) => (a, seen.iter().find(|&&k| k > a).copied().unwrap_or(a)),
                    (None, None) => unreachable!("at least one light was seen"),
                };
                if a == b {
                    return at(a);
                }
                let p = lerp(at(a), at(b), (i as f32 - a as f32) / (b as f32 - a as f32));
                [p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0)]
            })
            .collect(),
    )
}

/// Where a scan is.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Scanning { step: usize, of: usize },
    Decoding,
    /// Fixtures placed (of those scanned) and lights seen (of all scanned).
    Done { placed: usize, fixtures: usize, seen: usize, total: usize },
    Failed(String),
}

struct Run {
    targets: Vec<Target>,
    total: usize,
    steps: Steps,
}

/// Per fixture: its id and its lights' new positions; plus lights seen.
type Placed = (Vec<(u64, Vec<[f32; 2]>)>, usize);

/// The LED scanner: one scan at a time.
pub struct LedScanner {
    run: Option<Run>,
    pub status: Status,
    /// Seconds each picture shows before the camera frame is taken (DMX + LED + camera delay).
    pub settle: f64,
    decoding: Option<mpsc::Receiver<Placed>>,
    /// Lights and fixtures in the scan being decoded (for the report).
    decoding_total: (usize, usize),
    /// Fixtures to place: (fixture id, positions), for the app to apply.
    pub finished: Option<Vec<(u64, Vec<[f32; 2]>)>>,
}

impl Default for LedScanner {
    fn default() -> Self {
        LedScanner { run: None, status: Status::Idle, settle: 0.3, decoding: None, decoding_total: (0, 0), finished: None }
    }
}

impl LedScanner {
    /// Scans `only` one fixture or every LED fixture that's on, with the camera picture from monitor source `camera`
    /// (`aspect` = its width / height).
    pub fn start(&mut self, dmx: &DmxSettings, only: Option<u64>, camera: u64, aspect: f32) -> Result<(), String> {
        let targets = targets(dmx, only);
        let total: usize = targets.iter().map(|t| t.count).sum();
        if total == 0 {
            return Err("no LED fixtures to find (moving heads aren't scanned)".into());
        }
        let count = picture_count(total);
        self.status = Status::Scanning { step: 0, of: count };
        self.run = Some(Run { targets, total, steps: Steps::new(camera, aspect, count) });
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.run = None;
        self.status = Status::Idle;
    }

    pub fn busy(&self) -> bool {
        self.run.is_some() || self.status == Status::Decoding
    }

    /// What the lights show this frame while scanning (sent instead of the picture).
    pub fn universes(&self, dmx: &DmxSettings) -> Option<BTreeMap<u16, [u8; 512]>> {
        let r = self.run.as_ref()?;
        Some(universes(dmx, &r.targets, r.steps.step))
    }

    /// The camera picture to read back (monitor source, width, height) while scanning.
    pub fn picture(&self) -> Option<(u64, u32, u32)> {
        self.run.as_ref().map(|r| (r.steps.camera, r.steps.cam.0, r.steps.cam.1))
    }

    /// Read the camera this frame?
    pub fn due(&mut self, now: f64) -> bool {
        let settle = self.settle;
        self.run.as_mut().is_some_and(|r| r.steps.due(now, settle))
    }

    /// A camera frame came back: next picture; after the last, decode on a worker thread.
    pub fn give(&mut self, rgba: Vec<u8>, width: u32, height: u32) {
        let Some(r) = self.run.as_mut() else { return };
        if !r.steps.take(&rgba, width, height) {
            return;
        }
        if !r.steps.done() {
            self.status = Status::Scanning { step: r.steps.step, of: r.steps.count };
            return;
        }
        let r = self.run.take().unwrap();
        self.status = Status::Decoding;
        self.decoding_total = (r.total, r.targets.len());
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("led scan".into()).spawn(move || {
            let found = decode(&r.steps.frames, r.steps.cam, r.total);
            let seen = found.iter().filter(|f| f.is_some()).count();
            // A fixture moves only when enough of its lights were seen (a quarter, and two): a few chance finds
            // would throw it across the picture.
            let placed = r
                .targets
                .iter()
                .filter_map(|t| {
                    let lights = &found[t.first..t.first + t.count];
                    let n = lights.iter().filter(|f| f.is_some()).count();
                    (n * 4 >= t.count && n >= 2.min(t.count)).then(|| Some((t.fixture, fill(lights)?))).flatten()
                })
                .collect();
            let _ = tx.send((placed, seen));
        });
        match spawned {
            Ok(_) => self.decoding = Some(rx),
            Err(e) => self.status = Status::Failed(e.to_string()),
        }
    }

    /// Picks up the decoded positions (call every frame; cheap).
    pub fn poll(&mut self) {
        let Some(rx) = &self.decoding else { return };
        let Ok((placed, seen)) = rx.try_recv() else { return };
        self.decoding = None;
        let (total, fixtures) = self.decoding_total;
        if placed.is_empty() {
            self.status = Status::Failed(format!(
                "saw {seen} of {total} lights come on: too few to place anything (is the camera pointed at the lights and still, is LED output reaching them, and is the room dark enough?)"
            ));
            return;
        }
        self.status = Status::Done { placed: placed.len(), fixtures, seen, total };
        self.finished = Some(placed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_light_gets_its_own_code() {
        assert_eq!((bits(1), bits(2), bits(3), bits(4), bits(5), bits(256), bits(257)), (1, 1, 2, 2, 3, 8, 9));
        let total = 37;
        let codes: Vec<Vec<bool>> = (0..total).map(|i| (0..picture_count(total)).map(|s| lit(s, i)).collect()).collect();
        for (i, c) in codes.iter().enumerate() {
            assert!(!c[0] && c[1] && !c[c.len() - 1], "off, on, ... off again");
            assert!(c[2..c.len() - 1].chunks(2).all(|p| p[0] != p[1]), "each bit with its inverse");
            assert!(codes[..i].iter().all(|o| o != c), "light {i}'s code is its own");
        }
    }

    /// A simulated camera: `lights` (0..1 positions) glow as soft dots on a dim room, on or off per picture.
    fn camera(lights: &[[f32; 2]], cam: (u32, u32), step: usize) -> Vec<u8> {
        let mut out = vec![12u8; (cam.0 * cam.1) as usize];
        for (k, l) in lights.iter().enumerate() {
            if !lit(step, k) {
                continue;
            }
            let (lx, ly) = (l[0] * cam.0 as f32, l[1] * cam.1 as f32);
            for y in 0..cam.1 {
                for x in 0..cam.0 {
                    let d2 = (x as f32 + 0.5 - lx).powi(2) + (y as f32 + 0.5 - ly).powi(2);
                    let v = 230.0 * (-d2 / 6.0).exp();
                    let p = &mut out[(y * cam.0 + x) as usize];
                    *p = (*p as f32 + v).min(255.0) as u8;
                }
            }
        }
        out
    }

    #[test]
    fn a_simulated_camera_finds_each_light() {
        let cam = (160, 90);
        // A wavy strip of 20 lights, as hung on a real set (not a straight line).
        let lights: Vec<[f32; 2]> = (0..20).map(|i| [0.1 + 0.04 * i as f32, 0.5 + 0.2 * (i as f32 * 0.5).sin()]).collect();
        let frames: Vec<Vec<u8>> = (0..picture_count(20)).map(|s| camera(&lights, cam, s)).collect();
        let found = decode(&frames, cam, 20);
        for (i, (f, l)) in found.iter().zip(&lights).enumerate() {
            let f = f.unwrap_or_else(|| panic!("light {i} not found"));
            let err = ((f[0] - l[0]) * cam.0 as f32).hypot((f[1] - l[1]) * cam.1 as f32);
            assert!(err < 0.6, "light {i} {err:.2} camera pixels off");
        }
        // A light that's broken (never comes on) isn't found; the rest still are.
        let frames: Vec<Vec<u8>> = (0..picture_count(21)).map(|s| camera(&lights, cam, s)).collect();
        let found = decode(&frames, cam, 21);
        assert!(found[20].is_none() && found[..20].iter().all(|f| f.is_some()));
        // Something moving in front of the camera (here: the room brightening picture by picture) isn't a light.
        let moving: Vec<Vec<u8>> = (0..picture_count(4)).map(|s| vec![(12 + s * 30) as u8; (cam.0 * cam.1) as usize]).collect();
        assert!(decode(&moving, cam, 4).iter().all(|f| f.is_none()));
        // Camera noise / a busy moving picture: pixels read light numbers by chance, scattered; not lights.
        let mut seed = 12345u32;
        let noise: Vec<Vec<u8>> = (0..picture_count(20))
            .map(|_| {
                (0..cam.0 * cam.1)
                    .map(|_| {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        (seed >> 24) as u8
                    })
                    .collect()
            })
            .collect();
        let chance = decode(&noise, cam, 20).iter().filter(|f| f.is_some()).count();
        assert_eq!(chance, 0, "lights found in noise");
        // Nothing lights up: nothing found.
        let dark = vec![vec![12u8; (cam.0 * cam.1) as usize]; picture_count(20)];
        assert!(decode(&dark, cam, 20).iter().all(|f| f.is_none()));
    }

    #[test]
    fn lights_not_seen_are_filled_in() {
        let f = fill(&[None, Some([0.2, 0.5]), None, None, Some([0.5, 0.2]), None]).unwrap();
        assert_eq!(f[1], [0.2, 0.5]);
        assert!((f[2][0] - 0.3).abs() < 1e-6 && (f[3][0] - 0.4).abs() < 1e-6, "evenly between: {f:?}");
        assert!((f[0][0] - 0.1).abs() < 1e-6 && (f[5][0] - 0.6).abs() < 1e-6, "ends carry on the line: {f:?}");
        assert_eq!(fill(&[None, Some([0.3, 0.3]), None]).unwrap(), vec![[0.3, 0.3]; 3]);
        assert!(fill(&[None, None]).is_none());
    }

    /// The scan drives the fixtures: two strips get one numbering, picture by picture; others go dark.
    #[test]
    fn the_lights_show_the_pictures() {
        let mut dmx = DmxSettings::default();
        dmx.fixtures.push(Fixture { id: 1, cols: 3, brightness: 0.2, gamma: 2.2, ..Default::default() });
        dmx.fixtures.push(Fixture { id: 2, cols: 2, channel: 10, ..Default::default() });
        dmx.fixtures.push(Fixture { id: 3, cols: 2, channel: 20, ..Default::default() });
        let t = targets(&dmx, None);
        assert_eq!(t.iter().map(|t| (t.first, t.count)).collect::<Vec<_>>(), vec![(0, 3), (3, 2), (5, 2)]);
        let only = targets(&dmx, Some(2));
        assert_eq!(only, vec![Target { fixture: 2, first: 0, count: 2 }]);
        // All on: full white even on the dimmed, gamma-curved fixture.
        let u = universes(&dmx, &only, 1);
        assert_eq!(&u[&0][9..15], &[255; 6], "fixture 2, both lights");
        assert_eq!(&u[&0][..9], &[0; 9], "fixture 1 isn't being scanned: off");
        // Bit 0 picture: light 1 of fixture 2 on, light 0 off.
        let u = universes(&dmx, &only, 2);
        assert_eq!((u[&0][9], u[&0][12]), (0, 255));
    }

    #[test]
    fn the_scanner_steps_through_its_pictures() {
        let mut dmx = DmxSettings::default();
        dmx.fixtures.push(Fixture { id: 1, cols: 5, ..Default::default() });
        let mut s = LedScanner { settle: 0.0, ..Default::default() };
        assert!(s.start(&DmxSettings::default(), None, 9, 16.0 / 9.0).is_err());
        s.start(&dmx, None, 9, 16.0 / 9.0).unwrap();
        let n = picture_count(5);
        for step in 0..n {
            assert!(s.universes(&dmx).is_some());
            assert!(s.due(step as f64));
            assert!(!s.due(step as f64), "one frame per picture");
            let (src, w, h) = s.picture().unwrap();
            assert_eq!(src, 9);
            s.give(vec![0; (w * h * 4) as usize], w, h);
        }
        assert!(s.universes(&dmx).is_none(), "the picture goes back to the lights");
        for _ in 0..200 {
            s.poll();
            if s.status != Status::Decoding {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(matches!(s.status, Status::Failed(_)), "a black camera finds nothing: {:?}", s.status);
    }
}
