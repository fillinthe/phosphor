//! Projection mapping: output surfaces that each show a region of the master frame,
//! corner-pinned anywhere on the output (drag the corners onto the real-world object).

use serde::{Deserialize, Serialize};

mod groups;
pub use groups::SliceGroup;

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
    /// 3D-model mapping: a model (OBJ) seen through a virtual camera placed like the projector, textured with
    /// the picture through the model's own UVs. When set, the corners are not used.
    #[serde(default)]
    pub model: Option<Model3d>,
    /// Soft edge per side (left, right, top, bottom; fractions of the surface) instead of `feather` on all four:
    /// for a projector that overlaps its neighbour on one side only.
    #[serde(default)]
    pub edges: Option<[f32; 4]>,
    /// Locked: its points can't be dragged or nudged (nothing moves by accident once it's lined up).
    #[serde(default)]
    pub locked: bool,
    /// How it lands on the slices under it.
    #[serde(default)]
    pub blend: SliceBlend,
    /// The slice group it's in (`Mapping::groups`), if any.
    #[serde(default)]
    pub group: Option<u64>,
}

/// A 3D model on a surface and the virtual camera that lines it up with the real object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Model3d {
    /// The .obj file.
    pub path: String,
    /// Model rotation in turns (0..1 = a full turn) around the vertical, the horizontal and the view axis.
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    /// Camera distance in model sizes (the model is scaled to fit a unit cube).
    pub distance: f32,
    /// Vertical field of view of the projector lens, degrees.
    pub fov: f32,
    /// Where the model's centre lands on the output (0..1), and its size.
    pub center: [f32; 2],
    pub scale: f32,
    /// Hide triangles facing away from the projector.
    pub cull: bool,
    /// Calibration: model points pinned to where they really are on the output (4+ solve the projector).
    pub pins: Vec<Pin>,
    /// Calibrate mode is on (the canvas shows the model's points to pin).
    pub calibrating: bool,
}

/// A point of the model (in the model's unit-cube space) and where it must land on the output (0..1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    pub model: [f32; 3],
    pub at: [f32; 2],
}

impl Default for Model3d {
    fn default() -> Self {
        Self { path: String::new(), yaw: 0.08, pitch: 0.05, roll: 0.0, distance: 3.0, fov: 30.0, center: [0.5, 0.5], scale: 1.0, cull: true, pins: Vec::new(), calibrating: false }
    }
}

/// A loaded model: triangles of (position, uv), centred and scaled into a unit cube.
pub struct ObjMesh {
    pub tris: Vec<[([f32; 3], [f32; 2]); 3]>,
    /// Every distinct vertex position (unit-cube space): the points calibration can pin.
    pub verts: Vec<[f32; 3]>,
}

static OBJ_CACHE: std::sync::Mutex<Vec<(String, std::sync::Arc<Result<ObjMesh, String>>)>> = std::sync::Mutex::new(Vec::new());

/// A model from disk, read once (failures are remembered too, so a missing file isn't re-read every frame).
pub fn obj_mesh(path: &str) -> std::sync::Arc<Result<ObjMesh, String>> {
    let mut cache = OBJ_CACHE.lock().unwrap();
    if let Some((_, m)) = cache.iter().find(|(p, _)| p == path) {
        return m.clone();
    }
    let m = std::sync::Arc::new(std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| parse_obj(&t)));
    cache.push((path.to_string(), m.clone()));
    m
}

/// Forget a cached model (after the file changed).
pub fn forget_obj(path: &str) {
    OBJ_CACHE.lock().unwrap().retain(|(p, _)| p != path);
}

/// Wavefront OBJ: `v`, `vt` and `f` (triangles and polygons, fanned; `v`, `v/vt`, `v/vt/vn`, `v//vn`, negative
/// indices). Without texture coordinates the picture is laid on from the front.
pub fn parse_obj(text: &str) -> Result<ObjMesh, String> {
    let mut v: Vec<[f32; 3]> = Vec::new();
    let mut vt: Vec<[f32; 2]> = Vec::new();
    let mut faces: Vec<Vec<(usize, Option<usize>)>> = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let n: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if n.len() == 3 {
                    v.push([n[0], n[1], n[2]]);
                }
            }
            Some("vt") => {
                let n: Vec<f32> = it.take(2).filter_map(|x| x.parse().ok()).collect();
                if n.len() == 2 {
                    vt.push([n[0], n[1]]);
                }
            }
            Some("f") => {
                let idx = |s: &str, len: usize| -> Option<usize> {
                    let i: i64 = s.parse().ok()?;
                    let k = if i < 0 { len as i64 + i } else { i - 1 };
                    (k >= 0 && (k as usize) < len).then_some(k as usize)
                };
                let face: Vec<(usize, Option<usize>)> = it
                    .filter_map(|c| {
                        let mut p = c.split('/');
                        let vi = idx(p.next()?, v.len())?;
                        let ti = p.next().filter(|s| !s.is_empty()).and_then(|s| idx(s, vt.len()));
                        Some((vi, ti))
                    })
                    .collect();
                if face.len() >= 3 {
                    faces.push(face);
                }
            }
            _ => {}
        }
    }
    if faces.is_empty() {
        return Err("no faces in the model".into());
    }
    // Centre and fit into a unit cube.
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &v {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max).max(1e-6);
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let norm = |p: [f32; 3]| [(p[0] - c[0]) / size, (p[1] - c[1]) / size, (p[2] - c[2]) / size];
    let front = |p: [f32; 3]| [(p[0] - lo[0]) / (hi[0] - lo[0]).max(1e-6), (p[1] - lo[1]) / (hi[1] - lo[1]).max(1e-6)];
    let mut tris = Vec::new();
    for f in &faces {
        let vert = |(vi, ti): (usize, Option<usize>)| (norm(v[vi]), ti.map(|t| vt[t]).unwrap_or_else(|| front(v[vi])));
        for k in 1..f.len() - 1 {
            tris.push([vert(f[0]), vert(f[k]), vert(f[k + 1])]);
        }
    }
    let mut verts: Vec<[f32; 3]> = v.iter().map(|p| norm(*p)).collect();
    verts.dedup_by(|a, b| a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-6));
    Ok(ObjMesh { tris, verts })
}

impl Model3d {
    /// The model's triangles projected onto the output: (x, y, s, t) for `fs_mesh`, back faces culled and the
    /// rest sorted far to near (no depth buffer needed for most objects). `aspect` = output width / height.
    pub fn triangles(&self, aspect: f32) -> Vec<[f32; 4]> {
        self.project(aspect, true)
    }

    /// The model's own triangles projected (not split): for a wireframe on the mapping canvas.
    pub fn wireframe(&self, aspect: f32) -> Vec<[f32; 4]> {
        self.project(aspect, false)
    }

    /// Where a model point (unit-cube space) lands on the output (0..1), and its depth; None behind the projector.
    pub fn project_point(&self, p: [f32; 3], aspect: f32) -> Option<([f32; 2], f32)> {
        let tau = std::f32::consts::TAU;
        let (sy, cy) = (self.yaw * tau).sin_cos();
        let (sp, cp) = (self.pitch * tau).sin_cos();
        let (sr, cr) = (self.roll * tau).sin_cos();
        let f = 1.0 / (self.fov.clamp(5.0, 150.0).to_radians() * 0.5).tan();
        let dist = self.distance.max(0.6);
        let (x, y, z) = (p[0] * self.scale, p[1] * self.scale, p[2] * self.scale);
        let (x, z) = (x * cy + z * sy, -x * sy + z * cy);
        let (y, z) = (y * cp - z * sp, y * sp + z * cp);
        let (x, y) = (x * cr - y * sr, x * sr + y * cr);
        let depth = dist - z;
        if depth < 0.05 {
            return None;
        }
        let (px, py) = (x * f / depth / aspect.max(0.1), y * f / depth);
        Some(([self.center[0] + px * 0.5, self.center[1] - py * 0.5], depth))
    }

    /// Solves the virtual projector (turn, tilt, roll, distance, centre, and the lens from 6 pins up) so every pin's
    /// model point lands where it was pinned: damped least squares (Levenberg–Marquardt) from the current
    /// settings. Needs 4 pins; returns the remaining error in output fractions (RMS), or None if not solvable.
    pub fn solve(&mut self, aspect: f32) -> Option<f32> {
        if self.pins.len() < 4 {
            return None;
        }
        let with_fov = self.pins.len() >= 6;
        let get = |m: &Model3d| -> Vec<f64> {
            let mut x = vec![m.yaw as f64, m.pitch as f64, m.roll as f64, (m.distance as f64).ln(), m.center[0] as f64, m.center[1] as f64];
            if with_fov {
                x.push(m.fov as f64 / 60.0);
            }
            x
        };
        let set = |m: &mut Model3d, x: &[f64]| {
            m.yaw = x[0] as f32;
            m.pitch = x[1] as f32;
            m.roll = x[2] as f32;
            m.distance = (x[3].exp() as f32).clamp(0.6, 200.0);
            m.center = [x[4] as f32, x[5] as f32];
            if with_fov {
                m.fov = ((x[6] * 60.0) as f32).clamp(5.0, 150.0);
            }
        };
        let residuals = |m: &Model3d| -> Vec<f64> {
            let mut r = Vec::with_capacity(m.pins.len() * 2);
            for pin in &m.pins {
                match m.project_point(pin.model, aspect) {
                    Some((xy, _)) => {
                        r.push(((xy[0] - pin.at[0]) * aspect) as f64);
                        r.push((xy[1] - pin.at[1]) as f64);
                    }
                    None => {
                        r.push(10.0);
                        r.push(10.0);
                    }
                }
            }
            r
        };
        let cost = |r: &[f64]| r.iter().map(|v| v * v).sum::<f64>();
        let mut x = get(self);
        let mut trial = self.clone();
        let mut r = residuals(self);
        let mut c = cost(&r);
        let mut lambda = 1e-3;
        let n = x.len();
        for _ in 0..200 {
            // Numeric Jacobian.
            let mut jac = vec![vec![0.0f64; n]; r.len()];
            for k in 0..n {
                let h = 1e-5 * x[k].abs().max(1e-2);
                let mut xp = x.clone();
                xp[k] += h;
                set(&mut trial, &xp);
                let rp = residuals(&trial);
                for (i, row) in jac.iter_mut().enumerate() {
                    row[k] = (rp[i] - r[i]) / h;
                }
            }
            // (JᵀJ + λ diag) δ = −Jᵀr
            let mut a = vec![vec![0.0f64; n]; n];
            let mut g = vec![0.0f64; n];
            for (i, row) in jac.iter().enumerate() {
                for p in 0..n {
                    g[p] -= row[p] * r[i];
                    for q in 0..n {
                        a[p][q] += row[p] * row[q];
                    }
                }
            }
            let mut improved = false;
            for _ in 0..10 {
                let mut m = a.clone();
                for (p, row) in m.iter_mut().enumerate() {
                    row[p] += lambda * (a[p][p] + 1e-9);
                }
                let Some(delta) = solve_linear(m, g.clone()) else { break };
                let xn: Vec<f64> = x.iter().zip(&delta).map(|(a, d)| a + d).collect();
                set(&mut trial, &xn);
                let rn = residuals(&trial);
                let cn = cost(&rn);
                if cn < c {
                    x = xn;
                    r = rn;
                    let done = (c - cn) < 1e-14;
                    c = cn;
                    lambda = (lambda * 0.3).max(1e-9);
                    improved = true;
                    if done {
                        break;
                    }
                    break;
                }
                lambda *= 10.0;
            }
            if !improved || c < 1e-12 {
                break;
            }
        }
        set(self, &x);
        Some((c / self.pins.len() as f64).sqrt() as f32)
    }

    fn project(&self, aspect: f32, split: bool) -> Vec<[f32; 4]> {
        let mesh = obj_mesh(&self.path);
        let Ok(mesh) = mesh.as_ref() else { return Vec::new() };
        let project = |p: [f32; 3]| self.project_point(p, aspect);
        // Big triangles are split (in 3D, so the split follows perspective) until each covers a small part of the
        // output: the texture is interpolated flat across a triangle, which bends on large perspective faces.
        let mut work: Vec<[([f32; 3], [f32; 2]); 3]> = Vec::with_capacity(mesh.tris.len());
        let mut stack: Vec<([([f32; 3], [f32; 2]); 3], u8)> = mesh.tris.iter().map(|t| (*t, 0u8)).collect();
        while let Some((tri, depth)) = stack.pop() {
            let span = {
                let pts: Vec<[f32; 2]> = tri.iter().filter_map(|(p, _)| project(*p).map(|(xy, _)| xy)).collect();
                if pts.len() < 3 {
                    0.0
                } else {
                    let d = |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]) * aspect).hypot(a[1] - b[1]);
                    d(pts[0], pts[1]).max(d(pts[1], pts[2])).max(d(pts[2], pts[0]))
                }
            };
            if !split || span < 0.08 || depth >= 5 || work.len() + stack.len() > 200_000 {
                work.push(tri);
                continue;
            }
            let mid = |a: ([f32; 3], [f32; 2]), b: ([f32; 3], [f32; 2])| {
                ([(a.0[0] + b.0[0]) * 0.5, (a.0[1] + b.0[1]) * 0.5, (a.0[2] + b.0[2]) * 0.5], [(a.1[0] + b.1[0]) * 0.5, (a.1[1] + b.1[1]) * 0.5])
            };
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
            for t in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
                stack.push((t, depth + 1));
            }
        }
        let mut faces: Vec<(f32, [[f32; 4]; 3])> = Vec::with_capacity(work.len());
        for tri in &work {
            let mut out = [[0.0f32; 4]; 3];
            let mut depth = 0.0;
            let mut ok = true;
            for (k, (p, uv)) in tri.iter().enumerate() {
                match project(*p) {
                    Some((xy, d)) => {
                        out[k] = [xy[0], xy[1], uv[0], 1.0 - uv[1]];
                        depth += d;
                    }
                    None => ok = false,
                }
            }
            if !ok {
                continue;
            }
            if self.cull {
                // Output y points down: a front face (counter-clockwise in the model) turns clockwise here.
                let (a, b, c) = (out[0], out[1], out[2]);
                let cross = (b[0] - a[0]) * (c[1] - a[1]) * aspect - (b[1] - a[1]) * (c[0] - a[0]) * aspect;
                if cross > 0.0 {
                    continue;
                }
            }
            faces.push((depth, out));
        }
        faces.sort_by(|a, b| b.0.total_cmp(&a.0));
        faces.into_iter().flat_map(|(_, t)| t).collect()
    }

    /// For the hash that decides when to rebuild the triangles.
    pub fn key(&self) -> Vec<[f32; 2]> {
        vec![[self.yaw, self.pitch], [self.roll, self.distance], [self.fov, self.scale], self.center, [self.cull as u8 as f32, self.path.len() as f32]]
    }
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

/// How a slice lands on the slices drawn before it (MadMapper's surface blend modes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SliceBlend {
    /// Covers what's under it.
    #[default]
    Normal,
    /// Adds light: overlaps get brighter.
    Add,
    /// Brightens without blowing out.
    Screen,
    /// Darkens: only lets through what's under it.
    Multiply,
}

impl SliceBlend {
    pub const ALL: [SliceBlend; 4] = [SliceBlend::Normal, SliceBlend::Add, SliceBlend::Screen, SliceBlend::Multiply];

    pub fn name(self) -> &'static str {
        match self {
            SliceBlend::Normal => "Normal",
            SliceBlend::Add => "Add",
            SliceBlend::Screen => "Screen",
            SliceBlend::Multiply => "Multiply",
        }
    }

    /// The shader entry that draws a slice this way: `mesh` = a warped / line / 3D slice, else a corner-pinned one.
    pub fn entry(self, mesh: bool) -> &'static str {
        match (mesh, self) {
            (false, SliceBlend::Normal) => "fs_surface",
            (false, SliceBlend::Add) => "fs_surface_add",
            (false, SliceBlend::Screen) => "fs_surface_screen",
            (false, SliceBlend::Multiply) => "fs_surface_mul",
            (true, SliceBlend::Normal) => "fs_mesh",
            (true, SliceBlend::Add) => "fs_mesh_add",
            (true, SliceBlend::Screen) => "fs_mesh_screen",
            (true, SliceBlend::Multiply) => "fs_mesh_mul",
        }
    }
}

impl crate::model::Choice for SliceBlend {
    const OPTIONS: &'static [Self] = &SliceBlend::ALL;
    fn label(self) -> &'static str {
        self.name()
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

    /// (column, row) of point `k`.
    pub fn grid_pos(&self, k: usize) -> (u32, u32) {
        let w = self.cols as usize + 1;
        ((k % w) as u32, (k / w) as u32)
    }

    /// One more column of points, through the middle of the cells right of column `i` (on the current curve, so
    /// the shape doesn't change). None at the 16-column limit or past the last column.
    pub fn with_column_after(&self, i: u32) -> Option<Self> {
        if self.cols >= 16 || i >= self.cols {
            return None;
        }
        let mut points = Vec::with_capacity(((self.cols + 2) * (self.rows + 1)) as usize);
        for j in 0..=self.rows {
            for c in 0..=self.cols {
                points.push(self.point(c, j));
                if c == i {
                    points.push(self.eval((i as f32 + 0.5) / self.cols as f32, j as f32 / self.rows as f32));
                }
            }
        }
        Some(Self { cols: self.cols + 1, rows: self.rows, points })
    }

    /// One more row of points, through the middle of the cells below row `j`.
    pub fn with_row_after(&self, j: u32) -> Option<Self> {
        if self.rows >= 16 || j >= self.rows {
            return None;
        }
        let mut points = Vec::with_capacity(((self.cols + 1) * (self.rows + 2)) as usize);
        for r in 0..=self.rows {
            for c in 0..=self.cols {
                points.push(self.point(c, r));
            }
            if r == j {
                for c in 0..=self.cols {
                    points.push(self.eval(c as f32 / self.cols as f32, (j as f32 + 0.5) / self.rows as f32));
                }
            }
        }
        Some(Self { cols: self.cols, rows: self.rows + 1, points })
    }

    /// Without column `i` (an inner one: the outer edges stay).
    pub fn without_column(&self, i: u32) -> Option<Self> {
        if i == 0 || i >= self.cols {
            return None;
        }
        let points = self.points.iter().enumerate().filter(|(k, _)| self.grid_pos(*k).0 != i).map(|(_, p)| *p).collect();
        Some(Self { cols: self.cols - 1, rows: self.rows, points })
    }

    /// Without row `j` (an inner one).
    pub fn without_row(&self, j: u32) -> Option<Self> {
        if j == 0 || j >= self.rows {
            return None;
        }
        let points = self.points.iter().enumerate().filter(|(k, _)| self.grid_pos(*k).1 != j).map(|(_, p)| *p).collect();
        Some(Self { cols: self.cols, rows: self.rows - 1, points })
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
    /// Per point: a sharp corner in a smooth outline (the curve heads straight at its neighbours). Missing = smooth.
    pub corners: Vec<bool>,
    /// Attached to this slice: the points are in the slice's own square (0..1) and follow it when it moves or warps.
    #[serde(default)]
    pub attached: Option<u64>,
    /// Per point: a bezier handle, the offset from the point to the curve's pull after it (the pull before it is the
    /// mirror image). Missing = automatic (straight sides, or smooth with Smooth on).
    #[serde(default)]
    pub handles: Vec<Option<[f32; 2]>>,
}

impl Default for Mask {
    fn default() -> Self {
        Self { id: 0, name: "Mask".into(), enabled: true, points: vec![[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]], invert: false, feather: 0.0, smooth: false, corners: Vec::new(), attached: None, handles: Vec::new() }
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
    pub fn is_corner(&self, i: usize) -> bool {
        self.corners.get(i).copied().unwrap_or(false)
    }

    pub fn set_corner(&mut self, i: usize, on: bool) {
        if self.corners.len() <= i {
            self.corners.resize(i + 1, false);
        }
        self.corners[i] = on;
    }

    pub fn handle(&self, i: usize) -> Option<[f32; 2]> {
        self.handles.get(i).copied().flatten()
    }

    pub fn set_handle(&mut self, i: usize, h: Option<[f32; 2]>) {
        if self.handles.len() <= i {
            self.handles.resize(i + 1, None);
        }
        self.handles[i] = h;
    }

    /// The handle point `i` would get if you turned its handles on: the smooth curve's direction through it.
    pub fn auto_handle(&self, i: usize) -> [f32; 2] {
        let n = self.points.len();
        let (a, b) = (self.points[(i + n - 1) % n], self.points[(i + 1) % n]);
        [(b[0] - a[0]) / 6.0, (b[1] - a[1]) / 6.0]
    }

    /// Adds a point before index `i` (corner flags and handles stay with their points).
    pub fn insert_point(&mut self, i: usize, p: [f32; 2]) {
        if self.corners.len() >= i {
            self.corners.insert(i, false);
        }
        if self.handles.len() >= i {
            self.handles.insert(i, None);
        }
        self.points.insert(i, p);
    }

    pub fn remove_point(&mut self, i: usize) {
        if i < self.corners.len() {
            self.corners.remove(i);
        }
        if i < self.handles.len() {
            self.handles.remove(i);
        }
        self.points.remove(i);
    }

    /// This mask with its points (and handles) where they land on the output (attached masks follow their slice).
    pub fn placed(&self, surfaces: &[Surface]) -> Mask {
        if self.attached.is_none() {
            return self.clone();
        }
        let points: Vec<[f32; 2]> = self.points.iter().map(|p| mask_to_output(surfaces, self, *p)).collect();
        let handles = (0..self.points.len())
            .map(|i| {
                self.handle(i).map(|h| {
                    let tip = mask_to_output(surfaces, self, [self.points[i][0] + h[0], self.points[i][1] + h[1]]);
                    [tip[0] - points[i][0], tip[1] - points[i][1]]
                })
            })
            .collect();
        Mask { points, handles, attached: None, ..self.clone() }
    }

    /// The outline as drawn: the points, or a curve through them (12 steps per side) when Smooth is on or a point
    /// has bezier handles.
    pub fn outline(&self) -> Vec<[f32; 2]> {
        let n = self.points.len();
        if n < 3 {
            return self.points.clone();
        }
        if self.handles.iter().any(Option::is_some) {
            return self.bezier_outline();
        }
        if !self.smooth {
            return self.points.clone();
        }
        const STEPS: usize = 12;
        let p = |i: usize| self.points[i % n];
        // A corner's phantom neighbour is its real neighbour mirrored through it, so the curve leaves it heading
        // straight at the next point (two corners in a row = a straight side).
        let mirror = |a: [f32; 2], b: [f32; 2]| [2.0 * a[0] - b[0], 2.0 * a[1] - b[1]];
        let mut out = Vec::with_capacity(n * STEPS);
        for i in 0..n {
            let (mut p0, p1, p2, mut p3) = (p(i + n - 1), p(i), p(i + 1), p(i + 2));
            if self.is_corner(i) {
                p0 = mirror(p1, p2);
            }
            if self.is_corner((i + 1) % n) {
                p3 = mirror(p2, p1);
            }
            for k in 0..STEPS {
                let t = k as f32 / STEPS as f32;
                out.push([catmull(p0[0], p1[0], p2[0], p3[0], t), catmull(p0[1], p1[1], p2[1], p3[1], t)]);
            }
        }
        out
    }

    /// The outline as cubic bezier sides: a point with a handle pulls by it; others pull like the smooth curve
    /// (Smooth on, not a corner) or straight at their neighbours.
    fn bezier_outline(&self) -> Vec<[f32; 2]> {
        const STEPS: usize = 12;
        let n = self.points.len();
        let p = |i: usize| self.points[i % n];
        let third = |a: [f32; 2], b: [f32; 2]| [(b[0] - a[0]) / 3.0, (b[1] - a[1]) / 3.0];
        // The pull after point i (towards i + 1) and before it (towards i - 1).
        let after = |i: usize| match self.handle(i) {
            Some(h) => h,
            None if self.smooth && !self.is_corner(i) => self.auto_handle(i),
            None => third(p(i), p(i + 1)),
        };
        let before = |i: usize| match self.handle(i) {
            Some(h) => [-h[0], -h[1]],
            None if self.smooth && !self.is_corner(i) => {
                let h = self.auto_handle(i);
                [-h[0], -h[1]]
            }
            None => third(p(i), p(i + n - 1)),
        };
        let mut out = Vec::with_capacity(n * STEPS);
        for i in 0..n {
            let (a, d) = (p(i), p(i + 1));
            let (h1, h2) = (after(i), before((i + 1) % n));
            let (b, c) = ([a[0] + h1[0], a[1] + h1[1]], [d[0] + h2[0], d[1] + h2[1]]);
            for k in 0..STEPS {
                let t = k as f32 / STEPS as f32;
                let u = 1.0 - t;
                let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                out.push([w[0] * a[0] + w[1] * b[0] + w[2] * c[0] + w[3] * d[0], w[0] * a[1] + w[1] * b[1] + w[2] * c[1] + w[3] * d[1]]);
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

#[cfg(test)]
mod mask_corner_tests {
    use super::*;

    #[test]
    fn a_handle_set_to_the_automatic_one_changes_nothing() {
        let mut m = Mask { smooth: true, ..Default::default() };
        m.points[1] = [0.65, 0.35];
        let before = m.outline();
        let h = m.auto_handle(1);
        m.set_handle(1, Some(h));
        let after = m.outline();
        assert_eq!(before.len(), after.len());
        for (a, b) in before.iter().zip(&after) {
            assert!((a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn a_handle_bends_a_straight_side() {
        let mut m = Mask::default();
        assert_eq!(m.outline().len(), 4, "plain polygon");
        m.set_handle(0, Some([0.0, -0.1]));
        let o = m.outline();
        assert_eq!(o.len(), 48);
        // Side 0 -> 1 runs along y = 0.4; the handle pulls it upwards (smaller y), side 2 -> 3 stays straight.
        assert!(o[1..12].iter().any(|p| p[1] < 0.39), "{:?}", &o[..12]);
        assert!(o[24..36].iter().all(|p| (p[1] - 0.6).abs() < 1e-5));
        // Handles stay with their points.
        m.insert_point(0, [0.5, 0.3]);
        assert_eq!(m.handle(1), Some([0.0, -0.1]));
        m.remove_point(0);
        assert_eq!(m.handle(0), Some([0.0, -0.1]));
        // Not attached: placing it changes nothing.
        assert_eq!(m.placed(&[]).handles, m.handles);
    }

    #[test]
    fn two_corners_in_a_row_make_a_straight_side() {
        let mut m = Mask { smooth: true, ..Default::default() };
        let curved = m.outline();
        m.set_corner(0, true);
        m.set_corner(1, true);
        let o = m.outline();
        // Side 0 -> 1 runs along y = 0.4: every sample of it stays on that line once both ends are corners.
        assert!(o[..12].iter().all(|p| (p[1] - 0.4).abs() < 1e-5), "{:?}", &o[..12]);
        assert!(curved[..12].iter().any(|p| (p[1] - 0.4).abs() > 1e-3));
        // Corner flags follow their points.
        m.insert_point(1, [0.5, 0.3]);
        assert!(m.is_corner(0) && !m.is_corner(1) && m.is_corner(2));
        m.remove_point(0);
        assert!(!m.is_corner(0) && m.is_corner(1));
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
            model: None,
            edges: None,
            locked: false,
            blend: SliceBlend::Normal,
            group: None,
        }
    }
}

impl Surface {
    /// Takes `other`'s look (opacity, soft edges, edge curve, colour, blend, shape), not its place or source.
    pub fn copy_look_from(&mut self, other: &Surface) {
        self.opacity = other.opacity;
        self.feather = other.feather;
        self.edges = other.edges;
        self.blend_gamma = other.blend_gamma;
        self.color = other.color.clone();
        self.blend = other.blend;
        self.shape = other.shape;
    }

    /// For the slice shader (`u.p[11]` widths, `u.p[12].x` on): per-side soft edges, when set.
    pub fn edge_uniforms(&self) -> [[f32; 4]; 2] {
        match self.edges {
            Some(e) => [e, [1.0, 0.0, 0.0, 0.0]],
            None => [[0.0; 4]; 2],
        }
    }

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
        if let Some(m) = &self.model {
            return m.center;
        }
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
        if let Some(m) = &mut self.model {
            m.center[0] += dx;
            m.center[1] += dy;
        }
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
    /// Named groups of slices (they can sit inside each other); a slice says which one it's in.
    pub groups: Vec<SliceGroup>,
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

    /// A mask from a line drawn by hand (output space 0..1): simplified to its shape and drawn as a smooth outline.
    /// None for fewer than three distinct points.
    pub fn add_mask_stroke(&mut self, id: u64, stroke: &[[f32; 2]]) -> Option<&mut Mask> {
        let points = simplify_stroke(stroke, 0.004, 96);
        if points.len() < 3 {
            return None;
        }
        let n = self.masks.len() + 1;
        self.masks.push(Mask { id, name: format!("Drawn {n}"), points, smooth: true, ..Default::default() });
        self.masks.last_mut()
    }

    /// A new mask of `shape` in the middle of an output `aspect` (width / height) wide, so circles look round.
    pub fn add_mask_shape(&mut self, id: u64, shape: MaskShape, aspect: f32) -> &mut Mask {
        let n = self.masks.len() + 1;
        self.masks.push(Mask { id, name: format!("{} {n}", shape.name()), points: shape.points(aspect), ..Default::default() });
        self.masks.last_mut().unwrap()
    }

    /// The enabled masks with their points on the output (attached ones placed on their slice).
    pub fn placed_masks(&self) -> Vec<Mask> {
        self.masks
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.placed(&self.surfaces))
            .collect()
    }

    /// Removes the slices `gone` says yes to; masks attached to them stay where they are on the output.
    pub fn remove_slices(&mut self, gone: impl Fn(&Surface) -> bool) {
        let ids: Vec<u64> = self.surfaces.iter().filter(|s| gone(s)).map(|s| s.id).collect();
        let attached: Vec<u64> = self.masks.iter().filter(|m| m.attached.is_some_and(|a| ids.contains(&a))).map(|m| m.id).collect();
        for m in attached {
            self.attach_mask(m, None);
        }
        self.surfaces.retain(|s| !ids.contains(&s.id));
        self.prune_groups();
    }

    /// Attaches mask `mask` to slice `slice` (or detaches it with None), keeping it where it is on the output.
    pub fn attach_mask(&mut self, mask: u64, slice: Option<u64>) {
        let Some(i) = self.masks.iter().position(|m| m.id == mask) else { return };
        let out: Vec<[f32; 2]> = self.masks[i].points.iter().map(|p| mask_to_output(&self.surfaces, &self.masks[i], *p)).collect();
        self.masks[i].attached = slice.filter(|id| self.surfaces.iter().any(|s| s.id == *id));
        let local: Vec<[f32; 2]> = out.iter().map(|p| output_to_mask(&self.surfaces, &self.masks[i], *p)).collect();
        self.masks[i].points = local;
    }

    /// Everything the enabled masks black out, as one triangle list.
    pub fn mask_triangles(&self) -> Vec<[f32; 4]> {
        self.placed_masks().iter().flat_map(Mask::triangles).collect()
    }
}

/// A hand-drawn line with fewer points (Ramer–Douglas–Peucker): points closer than `tolerance` to the simplified
/// line go; the tolerance grows until at most `max` points are left.
pub fn simplify_stroke(stroke: &[[f32; 2]], tolerance: f32, max: usize) -> Vec<[f32; 2]> {
    fn rdp(p: &[[f32; 2]], tol: f32, out: &mut Vec<[f32; 2]>) {
        let (a, b) = (p[0], p[p.len() - 1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy).max(1e-9);
        let (mut far, mut at) = (0.0, 0);
        for (i, q) in p.iter().enumerate().take(p.len() - 1).skip(1) {
            let d = ((q[0] - a[0]) * dy - (q[1] - a[1]) * dx).abs() / len;
            if d > far {
                (far, at) = (d, i);
            }
        }
        if far > tol {
            rdp(&p[..=at], tol, out);
            out.pop();
            rdp(&p[at..], tol, out);
        } else {
            out.push(a);
            out.push(b);
        }
    }
    let mut pts: Vec<[f32; 2]> = Vec::with_capacity(stroke.len());
    for p in stroke {
        if pts.last().is_none_or(|l: &[f32; 2]| (l[0] - p[0]).hypot(l[1] - p[1]) > 1e-4) {
            pts.push(*p);
        }
    }
    if pts.len() < 3 {
        return pts;
    }
    let mut tol = tolerance;
    loop {
        let mut out = Vec::new();
        rdp(&pts, tol, &mut out);
        // A closed outline: the last point joins the first by itself.
        if out.len() > 3 && (out[0][0] - out[out.len() - 1][0]).hypot(out[0][1] - out[out.len() - 1][1]) < tol * 4.0 {
            out.pop();
        }
        if out.len() <= max.max(3) {
            return out;
        }
        tol *= 1.5;
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

/// Where a point of the slice's own square (s, t) lands on the output: through its warp, or its corner-pin.
pub fn surface_to_output(s: &Surface, p: [f32; 2]) -> [f32; 2] {
    if let Some(m) = &s.mesh {
        return m.eval(p[0], p[1]);
    }
    let h = square_to_quad(&s.corners);
    let w = h[6] * p[0] + h[7] * p[1] + h[8];
    let w = if w.abs() < 1e-9 { 1e-9 } else { w };
    [(h[0] * p[0] + h[1] * p[1] + h[2]) / w, (h[3] * p[0] + h[4] * p[1] + h[5]) / w]
}

/// A mask point on the output (attached masks follow their slice; a missing slice leaves points as they are).
pub fn mask_to_output(surfaces: &[Surface], m: &Mask, p: [f32; 2]) -> [f32; 2] {
    match m.attached.and_then(|id| surfaces.iter().find(|s| s.id == id)) {
        Some(s) => surface_to_output(s, p),
        None => p,
    }
}

/// The other way: an output point into the mask's own space (a warped slice is read through its outer corners).
pub fn output_to_mask(surfaces: &[Surface], m: &Mask, p: [f32; 2]) -> [f32; 2] {
    match m.attached.and_then(|id| surfaces.iter().find(|s| s.id == id)) {
        Some(s) => {
            let corners = s.mesh.as_ref().map_or(s.corners, |mesh| mesh.corners());
            output_to_surface(&corners, p).unwrap_or(p)
        }
        None => p,
    }
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

    #[test]
    fn a_drawn_circle_becomes_a_small_smooth_mask() {
        let stroke: Vec<[f32; 2]> = (0..=400).map(|k| {
            let a = k as f32 / 400.0 * std::f32::consts::TAU;
            [0.5 + 0.2 * a.cos(), 0.5 + 0.2 * a.sin()]
        }).collect();
        let mut m = Mapping::default();
        let mask = m.add_mask_stroke(9, &stroke).unwrap();
        assert!(mask.smooth && (8..=96).contains(&mask.points.len()), "{} points", mask.points.len());
        assert!(m.add_mask_stroke(10, &[[0.1, 0.1], [0.1, 0.1]]).is_none(), "a click is not a mask");
        assert!(simplify_stroke(&stroke, 0.0001, 12).len() <= 12);
    }

    #[test]
    fn an_attached_mask_follows_its_slice() {
        let mut m = Mapping::default();
        m.add_surface(1);
        m.surfaces[0].corners = [[0.2, 0.2], [0.6, 0.2], [0.6, 0.6], [0.2, 0.6]];
        m.add_mask(2);
        m.masks[0].points = vec![[0.3, 0.3], [0.5, 0.3], [0.4, 0.5]];
        m.attach_mask(2, Some(1));
        assert!(m.masks[0].attached.is_some());
        let near = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4;
        assert!(near(m.masks[0].points[0], [0.25, 0.25]), "stored in the slice's square: {:?}", m.masks[0].points[0]);
        assert!(near(m.placed_masks()[0].points[0], [0.3, 0.3]), "and still where it was on the output");
        m.surfaces[0].translate(0.1, 0.0);
        assert!(near(m.placed_masks()[0].points[0], [0.4, 0.3]), "moves with the slice");
        m.attach_mask(2, None);
        assert!(m.masks[0].attached.is_none() && near(m.masks[0].points[0], [0.4, 0.3]), "detached where it is");
        m.attach_mask(2, Some(1));
        m.remove_slices(|s| s.id == 1);
        assert!(m.masks[0].attached.is_none() && near(m.masks[0].points[0], [0.4, 0.3]), "its slice deleted: stays put");
    }

    #[test]
    fn copying_a_look_keeps_place_and_source() {
        let mut a = Surface::new(1, 0);
        a.opacity = 0.4;
        a.edges = Some([0.1, 0.0, 0.0, 0.2]);
        a.blend = SliceBlend::Add;
        let mut b = Surface::new(2, 1);
        b.layer = Some(7);
        let corners = b.corners;
        b.copy_look_from(&a);
        assert_eq!((b.opacity, b.edges, b.blend), (0.4, a.edges, SliceBlend::Add));
        assert_eq!((b.corners, b.layer, b.id), (corners, Some(7), 2));
    }

    #[test]
    fn mesh_lines_come_and_go_without_moving_the_shape() {
        let corners = [[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]];
        let m = Mesh::from_corners(&corners, 2, 2);
        let wider = m.with_column_after(0).unwrap();
        assert_eq!((wider.cols, wider.points.len()), (3, 12));
        assert_eq!(wider.corners(), m.corners(), "outline kept");
        assert_eq!(wider.point(1, 0), m.eval(0.25, 0.0), "the new point sits where the picture already was");
        let taller = m.with_row_after(1).unwrap();
        assert_eq!((taller.rows, taller.points.len()), (3, 12));
        assert_eq!(wider.without_column(1).unwrap(), m, "removing the added column gives the original back");
        assert!(m.without_column(0).is_none() && m.without_row(2).is_none(), "outer edges stay");
        assert_eq!(m.grid_pos(4), (1, 1));
    }

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

#[cfg(test)]
mod model_tests {
    use super::*;

    const CUBE: &str = "v -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nv -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\n\
vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n\
f 1/1 2/2 3/3 4/4\nf 6/1 5/2 8/3 7/4\nf 5/1 1/2 4/3 8/4\nf 2/1 6/2 7/3 3/4\nf 4/1 3/2 7/3 8/4\nf 5/1 6/2 2/3 1/4\n";

    #[test]
    fn a_cube_parses_and_projects_its_front_faces() {
        let mesh = parse_obj(CUBE).unwrap();
        assert_eq!(mesh.tris.len(), 12, "six quads make twelve triangles");
        // Unit cube, centred.
        assert!(mesh.tris.iter().flatten().all(|(p, _)| p.iter().all(|c| c.abs() <= 0.5 + 1e-6)));
        let path = std::env::temp_dir().join("phosphor-test-cube.obj");
        std::fs::write(&path, CUBE).unwrap();
        let p = path.to_string_lossy().to_string();
        forget_obj(&p);
        // Straight on: only the front face shows, centred on the output (split into small triangles).
        let m = Model3d { path: p.clone(), yaw: 0.0, pitch: 0.0, ..Default::default() };
        let t = m.triangles(16.0 / 9.0);
        assert!(!t.is_empty() && t.len() % 3 == 0);
        let n = t.len() as f32;
        let cx = t.iter().map(|v| v[0]).sum::<f32>() / n;
        assert!((cx - 0.5).abs() < 0.05, "{cx}");
        // Every vertex of the front face: texture spans the whole 0..1 square.
        assert!(t.iter().any(|v| v[2] < 0.01) && t.iter().any(|v| v[2] > 0.99));
        // Turned a bit: more triangles show (three faces), still textured 0..1.
        let turned = Model3d { path: p.clone(), yaw: 0.1, pitch: 0.08, ..Default::default() };
        let t3 = turned.triangles(16.0 / 9.0);
        assert!(t3.len() > t.len());
        assert!(t3.iter().all(|v| (0.0..=1.0).contains(&v[2]) && (0.0..=1.0).contains(&v[3])));
        // Without culling more is drawn (far faces first).
        assert!(Model3d { cull: false, ..turned }.triangles(16.0 / 9.0).len() > t3.len());
        assert!(Model3d { path: "/nope.obj".into(), ..Default::default() }.triangles(1.0).is_empty());
    }
}

/// Gaussian elimination with partial pivoting (small systems: the calibration solver).
fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[piv][col].abs() < 1e-15 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        for row in col + 1..n {
            let f = a[row][col] / a[col][col];
            for k in col..n {
                a[row][k] -= f * a[col][k];
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|k| a[i][k] * x[k]).sum();
        x[i] = (b[i] - s) / a[i][i];
    }
    Some(x)
}

#[cfg(test)]
mod calibration_tests {
    use super::*;

    #[test]
    fn pins_recover_the_projector() {
        let aspect = 16.0 / 9.0;
        let truth = Model3d { yaw: 0.11, pitch: 0.07, roll: 0.01, distance: 3.4, fov: 34.0, center: [0.47, 0.53], ..Default::default() };
        let corners: Vec<[f32; 3]> = vec![[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5]];
        let pins: Vec<Pin> = corners.iter().map(|&p| Pin { model: p, at: truth.project_point(p, aspect).unwrap().0 }).collect();
        // Start from the defaults (a different view), with all seven pins: the lens is solved too.
        let mut m = Model3d { pins: pins.clone(), ..Default::default() };
        let err = m.solve(aspect).unwrap();
        assert!(err < 1e-4, "rms {err}");
        for pin in &pins {
            let (xy, _) = m.project_point(pin.model, aspect).unwrap();
            assert!((xy[0] - pin.at[0]).abs() < 1e-3 && (xy[1] - pin.at[1]).abs() < 1e-3);
        }
        assert!((m.fov - truth.fov).abs() < 1.0, "fov {}", m.fov);
        // Fewer than four pins: nothing to solve.
        assert!(Model3d { pins: pins[..3].to_vec(), ..Default::default() }.solve(aspect).is_none());
    }
}
