//! Projection mapping: output surfaces that each show a region of the master frame,
//! corner-pinned anywhere on the output (drag the corners onto the real-world object).

use serde::{Deserialize, Serialize};

/// A corner-pinned quad. Coordinates are output-space 0..1 with the origin at the top-left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Surface {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    /// Top-left, top-right, bottom-right, bottom-left.
    pub corners: [[f32; 2]; 4],
    /// What this surface shows: `None` = the mix, `Some(id)` = just that layer (its own picture, before blending).
    pub layer: Option<u64>,
    /// Region of that picture this surface shows: x, y, width, height in 0..1.
    pub input: [f32; 4],
    pub opacity: f32,
    /// Soft-edge width as a fraction of the surface (0 = hard edge). Overlap two surfaces'
    /// feathered edges to blend projectors.
    pub feather: f32,
    /// Curve of the soft edge (1 = straight ramp). Around 2.2 makes two overlapping projectors
    /// add up to even brightness (projectors are gamma-curved).
    pub blend_gamma: f32,
    /// Mesh warp: a grid of points to bend the surface onto curved or uneven objects. `None` = the
    /// plain four-corner pin (exact perspective).
    pub mesh: Option<Mesh>,
    /// This surface's own colour correction (applied before the output's), e.g. two slices on different materials.
    pub color: crate::ColorGrade,
    /// Cut to an ellipse or triangle inside the quad (follows perspective and mesh warp).
    pub shape: SliceShape,
    /// A line slice: the picture stretched along a polyline with a width (MadMapper's Line), for outlines,
    /// edges and LED-like strips on architecture. When set, the corners are not used.
    #[serde(default)]
    pub line: Option<Line>,
}

/// A polyline with a width: the picture's horizontal axis runs along it, the vertical across it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Line {
    /// Output space 0..1.
    pub points: Vec<[f32; 2]>,
    /// Width as a fraction of the output height.
    pub width: f32,
}

impl Line {
    /// Triangles (x, y, s, t) for `fs_mesh`: a strip of quads along the polyline, joined with the bisector at
    /// each inner point so the strip doesn't gap or overlap at corners. `aspect` = output width / height.
    pub fn triangles(&self, aspect: f32) -> Vec<[f32; 4]> {
        let pts = &self.points;
        if pts.len() < 2 {
            return Vec::new();
        }
        let ax = aspect.max(0.1);
        // Work in "square" space (x scaled by aspect) so the width is the same in both directions.
        let sq: Vec<[f32; 2]> = pts.iter().map(|p| [p[0] * ax, p[1]]).collect();
        let mut len = vec![0.0f32];
        for w in sq.windows(2) {
            let d = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
            len.push(len.last().unwrap() + d);
        }
        let total = (*len.last().unwrap()).max(1e-6);
        let half = self.width.max(0.001) * 0.5;
        // Offset direction at each point: the normal of the segment (ends) or the bisector (inner points).
        let normal = |a: [f32; 2], b: [f32; 2]| -> [f32; 2] {
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = (dx * dx + dy * dy).sqrt().max(1e-6);
            [-dy / l, dx / l]
        };
        let n = sq.len();
        let mut edges: Vec<([f32; 2], [f32; 2])> = Vec::with_capacity(n);
        for i in 0..n {
            let dir = if i == 0 {
                normal(sq[0], sq[1])
            } else if i == n - 1 {
                normal(sq[n - 2], sq[n - 1])
            } else {
                let (n1, n2) = (normal(sq[i - 1], sq[i]), normal(sq[i], sq[i + 1]));
                let m = [n1[0] + n2[0], n1[1] + n2[1]];
                let ml = (m[0] * m[0] + m[1] * m[1]).sqrt();
                if ml < 1e-4 {
                    n2
                } else {
                    // Miter length, capped so sharp corners don't spike.
                    let cos_half = (m[0] * n1[0] + m[1] * n1[1]) / ml;
                    let k = (1.0 / cos_half.max(0.35)).min(3.0);
                    [m[0] / ml * k, m[1] / ml * k]
                }
            };
            let p = sq[i];
            let left = [(p[0] + dir[0] * half) / ax, p[1] + dir[1] * half];
            let right = [(p[0] - dir[0] * half) / ax, p[1] - dir[1] * half];
            edges.push((left, right));
        }
        let mut out = Vec::with_capacity((n - 1) * 6);
        for i in 0..n - 1 {
            let (s0, s1) = (len[i] / total, len[i + 1] / total);
            let (l0, r0) = edges[i];
            let (l1, r1) = edges[i + 1];
            out.push([l0[0], l0[1], s0, 0.0]);
            out.push([r0[0], r0[1], s0, 1.0]);
            out.push([l1[0], l1[1], s1, 0.0]);
            out.push([r0[0], r0[1], s0, 1.0]);
            out.push([r1[0], r1[1], s1, 1.0]);
            out.push([l1[0], l1[1], s1, 0.0]);
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SliceShape {
    #[default]
    Rectangle,
    Ellipse,
    Triangle,
}

impl SliceShape {
    pub const ALL: [SliceShape; 3] = [SliceShape::Rectangle, SliceShape::Ellipse, SliceShape::Triangle];

    pub fn name(self) -> &'static str {
        match self {
            SliceShape::Rectangle => "Rectangle",
            SliceShape::Ellipse => "Ellipse",
            SliceShape::Triangle => "Triangle",
        }
    }
}

impl crate::model::Choice for SliceShape {
    const OPTIONS: &'static [Self] = &SliceShape::ALL;
    fn label(self) -> &'static str {
        self.name()
    }
}

/// A warp grid: `(cols + 1) × (rows + 1)` points in output space, row by row from the top-left.
/// The picture follows a smooth curve through the points (Catmull-Rom), not straight lines.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    pub cols: u32,
    pub rows: u32,
    pub points: Vec<[f32; 2]>,
}

impl Mesh {
    /// A grid laid onto the four corners (following their perspective), so turning mesh on changes nothing.
    pub fn from_corners(corners: &[[f32; 2]; 4], cols: u32, rows: u32) -> Self {
        let (cols, rows) = (cols.clamp(1, 16), rows.clamp(1, 16));
        let h = square_to_quad(corners);
        let mut points = Vec::with_capacity(((cols + 1) * (rows + 1)) as usize);
        for j in 0..=rows {
            for i in 0..=cols {
                let (s, t) = (i as f32 / cols as f32, j as f32 / rows as f32);
                let w = h[6] * s + h[7] * t + h[8];
                points.push([(h[0] * s + h[1] * t + h[2]) / w, (h[3] * s + h[4] * t + h[5]) / w]);
            }
        }
        Self { cols, rows, points }
    }

    pub fn point(&self, i: u32, j: u32) -> [f32; 2] {
        let i = i.min(self.cols);
        let j = j.min(self.rows);
        self.points.get((j * (self.cols + 1) + i) as usize).copied().unwrap_or([0.0, 0.0])
    }

    /// The four outer corners (TL, TR, BR, BL).
    pub fn corners(&self) -> [[f32; 2]; 4] {
        [self.point(0, 0), self.point(self.cols, 0), self.point(self.cols, self.rows), self.point(0, self.rows)]
    }

    /// Where surface coordinate (s, t) in 0..1 lands, on a smooth curve through the points.
    pub fn eval(&self, s: f32, t: f32) -> [f32; 2] {
        let fx = s.clamp(0.0, 1.0) * self.cols as f32;
        let fy = t.clamp(0.0, 1.0) * self.rows as f32;
        let (i, j) = ((fx.floor() as i32).min(self.cols as i32 - 1), (fy.floor() as i32).min(self.rows as i32 - 1));
        let (u, v) = (fx - i as f32, fy - j as f32);
        let p = |a: i32, b: i32| self.point(a.clamp(0, self.cols as i32) as u32, b.clamp(0, self.rows as i32) as u32);
        let row = |b: i32| {
            let q = [p(i - 1, b), p(i, b), p(i + 1, b), p(i + 2, b)];
            [catmull(q[0][0], q[1][0], q[2][0], q[3][0], u), catmull(q[0][1], q[1][1], q[2][1], q[3][1], u)]
        };
        let r = [row(j - 1), row(j), row(j + 1), row(j + 2)];
        [catmull(r[0][0], r[1][0], r[2][0], r[3][0], v), catmull(r[0][1], r[1][1], r[2][1], r[3][1], v)]
    }

    /// The same shape on a finer or coarser grid.
    pub fn resized(&self, cols: u32, rows: u32) -> Self {
        let (cols, rows) = (cols.clamp(1, 16), rows.clamp(1, 16));
        let mut points = Vec::with_capacity(((cols + 1) * (rows + 1)) as usize);
        for j in 0..=rows {
            for i in 0..=cols {
                points.push(self.eval(i as f32 / cols as f32, j as f32 / rows as f32));
            }
        }
        Self { cols, rows, points }
    }

    /// Triangles for drawing: each vertex is (x, y, s, t). Subdivided finely enough to look smooth.
    pub fn triangles(&self) -> Vec<[f32; 4]> {
        let nx = (self.cols * 8).clamp(8, 96);
        let ny = (self.rows * 8).clamp(8, 96);
        let mut grid = Vec::with_capacity(((nx + 1) * (ny + 1)) as usize);
        for j in 0..=ny {
            for i in 0..=nx {
                let (s, t) = (i as f32 / nx as f32, j as f32 / ny as f32);
                let [x, y] = self.eval(s, t);
                grid.push([x, y, s, t]);
            }
        }
        let at = |i: u32, j: u32| grid[(j * (nx + 1) + i) as usize];
        let mut out = Vec::with_capacity((nx * ny * 6) as usize);
        for j in 0..ny {
            for i in 0..nx {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                out.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }
        out
    }
}

fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t2 * t)
}

/// A shape on an output that blacks out what's inside it (or, inverted, everything outside it):
/// keeps light off doors, faces, the DJ booth…
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mask {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    /// Polygon points in output space 0..1.
    pub points: Vec<[f32; 2]>,
    /// Black out everything *outside* the shape instead.
    pub invert: bool,
    /// Soft edge width in output space (0 = hard edge); the fade is centred on the outline.
    pub feather: f32,
    /// The outline is a smooth curve through the points (Catmull-Rom) instead of straight lines.
    pub smooth: bool,
}

impl Default for Mask {
    fn default() -> Self {
        Self { id: 0, name: "Mask".into(), enabled: true, points: vec![[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]], invert: false, feather: 0.0, smooth: false }
    }
}

/// Starting shapes for a new mask (every point can be dragged afterwards).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskShape {
    Rectangle,
    Circle,
    Triangle,
}

impl MaskShape {
    pub const ALL: [MaskShape; 3] = [MaskShape::Rectangle, MaskShape::Circle, MaskShape::Triangle];

    pub fn name(self) -> &'static str {
        match self {
            MaskShape::Rectangle => "Rectangle",
            MaskShape::Circle => "Circle",
            MaskShape::Triangle => "Triangle",
        }
    }

    /// Outline points (output space 0..1) centred, about a fifth of the output tall.
    pub fn points(self, aspect: f32) -> Vec<[f32; 2]> {
        let (r, rx) = (0.2, 0.2 / aspect.max(0.1));
        match self {
            MaskShape::Rectangle => vec![[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]],
            MaskShape::Circle => (0..32).map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                [0.5 + rx * a.cos(), 0.5 + r * a.sin()]
            }).collect(),
            MaskShape::Triangle => vec![[0.5, 0.5 - r], [0.5 + rx * 1.15, 0.5 + r * 0.6], [0.5 - rx * 1.15, 0.5 + r * 0.6]],
        }
    }
}

impl crate::model::Choice for MaskShape {
    const OPTIONS: &'static [Self] = &MaskShape::ALL;
    fn label(self) -> &'static str {
        self.name()
    }
}

/// Triangles filling `outer` (minus `hole`), each vertex (x, y, darkness, 0).
fn fill(outer: &[[f32; 2]], hole: Option<&[[f32; 2]]>, darkness: f32) -> Vec<[f32; 4]> {
    let mut data: Vec<f64> = outer.iter().flat_map(|p| [p[0] as f64, p[1] as f64]).collect();
    let mut holes = Vec::new();
    if let Some(h) = hole {
        holes.push(outer.len());
        data.extend(h.iter().flat_map(|p| [p[0] as f64, p[1] as f64]));
    }
    let Ok(idx) = earcutr::earcut(&data, &holes, 2) else { return Vec::new() };
    idx.into_iter().map(|i| [data[i * 2] as f32, data[i * 2 + 1] as f32, darkness, 0.0]).collect()
}

/// The polygon moved `d` outward (negative = inward), corners mitred (limited, so sharp corners don't spike).
pub fn offset_polygon(points: &[[f32; 2]], d: f32) -> Vec<[f32; 2]> {
    let n = points.len();
    let area: f32 = (0..n).map(|i| points[i][0] * points[(i + 1) % n][1] - points[(i + 1) % n][0] * points[i][1]).sum();
    let outward = |a: [f32; 2], b: [f32; 2]| {
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let len = (ex * ex + ey * ey).sqrt().max(1e-9);
        if area > 0.0 { [ey / len, -ex / len] } else { [-ey / len, ex / len] }
    };
    (0..n)
        .map(|i| {
            let (p0, p1, p2) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            let (n1, n2) = (outward(p0, p1), outward(p1, p2));
            let m = [n1[0] + n2[0], n1[1] + n2[1]];
            let ml = (m[0] * m[0] + m[1] * m[1]).sqrt();
            if ml < 1e-6 {
                return [p1[0] + n1[0] * d, p1[1] + n1[1] * d];
            }
            let m = [m[0] / ml, m[1] / ml];
            let k = d / (m[0] * n1[0] + m[1] * n1[1]).max(0.3);
            [p1[0] + m[0] * k, p1[1] + m[1] * k]
        })
        .collect()
}

impl Mask {
    /// The outline as drawn: the points, or a smooth closed curve through them (12 steps per side).
    pub fn outline(&self) -> Vec<[f32; 2]> {
        let n = self.points.len();
        if !self.smooth || n < 3 {
            return self.points.clone();
        }
        const STEPS: usize = 12;
        let p = |i: usize| self.points[i % n];
        let mut out = Vec::with_capacity(n * STEPS);
        for i in 0..n {
            let (p0, p1, p2, p3) = (p(i + n - 1), p(i), p(i + 1), p(i + 2));
            for k in 0..STEPS {
                let t = k as f32 / STEPS as f32;
                out.push([catmull(p0[0], p1[0], p2[0], p3[0], t), catmull(p0[1], p1[1], p2[1], p3[1], t)]);
            }
        }
        out
    }

    /// Triangles (x, y, darkness, 0) covering what this mask blacks out; with a feather, a band along the
    /// outline fades from black to clear.
    pub fn triangles(&self) -> Vec<[f32; 4]> {
        if self.points.len() < 3 {
            return Vec::new();
        }
        let outline = self.outline();
        let points = &outline;
        const OUTPUT: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let half = self.feather.max(0.0) / 2.0;
        if half <= 0.0 {
            // Inverted: the whole output with the shape cut out as a hole.
            return if self.invert { fill(&OUTPUT, Some(points), 1.0) } else { fill(points, None, 1.0) };
        }
        let (inner, outer) = (offset_polygon(points, -half), offset_polygon(points, half));
        let (mut out, d_in, d_out) = if self.invert { (fill(&OUTPUT, Some(&outer), 1.0), 0.0, 1.0) } else { (fill(&inner, None, 1.0), 1.0, 0.0) };
        let n = inner.len();
        for i in 0..n {
            let j = (i + 1) % n;
            let (a, b, c, d) = ([inner[i][0], inner[i][1], d_in, 0.0], [inner[j][0], inner[j][1], d_in, 0.0], [outer[j][0], outer[j][1], d_out, 0.0], [outer[i][0], outer[i][1], d_out, 0.0]);
            out.extend_from_slice(&[a, b, c, a, c, d]);
        }
        out
    }
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Surface".into(),
            enabled: true,
            corners: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            layer: None,
            input: [0.0, 0.0, 1.0, 1.0],
            opacity: 1.0,
            feather: 0.0,
            blend_gamma: 1.0,
            mesh: None,
            color: crate::ColorGrade::default(),
            shape: SliceShape::Rectangle,
            line: None,
        }
    }
}

impl Surface {
    /// A surface covering the middle of the output, so a new one is visible and grabbable.
    pub fn new(id: u64, index: usize) -> Self {
        let inset = 0.15 + 0.05 * (index % 4) as f32;
        Self {
            id,
            name: format!("Surface {}", index + 1),
            corners: [[inset, inset], [1.0 - inset, inset], [1.0 - inset, 1.0 - inset], [inset, 1.0 - inset]],
            ..Default::default()
        }
    }

    pub fn center(&self) -> [f32; 2] {
        if let Some(l) = &self.line
            && !l.points.is_empty()
        {
            let n = l.points.len() as f32;
            return l.points.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
        }
        let c = &self.corners;
        [(c[0][0] + c[1][0] + c[2][0] + c[3][0]) * 0.25, (c[0][1] + c[1][1] + c[2][1] + c[3][1]) * 0.25]
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        for c in self.corners.iter_mut().chain(self.mesh.iter_mut().flat_map(|m| m.points.iter_mut())).chain(self.line.iter_mut().flat_map(|l| l.points.iter_mut())) {
            c[0] += dx;
            c[1] += dy;
        }
    }

    /// Turns the line slice on (a line along the top edge of the current quad) or off.
    pub fn set_line(&mut self, on: bool) {
        if on {
            if self.line.is_none() {
                let c = self.corners;
                let mid = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
                self.line = Some(Line { points: vec![mid(c[0], c[3]), mid(c[1], c[2])], width: 0.06 });
                self.mesh = None;
            }
        } else {
            self.line = None;
        }
    }

    /// Turns mesh warp on (grid laid onto the current corners) or off (back to the corners).
    pub fn set_mesh(&mut self, on: bool, cols: u32, rows: u32) {
        match (on, &self.mesh) {
            (true, Some(m)) => self.mesh = Some(m.resized(cols, rows)),
            (true, None) => self.mesh = Some(Mesh::from_corners(&self.corners, cols, rows)),
            (false, Some(m)) => {
                self.corners = m.corners();
                self.mesh = None;
            }
            (false, None) => {}
        }
    }

    /// Keeps the corners (used for picking and moving) on the mesh's outer points.
    pub fn sync_corners(&mut self) {
        if let Some(m) = &self.mesh {
            self.corners = m.corners();
        }
    }

    /// True if an output-space point lies inside the quad.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        output_to_surface(&self.corners, p).is_some_and(|st| (0.0..=1.0).contains(&st[0]) && (0.0..=1.0).contains(&st[1]))
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Mapping {
    pub enabled: bool,
    pub surfaces: Vec<Surface>,
    /// Blackout shapes drawn over everything on this output.
    pub masks: Vec<Mask>,
    /// Show an alignment grid on every surface instead of the content.
    pub test_pattern: bool,
}

impl Mapping {
    pub fn add_surface(&mut self, id: u64) -> &mut Surface {
        let surface = Surface::new(id, self.surfaces.len());
        self.surfaces.push(surface);
        self.surfaces.last_mut().unwrap()
    }

    /// Layers that enabled surfaces show on their own (they need a full-size picture rendered).
    pub fn layer_feeds(&self) -> impl Iterator<Item = u64> + '_ {
        self.surfaces.iter().filter(|s| self.enabled && s.enabled).filter_map(|s| s.layer)
    }

    /// Surfaces are drawn when mapping is on and there is at least one enabled.
    pub fn active(&self) -> bool {
        self.enabled && self.surfaces.iter().any(|s| s.enabled)
    }

    pub fn add_mask(&mut self, id: u64) -> &mut Mask {
        self.add_mask_shape(id, MaskShape::Rectangle, 16.0 / 9.0)
    }

    /// A new mask of `shape` in the middle of an output `aspect` (width / height) wide, so circles look round.
    pub fn add_mask_shape(&mut self, id: u64, shape: MaskShape, aspect: f32) -> &mut Mask {
        let n = self.masks.len() + 1;
        self.masks.push(Mask { id, name: format!("{} {n}", shape.name()), points: shape.points(aspect), ..Default::default() });
        self.masks.last_mut().unwrap()
    }

    /// Everything the enabled masks black out, as one triangle list.
    pub fn mask_triangles(&self) -> Vec<[f32; 4]> {
        self.masks.iter().filter(|m| m.enabled).flat_map(Mask::triangles).collect()
    }
}

/// Row-major 3×3 homography taking the unit square (s, t) onto `corners`
/// (Heckbert's square-to-quad), so `(x, y, w) = H · (s, t, 1)`.
pub fn square_to_quad(c: &[[f32; 2]; 4]) -> [f32; 9] {
    let (x0, y0) = (c[0][0], c[0][1]);
    let (x1, y1) = (c[1][0], c[1][1]);
    let (x2, y2) = (c[2][0], c[2][1]);
    let (x3, y3) = (c[3][0], c[3][1]);
    let dx1 = x1 - x2;
    let dx2 = x3 - x2;
    let dx3 = x0 - x1 + x2 - x3;
    let dy1 = y1 - y2;
    let dy2 = y3 - y2;
    let dy3 = y0 - y1 + y2 - y3;
    if dx3.abs() < 1e-7 && dy3.abs() < 1e-7 {
        return [x1 - x0, x3 - x0, x0, y1 - y0, y3 - y0, y0, 0.0, 0.0, 1.0];
    }
    let det = dx1 * dy2 - dx2 * dy1;
    let g = (dx3 * dy2 - dx2 * dy3) / det;
    let h = (dx1 * dy3 - dx3 * dy1) / det;
    [x1 - x0 + g * x1, x3 - x0 + h * x3, x0, y1 - y0 + g * y1, y3 - y0 + h * y3, y0, g, h, 1.0]
}

pub fn invert3(m: &[f32; 9]) -> Option<[f32; 9]> {
    let [a, b, c, d, e, f, g, h, i] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    Some([
        (e * i - f * h) * inv,
        (c * h - b * i) * inv,
        (b * f - c * e) * inv,
        (f * g - d * i) * inv,
        (a * i - c * g) * inv,
        (c * d - a * f) * inv,
        (d * h - e * g) * inv,
        (b * g - a * h) * inv,
        (a * e - b * d) * inv,
    ])
}

/// Homography from output space back to surface-local (s, t); `None` if the quad is degenerate.
pub fn quad_to_square(corners: &[[f32; 2]; 4]) -> Option<[f32; 9]> {
    invert3(&square_to_quad(corners))
}

pub fn output_to_surface(corners: &[[f32; 2]; 4], p: [f32; 2]) -> Option<[f32; 2]> {
    let m = quad_to_square(corners)?;
    let w = m[6] * p[0] + m[7] * p[1] + m[8];
    if w.abs() < 1e-9 {
        return None;
    }
    Some([(m[0] * p[0] + m[1] * p[1] + m[2]) / w, (m[3] * p[0] + m[4] * p[1] + m[5]) / w])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
    }

    #[test]
    fn corners_map_back_to_unit_square() {
        // A clearly perspective quad (keystoned).
        let corners = [[0.1, 0.2], [0.9, 0.05], [0.8, 0.95], [0.2, 0.7]];
        let unit = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for (c, u) in corners.iter().zip(unit) {
            let st = output_to_surface(&corners, *c).unwrap();
            assert!(close(st, u), "{c:?} -> {st:?}, expected {u:?}");
        }
        // Affine case takes the shortcut path and must agree too.
        let rect = [[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]];
        assert!(close(output_to_surface(&rect, [0.5, 0.5]).unwrap(), [0.5, 0.5]));
        assert!(Surface { corners: rect, ..Default::default() }.contains([0.3, 0.3]));
        assert!(!Surface { corners: rect, ..Default::default() }.contains([0.1, 0.3]));
    }

    #[test]
    fn mesh_from_corners_matches_the_pin_and_passes_through_points() {
        let corners = [[0.1, 0.2], [0.9, 0.05], [0.8, 0.95], [0.2, 0.7]];
        let mut m = Mesh::from_corners(&corners, 4, 3);
        assert_eq!(m.points.len(), 5 * 4);
        assert!(close(m.corners()[2], corners[2]));
        // The smooth curve goes exactly through every control point.
        m.points[6] = [0.33, 0.42];
        assert!(close(m.eval(1.0 / 4.0, 1.0 / 3.0), [0.33, 0.42]));
        // Resizing keeps the outline.
        let r = m.resized(8, 8);
        assert!(close(r.corners()[0], corners[0]) && close(r.corners()[2], corners[2]));
        assert_eq!(m.triangles().len() % 3, 0);
    }

    #[test]
    fn masks_triangulate_and_invert() {
        let square = Mask { points: vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]], ..Default::default() };
        let area = |t: &[[f32; 4]]| t.chunks(3).map(|c| ((c[1][0] - c[0][0]) * (c[2][1] - c[0][1]) - (c[2][0] - c[0][0]) * (c[1][1] - c[0][1])).abs() * 0.5).sum::<f32>();
        assert!((area(&square.triangles()) - 0.25).abs() < 1e-4);
        let inverted = Mask { invert: true, ..square };
        assert!((area(&inverted.triangles()) - 0.75).abs() < 1e-4);
    }

    #[test]
    fn degenerate_quad_is_rejected() {
        let line = [[0.0, 0.0], [1.0, 0.0], [1.0, 0.0], [0.0, 0.0]];
        assert!(quad_to_square(&line).is_none());
    }
}

#[cfg(test)]
mod feather_tests {
    use super::*;

    #[test]
    fn offset_moves_a_square_in_and_out_whichever_way_it_was_drawn() {
        let sq = vec![[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]];
        let rev: Vec<[f32; 2]> = sq.iter().rev().copied().collect();
        for poly in [sq, rev] {
            let grown = offset_polygon(&poly, 0.05);
            let shrunk = offset_polygon(&poly, -0.05);
            let span = |p: &[[f32; 2]]| p.iter().map(|q| q[0]).fold(f32::MIN, f32::max) - p.iter().map(|q| q[0]).fold(f32::MAX, f32::min);
            assert!((span(&grown) - 0.3).abs() < 1e-4 && (span(&shrunk) - 0.1).abs() < 1e-4, "{grown:?} {shrunk:?}");
        }
    }

    #[test]
    fn mask_shapes_start_in_the_middle_and_circles_are_round_on_screen() {
        for shape in MaskShape::ALL {
            let pts = shape.points(16.0 / 9.0);
            assert!(pts.len() >= 3 && pts.iter().all(|p| (0.2..0.8).contains(&p[0]) && (0.2..0.8).contains(&p[1])), "{shape:?}");
        }
        let c = MaskShape::Circle.points(16.0 / 9.0);
        let w = c.iter().map(|p| p[0]).fold(f32::MIN, f32::max) - c.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        let h = c.iter().map(|p| p[1]).fold(f32::MIN, f32::max) - c.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        assert!((w * 1920.0 - h * 1080.0).abs() < 2.0, "{} px wide vs {} px tall", w * 1920.0, h * 1080.0);
    }

    #[test]
    fn feathered_mask_fades_across_its_edge() {
        let hard = Mask::default();
        assert!(hard.triangles().iter().all(|v| v[2] == 1.0));
        let soft = Mask { feather: 0.04, ..Default::default() };
        let t = soft.triangles();
        assert!(t.iter().any(|v| v[2] == 0.0) && t.iter().any(|v| v[2] == 1.0));
        // The clear edge sits outside the shape, the solid one inside.
        assert!(t.iter().filter(|v| v[2] == 0.0).all(|v| v[0] < 0.4 || v[0] > 0.6 || v[1] < 0.4 || v[1] > 0.6));
        let inv = Mask { feather: 0.04, invert: true, ..Default::default() };
        assert!(inv.triangles().iter().filter(|v| v[2] == 0.0).all(|v| v[0] > 0.4 && v[0] < 0.6 && v[1] > 0.4 && v[1] < 0.6));
    }
}

#[cfg(test)]
mod line_tests {
    use super::Line;

    #[test]
    fn a_line_becomes_a_strip_with_texture_along_it() {
        let l = Line { points: vec![[0.1, 0.5], [0.5, 0.5], [0.9, 0.5]], width: 0.1 };
        let t = l.triangles(1.0);
        assert_eq!(t.len(), 12, "two segments, two triangles each");
        // s runs 0 → 1 along the line, t is 0 or 1 across it.
        assert!(t.iter().all(|v| (0.0..=1.0).contains(&v[2]) && (v[3] == 0.0 || v[3] == 1.0)));
        assert!(t.iter().any(|v| v[2] == 0.0) && t.iter().any(|v| (v[2] - 1.0).abs() < 1e-6));
        // The strip is the width tall, centred on the line.
        let ys: Vec<f32> = t.iter().map(|v| v[1]).collect();
        let (lo, hi) = (ys.iter().cloned().fold(1.0, f32::min), ys.iter().cloned().fold(0.0, f32::max));
        assert!((lo - 0.45).abs() < 1e-5 && (hi - 0.55).abs() < 1e-5, "{lo} {hi}");
        // A wider output: the width stays a fraction of the height, so x offsets shrink by the aspect.
        let v = Line { points: vec![[0.5, 0.2], [0.5, 0.8]], width: 0.1 }.triangles(2.0);
        let xs: Vec<f32> = v.iter().map(|p| p[0]).collect();
        let (lo, hi) = (xs.iter().cloned().fold(1.0, f32::min), xs.iter().cloned().fold(0.0, f32::max));
        assert!((hi - lo - 0.05).abs() < 1e-5, "{lo} {hi}");
        assert!(Line { points: vec![[0.1, 0.1]], width: 0.1 }.triangles(1.0).is_empty());
    }
}
