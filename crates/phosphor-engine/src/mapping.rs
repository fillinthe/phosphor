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
    /// Region of the master frame this surface shows: x, y, width, height in 0..1.
    pub input: [f32; 4],
    pub opacity: f32,
    /// Soft-edge width as a fraction of the surface (0 = hard edge). Overlap two surfaces'
    /// feathered edges to blend projectors.
    pub feather: f32,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Surface".into(),
            enabled: true,
            corners: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            input: [0.0, 0.0, 1.0, 1.0],
            opacity: 1.0,
            feather: 0.0,
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
        let c = &self.corners;
        [(c[0][0] + c[1][0] + c[2][0] + c[3][0]) * 0.25, (c[0][1] + c[1][1] + c[2][1] + c[3][1]) * 0.25]
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        for c in &mut self.corners {
            c[0] += dx;
            c[1] += dy;
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
    /// Show an alignment grid on every surface instead of the content.
    pub test_pattern: bool,
}

impl Mapping {
    pub fn add_surface(&mut self, id: u64) -> &mut Surface {
        let surface = Surface::new(id, self.surfaces.len());
        self.surfaces.push(surface);
        self.surfaces.last_mut().unwrap()
    }

    /// Surfaces are drawn when mapping is on and there is at least one enabled.
    pub fn active(&self) -> bool {
        self.enabled && self.surfaces.iter().any(|s| s.enabled)
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
    fn degenerate_quad_is_rejected() {
        let line = [[0.0, 0.0], [1.0, 0.0], [1.0, 0.0], [0.0, 0.0]];
        assert!(quad_to_square(&line).is_none());
    }
}
