//! A laser look → the points a laser draws. A laser's mirrors can't jump: they sweep from point to point, so the
//! path matters. `look_paths` makes each shape's lines; `trace` orders them, travels between them with the beam
//! off (and waits for the mirrors to arrive before lighting up), lingers at corners so they come out sharp, and
//! spaces the points so a whole frame fits the laser's points-a-second budget. `project` places the picture in a
//! projector's scan area, applies its colour balance and brightness, and blanks anything below its safety floor.

use std::f32::consts::TAU;

use crate::control::modulate;
use crate::model::{LaserLook, LaserProjector, LaserShape, LaserShapeKind};
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

/// Every enabled shape's lines at `t` seconds.
pub fn look_paths(look: &LaserLook, t: f64, mods: &ModInputs) -> Vec<Path> {
    look.shapes.iter().filter(|s| s.enabled).flat_map(|s| shape_paths(s, t, mods)).collect()
}

/// One shape's lines at `t` seconds (`mods`: its modulation).
pub fn shape_paths(s: &LaserShape, t: f64, mods: &ModInputs) -> Vec<Path> {
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
    let (unit, closed, dots) = unit_shape(s.kind, count, s.shape.clamp(0.0, 1.0), phase);
    let (sin, cos) = turn.sin_cos();
    let n = unit.len().max(1) as f32;
    let mut path = Path { closed, dots, points: Vec::with_capacity(unit.len()) };
    for (i, [ux, uy]) in unit.into_iter().enumerate() {
        let (px, py) = (ux * size, uy * size);
        let p = [x + px * cos - py * sin, y + px * sin + py * cos];
        // Rainbow: the hue runs along the shape (and round over time when the shape moves).
        let rgb = if s.rainbow > 0.0 { blend(s.color, hue(i as f32 / n + phase / TAU), s.rainbow) } else { s.color };
        path.points.push((p, rgb));
    }
    if draw < 1.0 {
        path = cut(&path, draw);
    }
    vec![path]
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
    }
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
    let paths: Vec<&Path> = paths.iter().filter(|p| !p.points.is_empty()).collect();
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

    // What the frame costs besides the lit lines: travel, waits, corners, beams. The lit points get the rest.
    let mut fixed = 0usize;
    let mut lit_length = 0.0f32;
    let mut from = [0.0f32, 0.0];
    for (pts, &dot) in lines.iter().zip(&dots) {
        fixed += travel_points(from, pts[0].0);
        if dot {
            fixed += pts.len() * DOT_POINTS + travel_between_dots(pts);
        } else {
            // (Each segment takes one point at least, however short.)
            fixed += 2 * CORNER_WAIT + corners(pts) * CORNER_WAIT + pts.len().saturating_sub(1);
            lit_length += pts.windows(2).map(|w| dist(w[0].0, w[1].0)).sum::<f32>();
        }
        from = pts[pts.len() - 1].0;
    }
    fixed += travel_points(from, [0.0, 0.0]);
    let room = budget.saturating_sub(fixed).max(budget / 4);
    let step = (lit_length / room as f32).max(LIT_STEP);
    // Too much for the budget even so: beams get fewer points each.
    let dot_points = if fixed > budget { (DOT_POINTS * budget / fixed.max(1)).max(4) } else { DOT_POINTS };

    let mut out = Vec::with_capacity(budget);
    let mut from = [0.0f32, 0.0];
    for (pts, &dot) in lines.iter().zip(&dots) {
        blank_to(&mut out, from, pts[0].0);
        if dot {
            for (k, (p, rgb)) in pts.iter().enumerate() {
                if k > 0 {
                    blank_to(&mut out, pts[k - 1].0, *p);
                }
                out.extend(std::iter::repeat_n(LaserPoint { x: p[0], y: p[1], rgb: *rgb }, dot_points));
            }
        } else {
            let (p0, c0) = pts[0];
            out.extend(std::iter::repeat_n(LaserPoint { x: p0[0], y: p0[1], rgb: c0 }, CORNER_WAIT));
            for (k, w) in pts.windows(2).enumerate() {
                let ((a, ca), (b, cb)) = (w[0], w[1]);
                let n = (dist(a, b) / step).floor() as usize + 1;
                for j in 1..=n {
                    let f = j as f32 / n as f32;
                    let p = lerp(a, b, f);
                    out.push(LaserPoint { x: p[0], y: p[1], rgb: lerp3(ca, cb, f) });
                }
                if k + 2 < pts.len() && sharp(a, b, pts[k + 2].0) {
                    out.extend(std::iter::repeat_n(LaserPoint { x: b[0], y: b[1], rgb: cb }, CORNER_WAIT));
                }
            }
            let (pn, cn) = pts[pts.len() - 1];
            out.extend(std::iter::repeat_n(LaserPoint { x: pn[0], y: pn[1], rgb: cn }, CORNER_WAIT));
        }
        from = pts[pts.len() - 1].0;
    }
    blank_to(&mut out, from, [0.0, 0.0]);
    out
}

fn travel_points(a: [f32; 2], b: [f32; 2]) -> usize {
    2 * BLANK_WAIT + (dist(a, b) / BLANK_STEP).ceil() as usize
}

fn travel_between_dots(pts: &[([f32; 2], [f32; 3])]) -> usize {
    pts.windows(2).map(|w| travel_points(w[0].0, w[1].0)).sum()
}

/// Beam off: wait where we are, move, wait for the mirrors to settle.
fn blank_to(out: &mut Vec<LaserPoint>, a: [f32; 2], b: [f32; 2]) {
    let dark = |p: [f32; 2]| LaserPoint { x: p[0], y: p[1], rgb: [0.0; 3] };
    out.extend(std::iter::repeat_n(dark(a), BLANK_WAIT));
    let n = (dist(a, b) / BLANK_STEP).ceil() as usize;
    for j in 1..=n {
        out.push(dark(lerp(a, b, j as f32 / n as f32)));
    }
    out.extend(std::iter::repeat_n(dark(b), BLANK_WAIT));
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
/// brightness × `master`, kept inside the scan area, and dark below its safety floor.
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
            let inside = (-1.0..=1.0).contains(&x) && (-1.0..=1.0).contains(&y) && y >= p.floor;
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
        let shapes = kinds.iter().enumerate().map(|(i, &kind)| LaserShape { id: i as u64 + 1, kind, x: i as f32 * 0.3 - 0.3, size: 0.3, ..Default::default() }).collect();
        LaserLook { id: 1, name: "Test".into(), shapes }
    }

    /// Every shape draws, a frame stays within its budget, starts and ends dark, and never jumps far with the beam on.
    #[test]
    fn frames_fit_the_budget_and_never_jump_lit() {
        let mods = ModInputs::default();
        for &kind in &LaserShapeKind::ALL {
            let paths = look_paths(&look(&[kind, LaserShapeKind::Circle]), 0.5, &mods);
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

    /// A small budget makes lines coarser instead of dropping them; a big one spaces points at the finest step.
    #[test]
    fn budget_sets_the_spacing() {
        let paths = look_paths(&look(&[LaserShapeKind::Spiral]), 0.0, &ModInputs::default());
        let small = trace(&paths, 300);
        let big = trace(&paths, 3000);
        assert!(small.len() < big.len());
        assert!(small.iter().filter(|p| p.lit()).count() > 60, "the spiral is still drawn");
    }

    /// Half drawn = about half the lit points; nothing drawn = dark.
    #[test]
    fn draw_amount_cuts_the_shape() {
        let mut s = LaserShape { kind: LaserShapeKind::Circle, size: 0.5, ..Default::default() };
        let full: usize = trace(&shape_paths(&s, 0.0, &ModInputs::default()), 4000).iter().filter(|p| p.lit()).count();
        s.draw = 0.5;
        let half: usize = trace(&shape_paths(&s, 0.0, &ModInputs::default()), 4000).iter().filter(|p| p.lit()).count();
        assert!((half as f32 / full as f32 - 0.5).abs() < 0.15, "{half} of {full}");
        s.draw = 0.0;
        assert!(shape_paths(&s, 0.0, &ModInputs::default()).is_empty());
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
