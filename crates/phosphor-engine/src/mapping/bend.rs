//! Curved slice edges (MadMapper's bezier quads): each edge of a slice gets two handles that bend it, and the
//! picture follows the bend inside (a Coons patch of the four bulges added to the slice's own shape: the perspective
//! quad, or its mesh). Handles are stored as offsets from where they sit on a straight edge (a third of the way
//! along), so all zeros = straight, turning it on changes nothing, and moving a corner carries the curve along.

use serde::{Deserialize, Serialize};

use super::Surface;

/// Two handle offsets per edge: top (TL→TR), right (TR→BR), bottom (BR→BL), left (BL→TL).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Bend {
    pub handles: [[[f32; 2]; 2]; 4],
}

/// How far a cubic bezier through offset handles `a`, `b` strays from the straight edge at `u` (0 at both ends).
fn bulge(a: [f32; 2], b: [f32; 2], u: f32) -> [f32; 2] {
    let (wa, wb) = (3.0 * u * (1.0 - u) * (1.0 - u), 3.0 * u * u * (1.0 - u));
    [a[0] * wa + b[0] * wb, a[1] * wa + b[1] * wb]
}

impl Bend {
    pub fn is_straight(&self) -> bool {
        self.handles.iter().flatten().all(|h| *h == [0.0, 0.0])
    }

    /// The bend at surface point (s, t): each edge's bulge, faded towards the opposite edge.
    pub fn offset(&self, s: f32, t: f32) -> [f32; 2] {
        let h = &self.handles;
        let parts = [
            (bulge(h[0][0], h[0][1], s), 1.0 - t),
            (bulge(h[1][0], h[1][1], t), s),
            (bulge(h[2][0], h[2][1], 1.0 - s), t),
            (bulge(h[3][0], h[3][1], 1.0 - t), 1.0 - s),
        ];
        parts.iter().fold([0.0, 0.0], |acc, (b, w)| [acc[0] + b[0] * w, acc[1] + b[1] * w])
    }

    /// Where handle `k` (edge k / 2, its first or second) sits on the output, for slice `corners`.
    pub fn handle_at(&self, corners: &[[f32; 2]; 4], k: usize) -> [f32; 2] {
        let (edge, which) = (k / 2 % 4, k % 2);
        let (a, b) = (corners[edge], corners[(edge + 1) % 4]);
        let f = if which == 0 { 1.0 / 3.0 } else { 2.0 / 3.0 };
        let o = self.handles[edge][which];
        [a[0] + (b[0] - a[0]) * f + o[0], a[1] + (b[1] - a[1]) * f + o[1]]
    }

    /// Puts handle `k` at output point `p`.
    pub fn set_handle(&mut self, corners: &[[f32; 2]; 4], k: usize, p: [f32; 2]) {
        let straight = Bend::default().handle_at(corners, k);
        self.handles[k / 2 % 4][k % 2] = [p[0] - straight[0], p[1] - straight[1]];
    }

    /// The corner handle `k` hangs from (drawn joined to it).
    pub fn anchor(corners: &[[f32; 2]; 4], k: usize) -> [f32; 2] {
        let edge = k / 2 % 4;
        if k % 2 == 0 { corners[edge] } else { corners[(edge + 1) % 4] }
    }
}

impl Surface {
    /// Where surface point (s, t) lands on the output: its quad (exact perspective) or mesh, bent by `bend`.
    pub fn warp_at(&self, s: f32, t: f32) -> [f32; 2] {
        let base = match &self.mesh {
            Some(m) => m.eval(s, t),
            None => {
                let h = super::square_to_quad(&self.corners);
                let w = h[6] * s + h[7] * t + h[8];
                [(h[0] * s + h[1] * t + h[2]) / w, (h[3] * s + h[4] * t + h[5]) / w]
            }
        };
        match &self.bend {
            Some(b) => {
                let o = b.offset(s, t);
                [base[0] + o[0], base[1] + o[1]]
            }
            None => base,
        }
    }

    /// Drawn as triangles (x, y, s, t): a mesh, or a bent quad. `None` = a plain quad (drawn per pixel instead).
    pub fn warp_triangles(&self) -> Option<Vec<[f32; 4]>> {
        if self.bend.is_none() {
            return self.mesh.as_ref().map(|m| m.triangles());
        }
        let n = self.mesh.as_ref().map_or(32, |m| (m.cols.max(m.rows) * 8).clamp(32, 96));
        let mut grid = Vec::with_capacity(((n + 1) * (n + 1)) as usize);
        for j in 0..=n {
            for i in 0..=n {
                let (s, t) = (i as f32 / n as f32, j as f32 / n as f32);
                let [x, y] = self.warp_at(s, t);
                let [u, v] = self.mesh.as_ref().map_or([s, t], |m| m.eval_input(s, t));
                grid.push([x, y, u, v]);
            }
        }
        let at = |i: u32, j: u32| grid[(j * (n + 1) + i) as usize];
        let mut out = Vec::with_capacity((n * n * 6) as usize);
        for j in 0..n {
            for i in 0..n {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                out.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }
        Some(out)
    }

    /// The slice's outline on the output (`steps` points per edge), following mesh and bend.
    pub fn outline(&self, steps: usize) -> Vec<[f32; 2]> {
        let n = steps.max(1);
        let f = |k: usize| k as f32 / n as f32;
        let mut out = Vec::with_capacity(4 * n);
        out.extend((0..n).map(|k| self.warp_at(f(k), 0.0)));
        out.extend((0..n).map(|k| self.warp_at(1.0, f(k))));
        out.extend((0..n).map(|k| self.warp_at(1.0 - f(k), 1.0)));
        out.extend((0..n).map(|k| self.warp_at(0.0, 1.0 - f(k))));
        out
    }

    /// Curved edges on (straight to start) or off.
    pub fn set_bend(&mut self, on: bool) {
        self.bend = on.then(|| self.bend.unwrap_or_default());
    }
}

/// Is `p` inside the closed polygon `poly`?
pub fn inside(poly: &[[f32; 2]], p: [f32; 2]) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> Surface {
        Surface { corners: [[0.1, 0.2], [0.8, 0.1], [0.9, 0.9], [0.2, 0.7]], ..Default::default() }
    }

    #[test]
    fn straight_handles_change_nothing() {
        let plain = quad();
        let mut bent = quad();
        bent.set_bend(true);
        for (s, t) in [(0.0, 0.0), (0.3, 0.7), (1.0, 0.5), (0.5, 0.5)] {
            let (a, b) = (plain.warp_at(s, t), bent.warp_at(s, t));
            assert!((a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5);
        }
        assert!(bent.bend.unwrap().is_straight());
    }

    #[test]
    fn a_bent_edge_bulges_and_the_corners_stay() {
        let mut s = quad();
        s.set_bend(true);
        let mut b = s.bend.unwrap();
        // Pull both top handles up by 0.1: the top edge's middle rises 0.075 (3/4 of the pull), the corners don't move.
        for k in [0, 1] {
            let at = b.handle_at(&s.corners, k);
            b.set_handle(&s.corners, k, [at[0], at[1] - 0.1]);
        }
        s.bend = Some(b);
        let straight = quad();
        let mid = (s.warp_at(0.5, 0.0), straight.warp_at(0.5, 0.0));
        assert!((mid.1[1] - mid.0[1] - 0.075).abs() < 1e-5, "{mid:?}");
        assert_eq!(s.warp_at(0.0, 0.0), straight.warp_at(0.0, 0.0));
        // The bottom edge isn't touched; halfway down, half the bulge.
        let bottom = (s.warp_at(0.5, 1.0), straight.warp_at(0.5, 1.0));
        assert!((bottom.0[1] - bottom.1[1]).abs() < 1e-6);
        let half = (s.warp_at(0.5, 0.5), straight.warp_at(0.5, 0.5));
        assert!((half.1[1] - half.0[1] - 0.0375).abs() < 1e-5);
        // Its outline is the hit area.
        assert!(inside(&s.outline(16), [0.5, 0.12]), "inside the bulge");
        assert!(!inside(&straight.outline(16), [0.5, 0.12]));
        assert_eq!(s.warp_triangles().unwrap().len(), 32 * 32 * 6);
        assert!(straight.warp_triangles().is_none(), "a plain quad stays per-pixel");
    }
}
