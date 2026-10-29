//! A laser look → the points a laser draws. A laser's mirrors can't jump: they sweep from point to point, so the
//! path matters. `look_paths` makes each shape's lines; `trace` orders them, travels between them with the beam
//! off (and waits for the mirrors to arrive before lighting up), lingers at corners so they come out sharp, and
//! spaces the points so a whole frame fits the laser's points-a-second budget. `project` places the picture in a
//! projector's scan area, applies its colour balance and brightness, and blanks anything below its safety floor.
//! `test_pattern` lines a laser up; `outline` traces pictures (Video trace shapes).

mod outline;

pub use outline::Picture;

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::Arc;

use crate::control::modulate;
use crate::ilda;
use crate::model::{LaserFont, LaserLook, LaserProjector, LaserShape, LaserShapeKind, LaserZone, Lasers};
use crate::ModInputs;

/// One point the laser draws: -1..1 across and up, colour 0..1 (all 0 = beam off).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LaserPoint {
    pub x: f32,
    pub y: f32,
    pub rgb: [f32; 3],
}

impl LaserPoint {
    pub fn lit(&self) -> bool {
        self.rgb.iter().any(|c| *c > 0.0)
    }
}

/// A line to draw: its corners and their colours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub points: Vec<([f32; 2], [f32; 3])>,
    /// Comes back to where it started.
    pub closed: bool,
    /// Beams: each point is a dot the laser stays on (bright in haze), not a line.
    pub dots: bool,
}

/// Frames a second sent to a laser (a look moves smoothly; each frame is drawn over and over until the next).
pub const FPS: u32 = 50;

/// How far apart lit points may be at most (in scan-area units, -1..1): smaller = brighter, smoother lines.
const LIT_STEP: f32 = 0.012;
/// Travel with the beam off: points per unit of distance, and points to wait at either end for the mirrors.
const BLANK_STEP: f32 = 0.08;
const BLANK_WAIT: usize = 6;
/// Extra points at a sharp corner (and where a line starts and ends) so the mirrors reach it before turning.
const CORNER_WAIT: usize = 3;
/// A beam (dot): points spent on it.
const DOT_POINTS: usize = 24;

/// What shapes draw from outside the look (the app fills these in).
#[derive(Clone, Debug, Default)]
pub struct LaserInputs {
    /// Laser files' frames, by the file's path (`LaserShape::file`).
    pub files: HashMap<String, Arc<Vec<Vec<ilda::Point>>>>,
    /// Small pictures for Video trace shapes, by source (`LaserShape::source`; `MASTER_ID` = the mix).
    pub pictures: HashMap<u64, Arc<Picture>>,
    /// Image files traced by Video trace shapes (`LaserShape::image`), small, by path.
    pub images: HashMap<String, Arc<Picture>>,
}

/// The source a Video trace shape reads (its layer, group or comp; the mix when 0).
pub fn trace_source(s: &LaserShape) -> u64 {
    if s.source == 0 { crate::MASTER_ID } else { s.source }
}

/// Every enabled shape's lines at `t` seconds (`inputs`: laser files and pictures so far).
pub fn look_paths(look: &LaserLook, t: f64, mods: &ModInputs, inputs: &LaserInputs) -> Vec<Path> {
    look.shapes.iter().filter(|s| s.enabled).flat_map(|s| shape_paths(s, t, mods, inputs)).collect()
}

/// Everything projector `p` draws at `t` seconds, before tracing: its look filling its area, or each enabled zone's
/// look placed in that zone (with the test pattern on: the pattern, in each zone when it has zones).
pub fn projector_paths(lasers: &Lasers, p: &LaserProjector, t: f64, mods: &ModInputs, inputs: &LaserInputs) -> Vec<Path> {
    let look = |id: u64| if p.test_pattern { test_pattern() } else { lasers.look(id).map(|l| look_paths(l, t, mods, inputs)).unwrap_or_default() };
    if p.zones.is_empty() {
        return look(p.look);
    }
    p.zones.iter().filter(|z| z.enabled && z.brightness > 0.0).flat_map(|z| in_zone(look(if z.look == 0 { p.look } else { z.look }), z)).collect()
}

/// Paths placed in a zone: moved, sized and turned inside it, then onto its corners (a homography: straight lines
/// stay straight, so a keystoned surface gets a true rectangle), dimmed by its brightness.
pub fn in_zone(paths: Vec<Path>, z: &LaserZone) -> Vec<Path> {
    let h = crate::mapping::square_to_quad(&z.corners);
    let (sin, cos) = z.rotate.to_radians().sin_cos();
    let gain = z.brightness.clamp(0.0, 1.0);
    let place = |[x, y]: [f32; 2]| {
        let (x, y) = (x * z.size, y * z.size);
        let (lx, ly) = (x * cos - y * sin + z.x, x * sin + y * cos + z.y);
        let (s, t) = ((lx + 1.0) * 0.5, (ly + 1.0) * 0.5);
        let w = h[6] * s + h[7] * t + h[8];
        let w = if w.abs() < 1e-6 { 1e-6 } else { w };
        [(h[0] * s + h[1] * t + h[2]) / w, (h[3] * s + h[4] * t + h[5]) / w]
    };
    paths
        .into_iter()
        .map(|mut p| {
            for (q, rgb) in &mut p.points {
                *q = place(*q);
                *rgb = rgb.map(|c| c * gain);
            }
            p
        })
        .collect()
}

/// Lining a laser up: the scan area's edge, a circle, a cross in the middle, an arrow pointing up, and a corner
/// in each colour (red top left, green top right, blue bottom right, white bottom left) to check flips and colours.
pub fn test_pattern() -> Vec<Path> {
    const WHITE: [f32; 3] = [1.0; 3];
    let line = |pts: &[[f32; 2]], rgb: [f32; 3], closed: bool| Path { points: pts.iter().map(|p| (*p, rgb)).collect(), closed, dots: false };
    let corner = |x: f32, y: f32, rgb| line(&[[x, y - 0.25 * y.signum()], [x, y], [x - 0.25 * x.signum(), y]], rgb, false);
    let circle: Vec<[f32; 2]> = (0..48).map(|i| (i as f32 / 48.0 * TAU).sin_cos()).map(|(s, c)| [c * 0.5, s * 0.5]).collect();
    vec![
        line(&[[-0.95, -0.95], [0.95, -0.95], [0.95, 0.95], [-0.95, 0.95]], WHITE, true),
        line(&circle, WHITE, true),
        line(&[[-0.15, 0.0], [0.15, 0.0]], WHITE, false),
        line(&[[0.0, -0.15], [0.0, 0.15]], WHITE, false),
        line(&[[-0.1, 0.65], [0.0, 0.8], [0.1, 0.65]], WHITE, false),
        corner(-1.0, 1.0, [1.0, 0.0, 0.0]),
        corner(1.0, 1.0, [0.0, 1.0, 0.0]),
        corner(1.0, -1.0, [0.0, 0.0, 1.0]),
        corner(-1.0, -1.0, WHITE),
    ]
}

/// A shape's lines before they're placed: around (0, 0), about -1..1 big, with their own colours when they have them.
struct Unit {
    points: Vec<[f32; 2]>,
    colors: Option<Vec<[f32; 3]>>,
    closed: bool,
    dots: bool,
}

/// One shape's lines at `t` seconds (`mods`: its modulation).
pub fn shape_paths(s: &LaserShape, t: f64, mods: &ModInputs, inputs: &LaserInputs) -> Vec<Path> {
    let x = modulate(s.x, -1.0, 1.0, s.mods.x.as_ref(), mods);
    let y = modulate(s.y, -1.0, 1.0, s.mods.y.as_ref(), mods);
    let size = modulate(s.size, 0.0, 1.0, s.mods.size.as_ref(), mods);
    let rotate = modulate(s.rotate, -180.0, 180.0, s.mods.rotate.as_ref(), mods);
    let draw = modulate(s.draw, 0.0, 1.0, s.mods.draw.as_ref(), mods).clamp(0.0, 1.0);
    if size <= 0.0 || draw <= 0.0 {
        return Vec::new();
    }
    let turn = (rotate.to_radians() as f64 + s.spin as f64 * t * std::f64::consts::TAU) as f32;
    let phase = ((s.motion as f64 * t).fract() as f32) * TAU;
    let count = s.count.max(0.5);
    let units = match s.kind {
        LaserShapeKind::Text => text_strokes(&s.text, s.font),
        LaserShapeKind::File => inputs.files.get(&s.file).map(|frames| file_frame(frames, t, count)).unwrap_or_default(),
        LaserShapeKind::Trace => (if s.image.is_empty() { inputs.pictures.get(&trace_source(s)) } else { inputs.images.get(&s.image) })
            .map(|pic| outline::outlines(pic, s.shape.clamp(0.02, 0.98), count.round().max(1.0) as usize, s.picture_colors))
            .unwrap_or_default(),
        // One point alone: a beam.
        LaserShapeKind::Drawing if s.points.is_empty() => Vec::new(),
        LaserShapeKind::Drawing => vec![Unit { points: s.points.clone(), colors: None, closed: s.closed && s.points.len() > 2, dots: s.points.len() == 1 }],
        kind => {
            let (points, closed, dots) = unit_shape(kind, count, s.shape.clamp(0.0, 1.0), phase);
            vec![Unit { points, colors: None, closed, dots }]
        }
    };
    let (sin, cos) = turn.sin_cos();
    // Rainbow: the hue runs along the whole shape (and round over time when the shape moves).
    let n = units.iter().map(|u| u.points.len()).sum::<usize>().max(1) as f32;
    let mut i = 0usize;
    let mut paths: Vec<Path> = units
        .into_iter()
        .map(|u| {
            let mut path = Path { closed: u.closed, dots: u.dots, points: Vec::with_capacity(u.points.len()) };
            for (k, [ux, uy]) in u.points.into_iter().enumerate() {
                let (px, py) = (ux * size, uy * size);
                let p = [x + px * cos - py * sin, y + px * sin + py * cos];
                let own = u.colors.as_ref().and_then(|c| c.get(k).copied()).unwrap_or(s.color);
                let rgb = if s.rainbow > 0.0 { blend(own, hue(i as f32 / n + phase / TAU), s.rainbow) } else { own };
                path.points.push((p, rgb));
                i += 1;
            }
            path
        })
        .collect();
    if draw < 1.0 {
        paths = cut_all(&paths, draw);
    }
    paths
}

/// Words as single strokes (Hershey's fonts: each stroke drawn once, no outlines), lines stacked, centred and
/// fitted to -1..1 across.
fn text_strokes(text: &str, font: LaserFont) -> Vec<Unit> {
    use vector_text_core::Renderer;
    use vector_text_hershey::{HersheyFont, HersheyRenderer};
    let hershey = |f: LaserFont| match f {
        LaserFont::Simple => HersheyFont::Romans,
        LaserFont::Bold => HersheyFont::Romand,
        LaserFont::Serif => HersheyFont::Romanc,
        LaserFont::Script => HersheyFont::Scripts,
        LaserFont::Italic => HersheyFont::Italicc,
        LaserFont::Gothic => HersheyFont::Gotheng,
    };
    /// Hershey units from one line to the next.
    const LINE: f32 = 32.0;
    let mut strokes: Vec<Vec<[f32; 2]>> = Vec::new();
    for (row, line) in text.lines().enumerate() {
        let mut stroke: Vec<[f32; 2]> = Vec::new();
        for p in HersheyRenderer::render_text(line, hershey(font)) {
            // (Hershey's y runs down the page.)
            let q = [p.x as f32, -(p.y as f32) - row as f32 * LINE];
            if !p.pen && !stroke.is_empty() {
                strokes.push(std::mem::take(&mut stroke));
            }
            stroke.push(q);
        }
        if !stroke.is_empty() {
            strokes.push(stroke);
        }
    }
    strokes.retain(|s| s.len() > 1);
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in strokes.iter().flatten() {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    if span <= 0.0 {
        return Vec::new();
    }
    let (mid, scale) = ([(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5], 2.0 / span);
    strokes
        .into_iter()
        .map(|s| Unit { points: s.into_iter().map(|p| [(p[0] - mid[0]) * scale, (p[1] - mid[1]) * scale]).collect(), colors: None, closed: false, dots: false })
        .collect()
}

/// The frame of a laser file showing at `t` seconds (`fps` frames a second, looping), as lit lines with the file's
/// colours; a point lit on its own is a beam.
fn file_frame(frames: &[Vec<ilda::Point>], t: f64, fps: f32) -> Vec<Unit> {
    if frames.is_empty() {
        return Vec::new();
    }
    let frame = &frames[((t.max(0.0) * fps as f64) as usize) % frames.len()];
    let mut out = Vec::new();
    let mut run: (Vec<[f32; 2]>, Vec<[f32; 3]>) = (Vec::new(), Vec::new());
    let finish = |run: &mut (Vec<[f32; 2]>, Vec<[f32; 3]>), out: &mut Vec<Unit>| {
        let (mut points, mut colors) = std::mem::take(run);
        // Every point on one spot: a beam (the laser stays there).
        if points.windows(2).all(|w| dist(w[0], w[1]) < 1e-4) {
            points.truncate(1);
            colors = vec![colors.iter().copied().find(|c| *c != [0.0; 3]).unwrap_or([0.0; 3])];
        }
        if points.len() >= 2 || colors.first().is_some_and(|c| *c != [0.0; 3]) {
            let dots = points.len() == 1;
            out.push(Unit { points, colors: Some(colors), closed: false, dots });
        }
    };
    for p in frame {
        let rgb = p.rgb.map(|c| c as f32 / 255.0);
        if p.blank || rgb == [0.0; 3] {
            // The beam travels dark to here; the next lit point draws from here.
            finish(&mut run, &mut out);
            run.0.push([p.x, p.y]);
            run.1.push([0.0; 3]);
            continue;
        }
        if let Some(c) = run.1.last_mut().filter(|c| **c == [0.0; 3]) {
            *c = rgb;
        }
        run.0.push([p.x, p.y]);
        run.1.push(rgb);
    }
    finish(&mut run, &mut out);
    // A lone dark start point (the beam only travelled there) isn't drawn.
    out.retain(|u| u.points.len() >= 2 || u.colors.as_ref().is_some_and(|c| c[0] != [0.0; 3]));
    out
}

/// A shape's corners around (0, 0), about -1..1 big: (points, closed, dots).
fn unit_shape(kind: LaserShapeKind, count: f32, shape: f32, phase: f32) -> (Vec<[f32; 2]>, bool, bool) {
    let ring = |n: usize, r: f32, start: f32| (0..n).map(move |i| {
        let a = start + i as f32 / n as f32 * TAU;
        [a.cos() * r, a.sin() * r]
    });
    match kind {
        LaserShapeKind::Line => (vec![[-1.0, 0.0], [1.0, 0.0]], false, false),
        LaserShapeKind::Circle => (ring(48, 1.0, 0.0).collect(), true, false),
        LaserShapeKind::Polygon => (ring(count.round().clamp(3.0, 32.0) as usize, 1.0, TAU / 4.0).collect(), true, false),
        LaserShapeKind::Star => {
            let n = count.round().clamp(3.0, 32.0) as usize;
            let inner = 0.15 + shape * 0.8;
            let pts = (0..n * 2)
                .map(|i| {
                    let a = TAU / 4.0 + i as f32 / (n * 2) as f32 * TAU;
                    let r = if i % 2 == 0 { 1.0 } else { inner };
                    [a.cos() * r, a.sin() * r]
                })
                .collect();
            (pts, true, false)
        }
        LaserShapeKind::Wave => {
            let n = 64;
            let pts = (0..=n)
                .map(|i| {
                    let u = i as f32 / n as f32;
                    [u * 2.0 - 1.0, (u * count * TAU + phase).sin() * shape.max(0.02)]
                })
                .collect();
            (pts, false, false)
        }
        LaserShapeKind::Lissajous => {
            let n = 160;
            let b = 1.0 + (shape * 6.0).round();
            let pts = (0..n).map(|i| {
                let u = i as f32 / n as f32 * TAU;
                [(count.round() * u + phase).sin(), (b * u).sin()]
            });
            (pts.collect(), true, false)
        }
        LaserShapeKind::Spiral => {
            let n = (count * 40.0) as usize;
            let pts = (0..=n)
                .map(|i| {
                    let u = i as f32 / n as f32;
                    let a = u * count * TAU + phase;
                    [a.cos() * u, a.sin() * u]
                })
                .collect();
            (pts, false, false)
        }
        LaserShapeKind::Beams => {
            let n = count.round().clamp(1.0, 64.0) as usize;
            let r = if n == 1 { 0.0 } else { shape.max(0.02) };
            (ring(n, r, TAU / 4.0 + phase).collect(), false, true)
        }
        // (Made from their words, file or picture: `text_strokes`, `file_frame`, `outline::outlines`.)
        LaserShapeKind::Text | LaserShapeKind::File | LaserShapeKind::Trace | LaserShapeKind::Drawing => (Vec::new(), false, false),
    }
}

/// The first `part` (0..1) of a shape's paths, by length, drawn in order.
fn cut_all(paths: &[Path], part: f32) -> Vec<Path> {
    if paths.len() == 1 {
        return vec![cut(&paths[0], part)];
    }
    let len = |p: &Path| if p.dots { p.points.len() as f32 * 0.05 } else { p.points.windows(2).map(|w| dist(w[0].0, w[1].0)).sum::<f32>() };
    let mut left = paths.iter().map(len).sum::<f32>() * part;
    let mut out = Vec::new();
    for p in paths {
        let l = len(p);
        if l <= left {
            out.push(p.clone());
            left -= l;
        } else {
            if left > 0.0 && l > 0.0 {
                out.push(cut(p, left / l));
            }
            break;
        }
    }
    out
}

/// The first `part` (0..1) of a path, by length.
fn cut(path: &Path, part: f32) -> Path {
    let mut pts = path.points.clone();
    if path.closed && !path.dots {
        if let Some(first) = pts.first().copied() {
            pts.push(first);
        }
    }
    if path.dots {
        let keep = ((pts.len() as f32 * part).ceil() as usize).min(pts.len());
        pts.truncate(keep);
        return Path { points: pts, closed: false, dots: true };
    }
    let total: f32 = pts.windows(2).map(|w| dist(w[0].0, w[1].0)).sum();
    let mut left = total * part;
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let d = dist(w[0].0, w[1].0);
        if d >= left {
            let f = if d > 0.0 { left / d } else { 0.0 };
            out.push((lerp(w[0].0, w[1].0, f), w[1].1));
            break;
        }
        left -= d;
        out.push(w[1]);
    }
    Path { points: out, closed: false, dots: false }
}

/// Paths → the points of one frame: ordered nearest first, joined by blanked travel, at most `budget` points
/// (points a second ÷ frames a second). Starts and ends dark at (0, 0) so frames join cleanly.
pub fn trace(paths: &[Path], budget: usize) -> Vec<LaserPoint> {
    let paths = fitting(paths, budget);
    if paths.is_empty() || budget < 8 {
        return vec![LaserPoint::default()];
    }
    // Nearest next, each path drawn from whichever end is closer (a closed one from its first corner).
    let mut order: Vec<(usize, bool)> = Vec::with_capacity(paths.len());
    let mut used = vec![false; paths.len()];
    let mut at = [0.0f32, 0.0];
    for _ in 0..paths.len() {
        let mut best: Option<(f32, usize, bool)> = None;
        for (i, p) in paths.iter().enumerate().filter(|(i, _)| !used[*i]) {
            let first = p.points[0].0;
            let last = p.points[p.points.len() - 1].0;
            let (d, rev) = if p.closed || dist(at, first) <= dist(at, last) { (dist(at, first), false) } else { (dist(at, last), true) };
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, i, rev));
            }
        }
        let (_, i, rev) = best.expect("a path is left");
        used[i] = true;
        let p = paths[i];
        at = if p.closed { p.points[0].0 } else if rev { p.points[0].0 } else { p.points[p.points.len() - 1].0 };
        order.push((i, rev));
    }
    let lines: Vec<Vec<([f32; 2], [f32; 3])>> = order
        .iter()
        .map(|&(i, rev)| {
            let p = paths[i];
            let mut pts = p.points.clone();
            if rev {
                pts.reverse();
            }
            if p.closed && !p.dots {
                pts.push(pts[0]);
            }
            pts
        })
        .collect();
    let dots: Vec<bool> = order.iter().map(|&(i, _)| paths[i].dots).collect();

    // What the frame costs besides the lit lines: travel, waits, corners, beams. The lit points get the rest. A
    // busy frame (words, many strokes) waits less at jumps and corners before its lines get coarser.
    let cost = |blank_wait: usize, corner_wait: usize| {
        let mut fixed = 0usize;
        let mut from = [0.0f32, 0.0];
        for (pts, &dot) in lines.iter().zip(&dots) {
            fixed += travel_points(from, pts[0].0, blank_wait);
            if dot {
                fixed += pts.len() * DOT_POINTS + travel_between_dots(pts, blank_wait);
            } else {
                // (Each segment takes one point at least, however short.)
                fixed += 2 * corner_wait + corners(pts) * corner_wait + pts.len().saturating_sub(1);
            }
            from = pts[pts.len() - 1].0;
        }
        fixed + travel_points(from, [0.0, 0.0], blank_wait)
    };
    let waits = [(BLANK_WAIT, CORNER_WAIT), (4, 2), (3, 1), (2, 1)];
    let (blank_wait, corner_wait) = waits.iter().copied().find(|&(b, c)| cost(b, c) <= budget * 3 / 4).unwrap_or(waits[waits.len() - 1]);
    let fixed = cost(blank_wait, corner_wait);
    let lit_length: f32 = lines.iter().zip(&dots).filter(|(_, d)| !**d).map(|(pts, _)| pts.windows(2).map(|w| dist(w[0].0, w[1].0)).sum::<f32>()).sum();
    let room = budget.saturating_sub(fixed).max(1);
    let step = (lit_length / room as f32).max(LIT_STEP);
    // Too much for the budget even so: beams get fewer points each.
    let dot_points = if fixed > budget { (DOT_POINTS * budget / fixed.max(1)).max(4) } else { DOT_POINTS };

    let mut out = Vec::with_capacity(budget);
    let mut from = [0.0f32, 0.0];
    for (pts, &dot) in lines.iter().zip(&dots) {
        blank_to(&mut out, from, pts[0].0, blank_wait);
        if dot {
            for (k, (p, rgb)) in pts.iter().enumerate() {
                if k > 0 {
                    blank_to(&mut out, pts[k - 1].0, *p, blank_wait);
                }
                out.extend(std::iter::repeat_n(LaserPoint { x: p[0], y: p[1], rgb: *rgb }, dot_points));
            }
        } else {
            let (p0, c0) = pts[0];
            out.extend(std::iter::repeat_n(LaserPoint { x: p0[0], y: p0[1], rgb: c0 }, corner_wait));
            for (k, w) in pts.windows(2).enumerate() {
                let ((a, ca), (b, cb)) = (w[0], w[1]);
                let n = (dist(a, b) / step).floor() as usize + 1;
                for j in 1..=n {
                    let f = j as f32 / n as f32;
                    let p = lerp(a, b, f);
                    out.push(LaserPoint { x: p[0], y: p[1], rgb: lerp3(ca, cb, f) });
                }
                if k + 2 < pts.len() && sharp(a, b, pts[k + 2].0) {
                    out.extend(std::iter::repeat_n(LaserPoint { x: b[0], y: b[1], rgb: cb }, corner_wait));
                }
            }
            let (pn, cn) = pts[pts.len() - 1];
            out.extend(std::iter::repeat_n(LaserPoint { x: pn[0], y: pn[1], rgb: cn }, corner_wait));
        }
        from = pts[pts.len() - 1].0;
    }
    blank_to(&mut out, from, [0.0, 0.0], blank_wait);
    out
}

/// The paths a frame of `budget` points can hold: with too many lines even at the shortest waits (many outlines,
/// long words), the shortest go.
fn fitting(paths: &[Path], budget: usize) -> Vec<&Path> {
    let mut paths: Vec<&Path> = paths.iter().filter(|p| !p.points.is_empty()).collect();
    while paths.len() > 1 && least_cost(&paths) > budget * 9 / 10 {
        let shortest = (0..paths.len()).min_by(|&a, &b| path_length(paths[a]).total_cmp(&path_length(paths[b]))).expect("paths are left");
        paths.remove(shortest);
    }
    paths
}

/// How many of `paths` a frame of `budget` points leaves out (`trace` drops the shortest when crowded).
pub fn left_out(paths: &[Path], budget: usize) -> usize {
    paths.iter().filter(|p| !p.points.is_empty()).count() - fitting(paths, budget).len()
}

/// About the fewest points `paths` take outside their lit lines (shortest waits, drawn in the given order).
fn least_cost(paths: &[&Path]) -> usize {
    let mut fixed = 0;
    let mut from = [0.0f32, 0.0];
    for p in paths {
        fixed += travel_points(from, p.points[0].0, 2);
        fixed += if p.dots { p.points.len() * 4 + travel_between_dots(&p.points, 2) } else { 2 + corners(&p.points) + p.points.len() };
        from = p.points[p.points.len() - 1].0;
    }
    fixed
}

fn path_length(p: &Path) -> f32 {
    if p.dots { p.points.len() as f32 * 0.05 } else { p.points.windows(2).map(|w| dist(w[0].0, w[1].0)).sum() }
}

fn travel_points(a: [f32; 2], b: [f32; 2], wait: usize) -> usize {
    2 * wait + (dist(a, b) / BLANK_STEP).ceil() as usize
}

fn travel_between_dots(pts: &[([f32; 2], [f32; 3])], wait: usize) -> usize {
    pts.windows(2).map(|w| travel_points(w[0].0, w[1].0, wait)).sum()
}

/// Beam off: wait where we are, move, wait for the mirrors to settle.
fn blank_to(out: &mut Vec<LaserPoint>, a: [f32; 2], b: [f32; 2], wait: usize) {
    let dark = |p: [f32; 2]| LaserPoint { x: p[0], y: p[1], rgb: [0.0; 3] };
    out.extend(std::iter::repeat_n(dark(a), wait));
    let n = (dist(a, b) / BLANK_STEP).ceil() as usize;
    for j in 1..=n {
        out.push(dark(lerp(a, b, j as f32 / n as f32)));
    }
    out.extend(std::iter::repeat_n(dark(b), wait));
}

/// Corners sharper than about 45°.
fn corners(pts: &[([f32; 2], [f32; 3])]) -> usize {
    pts.windows(3).filter(|w| sharp(w[0].0, w[1].0, w[2].0)).count()
}

fn sharp(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
    let (lu, lv) = ((u[0] * u[0] + u[1] * u[1]).sqrt(), (v[0] * v[0] + v[1] * v[1]).sqrt());
    lu > 1e-6 && lv > 1e-6 && (u[0] * v[0] + u[1] * v[1]) / (lu * lv) < 0.7
}

/// The frame as projector `p` draws it: placed (size, position, turn, flips), colour-balanced and dimmed by its
/// brightness × `master`, kept inside the scan area, and dark below its safety floor and in its dark areas.
pub fn project(points: &[LaserPoint], p: &LaserProjector, master: f32) -> Vec<LaserPoint> {
    let (sin, cos) = p.rotate.to_radians().sin_cos();
    let gain = (p.brightness * master).clamp(0.0, 1.0);
    points
        .iter()
        .map(|q| {
            let (mut x, mut y) = (q.x * p.size, q.y * p.size);
            if p.flip_x {
                x = -x;
            }
            if p.flip_y {
                y = -y;
            }
            let (x, y) = (x * cos - y * sin + p.x, x * sin + y * cos + p.y);
            let inside = (-1.0..=1.0).contains(&x) && (-1.0..=1.0).contains(&y) && y >= p.floor && !p.dark.iter().any(|a| a.contains(x, y));
            let rgb = if inside { [0, 1, 2].map(|i| (q.rgb[i] * p.color[i] * gain).clamp(0.0, 1.0)) } else { [0.0; 3] };
            LaserPoint { x: x.clamp(-1.0, 1.0), y: y.clamp(-1.0, 1.0), rgb }
        })
        .collect()
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

fn lerp(a: [f32; 2], b: [f32; 2], f: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]
}

fn lerp3(a: [f32; 3], b: [f32; 3], f: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * f)
}

fn blend(a: [f32; 3], b: [f32; 3], f: f32) -> [f32; 3] {
    lerp3(a, b, f.clamp(0.0, 1.0))
}

/// A fully saturated colour at hue `h` (0..1 round the wheel).
pub fn hue(h: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let f = |n: f32| (((n + h) % 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0);
    [f(0.0), f(4.0), f(2.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(kinds: &[LaserShapeKind]) -> LaserLook {
        let shapes = kinds.iter().enumerate().map(|(i, &kind)| LaserShape { id: i as u64 + 1, kind, x: i as f32 * 0.3 - 0.3, size: 0.3, file: "test.ild".into(), points: vec![[-1.0, 0.0], [0.0, 1.0], [1.0, 0.0]], ..Default::default() }).collect();
        LaserLook { id: 1, name: "Test".into(), shapes, ..Default::default() }
    }

    fn pt(x: f32, y: f32, blank: bool) -> ilda::Point {
        ilda::Point { x, y, rgb: [255, 0, 0], blank }
    }

    /// A two-frame laser file: a square (one dark move, then lit), then a line and a lone dot.
    /// ... and a picture of the mix: a bright disc on black.
    fn files() -> LaserInputs {
        let square = vec![pt(-0.5, -0.5, true), pt(0.5, -0.5, false), pt(0.5, 0.5, false), pt(-0.5, 0.5, false), pt(-0.5, -0.5, false)];
        let second = vec![pt(-0.8, 0.0, true), pt(0.8, 0.0, false), pt(0.0, 0.6, true), pt(0.0, 0.6, false)];
        let files = HashMap::from([("test.ild".to_string(), Arc::new(vec![square, second]))]);
        let pictures = HashMap::from([(crate::MASTER_ID, Arc::new(outline::tests::disc(64, 36, [255, 120, 0])))]);
        LaserInputs { files, pictures, ..Default::default() }
    }

    /// The test pattern fits a frame, keeps its corners' colours and reaches the scan area's corners.
    #[test]
    fn test_pattern_fits_and_shows_colours() {
        let frame = trace(&test_pattern(), 30_000 / FPS as usize);
        assert!(frame.len() <= 600, "{}", frame.len());
        for c in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
            assert!(frame.iter().any(|p| p.rgb == c), "{c:?} corner");
        }
        assert!(frame.iter().any(|p| p.lit() && p.x == 1.0 && p.y == 1.0), "reaches the scan area's corner");
    }

    /// An image traced instead of a layer; a drawing goes where its corners are (placed, closed), one corner = a beam.
    #[test]
    fn images_trace_and_drawings_draw() {
        let mut inputs = files();
        inputs.images.insert("logo.png".into(), Arc::new(outline::tests::disc(40, 40, [0, 0, 255])));
        let s = LaserShape { kind: LaserShapeKind::Trace, image: "logo.png".into(), size: 1.0, count: 5.0, shape: 0.5, ..Default::default() };
        let paths = shape_paths(&s, 0.0, &ModInputs::default(), &inputs);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].points[0].1, [0.0, 0.0, 1.0], "the image's colour, not the mix's");
        assert!(shape_paths(&LaserShape { image: "not read.png".into(), ..s }, 0.0, &ModInputs::default(), &inputs).is_empty());
        let d = LaserShape { kind: LaserShapeKind::Drawing, points: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], closed: true, x: 0.5, size: 0.5, ..Default::default() };
        let paths = shape_paths(&d, 0.0, &ModInputs::default(), &inputs);
        assert!(paths[0].closed);
        assert_eq!(paths[0].points.iter().map(|p| p.0).collect::<Vec<_>>(), vec![[0.5, 0.0], [1.0, 0.0], [1.0, 0.5]]);
        let one = LaserShape { points: vec![[0.2, 0.2]], ..d.clone() };
        assert!(shape_paths(&one, 0.0, &ModInputs::default(), &inputs)[0].dots);
        assert!(shape_paths(&LaserShape { points: Vec::new(), ..d }, 0.0, &ModInputs::default(), &inputs).is_empty());
    }

    /// Text: one stroke a path, fitted to the shape's size and centred; lines stack; nothing for empty words.
    #[test]
    fn text_draws_single_strokes() {
        let s = LaserShape { kind: LaserShapeKind::Text, text: "HI".into(), size: 0.5, ..Default::default() };
        let paths = shape_paths(&s, 0.0, &ModInputs::default(), &files());
        assert!(paths.len() >= 4, "H is 3 strokes, I one: {}", paths.len());
        let xs: Vec<f32> = paths.iter().flat_map(|p| p.points.iter().map(|q| q.0[0])).collect();
        let (lo, hi) = (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max));
        assert!((hi - lo - 1.0).abs() < 0.05 && (hi + lo).abs() < 0.05, "1 wide (size 0.5) and centred: {lo}..{hi}");
        let two = LaserShape { text: "HI\nHI".into(), ..s.clone() };
        let ys: Vec<f32> = shape_paths(&two, 0.0, &ModInputs::default(), &files()).iter().flat_map(|p| p.points.iter().map(|q| q.0[1])).collect();
        assert!(ys.iter().copied().fold(f32::MIN, f32::max) > 0.1 && ys.iter().copied().fold(f32::MAX, f32::min) < -0.1, "two lines, one above the other");
        let busy = LaserShape { text: "PHOSPHOR".into(), ..s.clone() };
        let frame = trace(&shape_paths(&busy, 0.0, &ModInputs::default(), &files()), 600);
        assert!(frame.len() <= 600, "a word fits a 600-point frame: {}", frame.len());
        let blank = LaserShape { text: "  ".into(), ..s };
        assert!(shape_paths(&blank, 0.0, &ModInputs::default(), &files()).is_empty());
        for font in LaserFont::ALL {
            let f = LaserShape { kind: LaserShapeKind::Text, font, ..Default::default() };
            assert!(!shape_paths(&f, 0.0, &ModInputs::default(), &files()).is_empty(), "{font:?} draws");
        }
    }

    /// A laser file plays its frames in time (looping), lit runs as lines in the file's colours, a lone lit point
    /// as a beam, dark moves left dark; a file not read yet draws nothing.
    #[test]
    fn laser_files_play_their_frames() {
        let s = LaserShape { kind: LaserShapeKind::File, file: "test.ild".into(), size: 1.0, count: 10.0, ..Default::default() };
        let first = shape_paths(&s, 0.0, &ModInputs::default(), &files());
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].points.len(), 5, "the square, from where the dark move ended");
        assert_eq!(first[0].points[0].1, [1.0, 0.0, 0.0], "the file's colour");
        let second = shape_paths(&s, 0.15, &ModInputs::default(), &files());
        assert_eq!(second.len(), 2, "a line and a beam");
        assert!(second.iter().any(|p| p.dots && p.points.len() == 1));
        assert_eq!(shape_paths(&s, 0.2, &ModInputs::default(), &files()), first, "loops");
        let missing = LaserShape { file: "not read.ild".into(), ..s };
        assert!(shape_paths(&missing, 0.0, &ModInputs::default(), &files()).is_empty());
    }

    /// Every shape draws, a frame stays within its budget, starts and ends dark, and never jumps far with the beam on.
    #[test]
    fn frames_fit_the_budget_and_never_jump_lit() {
        let mods = ModInputs::default();
        for &kind in &LaserShapeKind::ALL {
            let paths = look_paths(&look(&[kind, LaserShapeKind::Circle]), 0.5, &mods, &files());
            assert!(!paths.is_empty(), "{kind:?} draws");
            let budget = 30_000 / 30;
            let frame = trace(&paths, budget);
            assert!(frame.len() <= budget, "{kind:?}: {} points for a budget of {budget}", frame.len());
            assert!(frame.iter().any(LaserPoint::lit), "{kind:?} lights up");
            assert!(!frame[0].lit() && !frame[frame.len() - 1].lit(), "{kind:?}: dark at both ends");
            for w in frame.windows(2) {
                if w[0].lit() && w[1].lit() {
                    assert!(dist([w[0].x, w[0].y], [w[1].x, w[1].y]) <= 0.2, "{kind:?}: a lit jump");
                }
            }
        }
    }

    /// More lines than a frame can hold: the shortest go, the frame keeps its budget and the longest line.
    #[test]
    fn crowded_frames_drop_the_shortest_lines() {
        let mut paths: Vec<Path> = (0..120).map(|i| {
            let (x, y) = ((i % 12) as f32 * 0.16 - 0.9, (i / 12) as f32 * 0.18 - 0.9);
            Path { points: vec![([x, y], [1.0; 3]), ([x + 0.05, y], [1.0; 3]), ([x + 0.05, y + 0.05], [1.0; 3])], closed: false, dots: false }
        }).collect();
        paths.push(Path { points: vec![([-1.0, 0.95], [1.0, 0.0, 0.0]), ([1.0, 0.95], [1.0, 0.0, 0.0])], closed: false, dots: false });
        let frame = trace(&paths, 600);
        assert!(frame.len() <= 600, "{}", frame.len());
        assert!(frame.iter().any(|p| p.rgb == [1.0, 0.0, 0.0]), "the longest line stays");
        assert!(left_out(&paths, 600) > 0 && left_out(&paths, 100_000) == 0);
    }

    /// A small budget makes lines coarser instead of dropping them; a big one spaces points at the finest step.
    #[test]
    fn budget_sets_the_spacing() {
        let paths = look_paths(&look(&[LaserShapeKind::Spiral]), 0.0, &ModInputs::default(), &files());
        let small = trace(&paths, 300);
        let big = trace(&paths, 3000);
        assert!(small.len() < big.len());
        assert!(small.iter().filter(|p| p.lit()).count() > 60, "the spiral is still drawn");
    }

    /// Half drawn = about half the lit points; nothing drawn = dark.
    #[test]
    fn draw_amount_cuts_the_shape() {
        let mut s = LaserShape { kind: LaserShapeKind::Circle, size: 0.5, ..Default::default() };
        let full: usize = trace(&shape_paths(&s, 0.0, &ModInputs::default(), &files()), 4000).iter().filter(|p| p.lit()).count();
        s.draw = 0.5;
        let half: usize = trace(&shape_paths(&s, 0.0, &ModInputs::default(), &files()), 4000).iter().filter(|p| p.lit()).count();
        assert!((half as f32 / full as f32 - 0.5).abs() < 0.15, "{half} of {full}");
        s.draw = 0.0;
        assert!(shape_paths(&s, 0.0, &ModInputs::default(), &files()).is_empty());
    }

    /// Zones: each shows its look placed on its corners (a keystone keeps lines straight), dimmed by its brightness;
    /// switched-off zones draw nothing; the test pattern shows in every zone; dark areas blank the beam.
    #[test]
    fn zones_place_their_looks() {
        let mut lasers = Lasers::default();
        let line = |id, y| LaserLook { id, name: "L".into(), shapes: vec![LaserShape { id: id + 100, kind: LaserShapeKind::Line, y, size: 1.0, ..Default::default() }], ..Default::default() };
        lasers.looks = vec![line(1, 0.0), line(2, 0.5)];
        let left = LaserZone { id: 10, look: 0, corners: [[-1.0, -1.0], [0.0, -1.0], [0.0, 1.0], [-1.0, 1.0]], ..Default::default() };
        // A keystoned zone: narrower at the top.
        let right = LaserZone { id: 11, look: 2, brightness: 0.5, corners: [[0.0, -1.0], [1.0, -1.0], [0.8, 1.0], [0.2, 1.0]], ..Default::default() };
        let mut p = LaserProjector { id: 5, look: 1, zones: vec![left, right], ..Default::default() };
        let paths = projector_paths(&lasers, &p, 0.0, &ModInputs::default(), &LaserInputs::default());
        assert_eq!(paths.len(), 2);
        let a = &paths[0].points;
        assert!((a[0].0[0] + 1.0).abs() < 1e-5 && a[1].0[0].abs() < 1e-5 && a[0].0[1].abs() < 1e-5, "the projector's look across the left half: {a:?}");
        let b = &paths[1].points;
        // The line's ends sit on the keystone's slanted sides (x = 0.1 (y + 1) on the left), level with each other.
        let (l, r) = (b[0].0, b[1].0);
        assert!((l[0] - 0.1 * (l[1] + 1.0)).abs() < 1e-4 && (r[0] - (1.0 - 0.1 * (r[1] + 1.0))).abs() < 1e-4 && (l[1] - r[1]).abs() < 1e-5 && l[1] > 0.3, "the zone's look on its keystone: {b:?}");
        assert_eq!(b[0].1, [0.0, 0.5, 0.15], "half as bright");
        p.zones[1].enabled = false;
        assert_eq!(projector_paths(&lasers, &p, 0.0, &ModInputs::default(), &LaserInputs::default()).len(), 1);
        p.test_pattern = true;
        assert_eq!(projector_paths(&lasers, &p, 0.0, &ModInputs::default(), &LaserInputs::default()).len(), test_pattern().len(), "the pattern in the one zone on");
        let dark = LaserProjector { dark: vec![crate::model::LaserArea { left: -0.1, bottom: -0.1, right: 0.1, top: 0.1 }], ..Default::default() };
        let out = project(&[LaserPoint { x: 0.0, y: 0.0, rgb: [1.0; 3] }, LaserPoint { x: 0.5, y: 0.0, rgb: [1.0; 3] }], &dark, 1.0);
        assert!(!out[0].lit() && out[1].lit(), "dark inside the dark area only");
    }

    /// Projection: placed, flipped, dimmed, colour-balanced; the floor and the scan area's edge blank the beam.
    #[test]
    fn projector_places_dims_and_keeps_the_floor() {
        let pts = [LaserPoint { x: 0.5, y: 0.5, rgb: [1.0; 3] }, LaserPoint { x: 0.5, y: -0.5, rgb: [1.0; 3] }, LaserPoint { x: 0.9, y: 0.0, rgb: [1.0; 3] }];
        let p = LaserProjector { size: 0.5, x: 0.1, flip_x: true, brightness: 0.5, color: [1.0, 0.5, 0.0], floor: -0.1, ..Default::default() };
        let out = project(&pts, &p, 0.8);
        assert!((out[0].x - (-0.25 + 0.1)).abs() < 1e-5 && (out[0].y - 0.25).abs() < 1e-5);
        assert_eq!(out[0].rgb, [0.4, 0.2, 0.0]);
        assert!(!out[1].lit(), "below the floor: dark");
        let off = project(&pts, &LaserProjector { x: 0.5, ..Default::default() }, 1.0);
        assert!(!off[2].lit() && off[2].x == 1.0, "past the edge: dark and held at the edge");
        assert!(project(&pts, &p, 0.0).iter().all(|q| !q.lit()), "master at 0: all dark");
    }
}
