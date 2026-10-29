//! Video trace: a small picture → outlines a laser can draw. The picture is blurred a little (video noise makes
//! outlines crawl), then marching squares finds where its brightness crosses a level: the edges of bright shapes.
//! The outlines are joined into lines, simplified (Douglas–Peucker) and the longest kept.

use super::Unit;

/// A picture to trace (read back small from the GPU): `width` × `height` RGBA, top row first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// How far (in pixels) a simplified outline may stray from the traced one.
const TOLERANCE: f32 = 0.7;
/// Outlines shorter than this (pixels) are noise.
const SHORTEST: f32 = 6.0;

/// The `most` longest outlines where `pic`'s brightness crosses `level` (0..1), fitted to -1..1 across (the
/// picture's longer side); `own_colors`: each point takes the picture's colour there (made bright), else none.
pub(super) fn outlines(pic: &Picture, level: f32, most: usize, own_colors: bool) -> Vec<Unit> {
    let (w, h) = (pic.width, pic.height);
    if w < 2 || h < 2 || pic.rgba.len() < w * h * 4 {
        return Vec::new();
    }
    let luma: Vec<f32> = pic.rgba.chunks_exact(4).take(w * h).map(|c| (0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32) / 255.0).collect();
    let lum = blur(&luma, w, h);
    let at = |x: usize, y: usize| lum[y * w + x];
    // Edge ids: across (x, y)→(x+1, y) = 2·(y·w + x); down (x, y)→(x, y+1) = 2·(y·w + x) + 1. Each edge joins at
    // most two others (one through each cell beside it).
    let across = |x: usize, y: usize| 2 * (y * w + x);
    let down = |x: usize, y: usize| 2 * (y * w + x) + 1;
    const NONE: usize = usize::MAX;
    let mut links = vec![[NONE; 2]; 2 * w * h];
    let mut link = |a: usize, b: usize| {
        for (e, o) in [(a, b), (b, a)] {
            let l = &mut links[e];
            if l[0] == NONE { l[0] = o } else { l[1] = o }
        }
    };
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            let (a, b, c, d) = (at(x, y), at(x + 1, y), at(x + 1, y + 1), at(x, y + 1));
            let case = (a > level) as u8 * 8 + (b > level) as u8 * 4 + (c > level) as u8 * 2 + (d > level) as u8;
            let (top, right, bottom, left) = (across(x, y), down(x + 1, y), across(x, y + 1), down(x, y));
            let middle = (a + b + c + d) * 0.25 > level;
            match case {
                1 | 14 => link(left, bottom),
                2 | 13 => link(bottom, right),
                3 | 12 => link(left, right),
                4 | 11 => link(top, right),
                6 | 9 => link(top, bottom),
                7 | 8 => link(left, top),
                // Two corners across from each other: the middle says which pair the bright part joins.
                5 if middle => {
                    link(left, top);
                    link(bottom, right);
                }
                5 => {
                    link(top, right);
                    link(left, bottom);
                }
                10 if middle => {
                    link(top, right);
                    link(left, bottom);
                }
                10 => {
                    link(left, top);
                    link(bottom, right);
                }
                _ => {}
            }
        }
    }
    // Where on its edge the level is crossed.
    let point = |e: usize| -> [f32; 2] {
        let (i, along) = (e / 2, e % 2 == 0);
        let (x, y) = (i % w, i / w);
        let (p, q) = if along { (at(x, y), at(x + 1, y)) } else { (at(x, y), at(x, y + 1)) };
        let f = if (q - p).abs() > 1e-6 { ((level - p) / (q - p)).clamp(0.0, 1.0) } else { 0.5 };
        if along { [x as f32 + f, y as f32] } else { [x as f32, y as f32 + f] }
    };
    // Walk the links: open lines (ending at the picture's edge) first, then closed loops.
    let mut seen = vec![false; links.len()];
    let mut lines: Vec<(Vec<[f32; 2]>, bool)> = Vec::new();
    for open in [true, false] {
        for start in 0..links.len() {
            let l = links[start];
            if seen[start] || l[0] == NONE || open != (l[1] == NONE) {
                continue;
            }
            let mut pts = vec![point(start)];
            seen[start] = true;
            let (mut prev, mut cur) = (start, l[0]);
            let mut closed = false;
            while cur != NONE {
                if cur == start {
                    closed = true;
                    break;
                }
                if seen[cur] {
                    break;
                }
                seen[cur] = true;
                pts.push(point(cur));
                let [n0, n1] = links[cur];
                let next = if n0 == prev { n1 } else { n0 };
                (prev, cur) = (cur, next);
            }
            lines.push((pts, closed));
        }
    }
    let length = |pts: &[[f32; 2]], closed: bool| {
        let mut l: f32 = pts.windows(2).map(|p| dist(p[0], p[1])).sum();
        if closed && pts.len() > 1 {
            l += dist(pts[0], pts[pts.len() - 1]);
        }
        l
    };
    let mut kept: Vec<(f32, Vec<[f32; 2]>, bool)> = lines.into_iter().map(|(p, c)| (length(&p, c), p, c)).filter(|(l, _, _)| *l >= SHORTEST).collect();
    kept.sort_by(|a, b| b.0.total_cmp(&a.0));
    kept.truncate(most);
    let scale = 2.0 / (w.max(h) - 1) as f32;
    let (cx, cy) = ((w - 1) as f32 * 0.5, (h - 1) as f32 * 0.5);
    kept.into_iter()
        .map(|(_, pts, closed)| {
            let pts = simplify(&pts, TOLERANCE);
            let colors = own_colors.then(|| pts.iter().map(|p| bright_colour(pic, &luma, *p)).collect());
            Unit { points: pts.iter().map(|p| [(p[0] - cx) * scale, -(p[1] - cy) * scale]).collect(), colors, closed, dots: false }
        })
        .filter(|u| u.points.len() >= 2)
        .collect()
}

/// A 3 × 3 box blur (edges use what's there).
fn blur(v: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0; v.len()];
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut n) = (0.0, 0.0);
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    sum += v[yy * w + xx];
                    n += 1.0;
                }
            }
            out[y * w + x] = sum / n;
        }
    }
    out
}

/// The colour on the bright side of an outline point (the brightest pixel touching it), at full brightness so the
/// laser shows it; white where the picture is grey or dark.
fn bright_colour(pic: &Picture, luma: &[f32], p: [f32; 2]) -> [f32; 3] {
    let (w, h) = (pic.width, pic.height);
    let (x0, y0) = (p[0].floor().max(0.0) as usize, p[1].floor().max(0.0) as usize);
    let best = [(x0, y0), (x0 + 1, y0), (x0, y0 + 1), (x0 + 1, y0 + 1)]
        .into_iter()
        .filter(|&(x, y)| x < w && y < h)
        .map(|(x, y)| y * w + x)
        .max_by(|a, b| luma[*a].total_cmp(&luma[*b]))
        .unwrap_or(0);
    let c = [0, 1, 2].map(|i| pic.rgba[best * 4 + i] as f32 / 255.0);
    let top = c[0].max(c[1]).max(c[2]);
    let low = c[0].min(c[1]).min(c[2]);
    if top < 0.05 || top - low < 0.08 { [1.0; 3] } else { c.map(|v| v / top) }
}

/// Douglas–Peucker: the fewest corners that stay within `tol` of the line.
fn simplify(pts: &[[f32; 2]], tol: f32) -> Vec<[f32; 2]> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (mut far, mut at) = (0.0f32, 0);
        for i in a + 1..b {
            let d = off_line(pts[i], pts[a], pts[b]);
            if d > far {
                (far, at) = (d, i);
            }
        }
        if far > tol {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// How far `p` is from the line through `a` and `b`.
fn off_line(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return dist(p, a);
    }
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A `w` × `h` black picture with a `rgb` disc in the middle (a third of the height across).
    pub(crate) fn disc(w: usize, h: usize, rgb: [u8; 3]) -> Picture {
        let mut rgba = vec![0u8; w * h * 4];
        let r = h as f32 / 3.0;
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f32 - (w - 1) as f32 / 2.0, y as f32 - (h - 1) as f32 / 2.0);
                let c = if dx * dx + dy * dy < r * r { [rgb[0], rgb[1], rgb[2], 255] } else { [0, 0, 0, 255] };
                rgba[(y * w + x) * 4..][..4].copy_from_slice(&c);
            }
        }
        Picture { width: w, height: h, rgba }
    }

    /// A disc → one closed round outline about its size, in its colour (made bright); a flat picture → nothing;
    /// a bright half → one open line along the split.
    #[test]
    fn traces_bright_shapes() {
        let out = outlines(&disc(64, 36, [255, 120, 0]), 0.3, 10, true);
        assert_eq!(out.len(), 1, "one outline");
        let u = &out[0];
        assert!(u.closed && u.points.len() >= 8 && u.points.len() < 60, "closed, simplified: {} points", u.points.len());
        let r = 12.0 * 2.0 / 63.0;
        for p in &u.points {
            assert!(((p[0] * p[0] + p[1] * p[1]).sqrt() - r).abs() < 0.06, "on the disc's edge: {p:?}");
        }
        let c = u.colors.as_ref().unwrap()[0];
        assert!(c[0] == 1.0 && (c[1] - 120.0 / 255.0).abs() < 0.02 && c[2] == 0.0, "the disc's colour at full brightness: {c:?}");
        assert!(outlines(&disc(64, 36, [255, 120, 0]), 0.3, 10, false)[0].colors.is_none());
        let flat = Picture { width: 16, height: 16, rgba: vec![200; 16 * 16 * 4] };
        assert!(outlines(&flat, 0.5, 10, true).is_empty());
        let mut half = flat.clone();
        for (i, px) in half.rgba.chunks_exact_mut(4).enumerate() {
            px.copy_from_slice(if i % 16 < 8 { &[0, 0, 0, 255] } else { &[255, 255, 255, 255] });
        }
        let split = outlines(&half, 0.5, 10, true);
        assert_eq!(split.len(), 1);
        assert!(!split[0].closed && split[0].points.len() == 2, "a straight line: {:?}", split[0].points);
    }

    /// Only the `most` longest outlines are kept, longest first.
    #[test]
    fn keeps_the_longest() {
        let (w, h) = (60, 20);
        let mut rgba = vec![0u8; w * h * 4];
        for (x0, size) in [(4usize, 4usize), (20, 10), (40, 6)] {
            for y in 5..5 + size.min(14) {
                for x in x0..x0 + size {
                    rgba[(y * w + x) * 4..][..4].copy_from_slice(&[255; 4]);
                }
            }
        }
        let pic = Picture { width: w, height: h, rgba };
        assert_eq!(outlines(&pic, 0.5, 10, false).len(), 3);
        let two = outlines(&pic, 0.5, 2, false);
        assert_eq!(two.len(), 2);
        let span = |u: &Unit| u.points.iter().map(|p| p[0]).fold(f32::MIN, f32::max) - u.points.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        assert!(span(&two[0]) > span(&two[1]), "the biggest square first");
    }
}
