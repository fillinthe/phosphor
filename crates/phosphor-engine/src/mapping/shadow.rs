//! Cast shadows on a 3D model slice: is a point of the model hidden from the light by another part of it? Rays go
//! from the point toward a far light; the model's triangles are sorted into a grid seen from the light, so each ray
//! only tests the few triangles in its cell.

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-9);
    a.map(|v| v / l)
}

const CELLS: usize = 48;

/// The model's triangles as the light sees them.
pub struct Occluders {
    tris: Vec<[V3; 3]>,
    light: V3,
    /// Two directions across the light's view; a point's place in the grid is (p·u, p·w).
    u: V3,
    w: V3,
    min: [f32; 2],
    cell: [f32; 2],
    grid: Vec<Vec<u32>>,
}

impl Occluders {
    /// `tris` in the same space as `light` (the direction toward the light).
    pub fn new(tris: Vec<[V3; 3]>, light: V3) -> Self {
        let light = unit(light);
        let up = if light[1].abs() > 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let u = unit(cross(light, up));
        let w = cross(light, u);
        let flat = |p: V3| [dot(p, u), dot(p, w)];
        let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in tris.iter().flatten() {
            let q = flat(*p);
            for k in 0..2 {
                min[k] = min[k].min(q[k]);
                max[k] = max[k].max(q[k]);
            }
        }
        let cell = [((max[0] - min[0]) / CELLS as f32).max(1e-6), ((max[1] - min[1]) / CELLS as f32).max(1e-6)];
        let mut grid = vec![Vec::new(); CELLS * CELLS];
        let at = |v: f32, k: usize| (((v - min[k]) / cell[k]).floor().max(0.0) as usize).min(CELLS - 1);
        for (i, t) in tris.iter().enumerate() {
            let q = t.map(flat);
            let (x0, x1) = (at(q[0][0].min(q[1][0]).min(q[2][0]), 0), at(q[0][0].max(q[1][0]).max(q[2][0]), 0));
            let (y0, y1) = (at(q[0][1].min(q[1][1]).min(q[2][1]), 1), at(q[0][1].max(q[1][1]).max(q[2][1]), 1));
            for y in y0..=y1 {
                for x in x0..=x1 {
                    grid[y * CELLS + x].push(i as u32);
                }
            }
        }
        Self { tris, light, u, w, min, cell, grid }
    }

    /// True when something lies between `p` and the light.
    pub fn shadowed(&self, p: V3) -> bool {
        let q = [dot(p, self.u), dot(p, self.w)];
        let cx = (q[0] - self.min[0]) / self.cell[0];
        let cy = (q[1] - self.min[1]) / self.cell[1];
        if cx < 0.0 || cy < 0.0 || cx >= CELLS as f32 || cy >= CELLS as f32 {
            return false;
        }
        let cell = &self.grid[cy as usize * CELLS + cx as usize];
        cell.iter().any(|&i| hits(p, self.light, &self.tris[i as usize]))
    }
}

/// Ray from `o` along `d` meets the triangle a little way out (Möller–Trumbore; the start's own face doesn't count).
fn hits(o: V3, d: V3, t: &[V3; 3]) -> bool {
    let (e1, e2) = (sub(t[1], t[0]), sub(t[2], t[0]));
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-9 {
        return false;
    }
    let inv = 1.0 / det;
    let s = sub(o, t[0]);
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = cross(s, e1);
    let v = dot(d, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    dot(e2, q) * inv > 1e-3
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small square floating above a floor, light from straight above: the floor under it is in shadow, the floor
    /// beside it isn't, and the square itself (facing the light) isn't.
    #[test]
    fn a_roof_shades_the_floor_under_it() {
        let quad = |y: f32, r: f32| [[[-r, y, -r], [r, y, -r], [r, y, r]], [[-r, y, -r], [r, y, r], [-r, y, r]]];
        let mut tris = quad(0.0, 1.0).to_vec();
        tris.extend(quad(0.5, 0.25));
        let o = Occluders::new(tris, [0.0, 1.0, 0.0]);
        assert!(o.shadowed([0.0, 0.0, 0.0]));
        assert!(o.shadowed([0.2, 0.0, -0.2]));
        assert!(!o.shadowed([0.6, 0.0, 0.6]));
        assert!(!o.shadowed([0.0, 0.5, 0.0]), "the roof isn't shaded by itself");
        // Light from the side at 45°: the shadow moves over.
        let mut tris = quad(0.0, 1.0).to_vec();
        tris.extend(quad(0.5, 0.25));
        let o = Occluders::new(tris, [1.0, 1.0, 0.0]);
        assert!(!o.shadowed([0.0, 0.0, 0.0]));
        assert!(o.shadowed([-0.5, 0.0, 0.0]));
    }
}
