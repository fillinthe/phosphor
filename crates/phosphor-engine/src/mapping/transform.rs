//! Moving, scaling and turning shapes on the mapping canvas: the transform box's operations (`CanvasShape`, slices
//! and masks) and the animatable transform a mask carries on top of its own points (`ShapeTransform`): each value can
//! be mapped (MIDI / OSC / keys), modulated, put on the timeline or in a cue (`ControlTarget::MaskTransform`).

use serde::{Deserialize, Serialize};

use crate::{ModInputs, Modulation, control::modulate};

/// Anything on the mapping canvas that moves, scales and turns as a whole (slices, masks): the transform box.
pub trait CanvasShape {
    /// Every point of the shape through `f`.
    fn map_points(&mut self, f: impl Fn([f32; 2]) -> [f32; 2]);
    /// The points the box is drawn round.
    fn shape_points(&self) -> Vec<[f32; 2]>;

    /// The box round the shape's points: (top-left, bottom-right).
    fn bounds(&self) -> ([f32; 2], [f32; 2]) {
        self.shape_points().iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]))
    }

    /// Scaled from `anchor` (a point that stays put): `sx` across, `sy` down.
    fn scale_about(&mut self, anchor: [f32; 2], sx: f32, sy: f32) {
        self.map_points(|p| [anchor[0] + (p[0] - anchor[0]) * sx, anchor[1] + (p[1] - anchor[1]) * sy]);
    }

    /// Turned round `centre` by `radians` (clockwise on screen), as it looks on an output `aspect` wide per high.
    fn rotate_about(&mut self, centre: [f32; 2], radians: f32, aspect: f32) {
        let (c, s) = (radians.cos(), radians.sin());
        self.map_points(|p| {
            let (x, y) = ((p[0] - centre[0]) * aspect, p[1] - centre[1]);
            [centre[0] + (c * x - s * y) / aspect, centre[1] + s * x + c * y]
        });
    }
}

/// Position, size and turn on top of a shape's own points. Identity by default (old projects open unchanged).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeTransform {
    /// Moved across / down (output widths / heights, -1..1).
    pub x: f32,
    pub y: f32,
    /// Stretched across / down round the shape's centre (1 = as drawn).
    pub scale_x: f32,
    pub scale_y: f32,
    /// Turned round the shape's centre (degrees, clockwise).
    pub rotation: f32,
    pub x_mod: Option<Modulation>,
    pub y_mod: Option<Modulation>,
    pub scale_x_mod: Option<Modulation>,
    pub scale_y_mod: Option<Modulation>,
    pub rotation_mod: Option<Modulation>,
}

impl Default for ShapeTransform {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, scale_x: 1.0, scale_y: 1.0, rotation: 0.0, x_mod: None, y_mod: None, scale_x_mod: None, scale_y_mod: None, rotation_mod: None }
    }
}

/// One of a `ShapeTransform`'s values: its name, range and whether the range is logarithmic (scale).
pub struct TransformPart {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub log: bool,
    pub default: f32,
}

impl TransformPart {
    /// The default as 0..1 across the range.
    pub fn default_unit(&self) -> f32 {
        if self.log { (self.default / self.min).ln() / (self.max / self.min).ln() } else { (self.default - self.min) / (self.max - self.min) }
    }
}

impl ShapeTransform {
    /// Position X, Position Y, Scale X, Scale Y, Rotation (`part` indexes these everywhere: controls, OSC, UI).
    pub const PARTS: [TransformPart; 5] = [
        TransformPart { name: "Position X", min: -1.0, max: 1.0, log: false, default: 0.0 },
        TransformPart { name: "Position Y", min: -1.0, max: 1.0, log: false, default: 0.0 },
        TransformPart { name: "Scale X", min: 0.05, max: 4.0, log: true, default: 1.0 },
        TransformPart { name: "Scale Y", min: 0.05, max: 4.0, log: true, default: 1.0 },
        TransformPart { name: "Rotation", min: -180.0, max: 180.0, log: false, default: 0.0 },
    ];

    /// Value `part` and its modulation.
    pub fn part_mut(&mut self, part: usize) -> (&mut f32, &mut Option<Modulation>) {
        match part {
            0 => (&mut self.x, &mut self.x_mod),
            1 => (&mut self.y, &mut self.y_mod),
            2 => (&mut self.scale_x, &mut self.scale_x_mod),
            3 => (&mut self.scale_y, &mut self.scale_y_mod),
            _ => (&mut self.rotation, &mut self.rotation_mod),
        }
    }

    fn part(&self, part: usize) -> (f32, Option<&Modulation>) {
        match part {
            0 => (self.x, self.x_mod.as_ref()),
            1 => (self.y, self.y_mod.as_ref()),
            2 => (self.scale_x, self.scale_x_mod.as_ref()),
            3 => (self.scale_y, self.scale_y_mod.as_ref()),
            _ => (self.rotation, self.rotation_mod.as_ref()),
        }
    }

    /// Value `part` as 0..1 across its range (a controller's value).
    pub fn unit(&self, part: usize) -> f32 {
        let p = &Self::PARTS[part.min(4)];
        let v = self.part(part).0.clamp(p.min, p.max);
        if p.log { (v / p.min).ln() / (p.max / p.min).ln() } else { (v - p.min) / (p.max - p.min) }
    }

    /// Sets value `part` from 0..1 across its range.
    pub fn set_unit(&mut self, part: usize, u: f32) {
        let p = &Self::PARTS[part.min(4)];
        let u = u.clamp(0.0, 1.0);
        *self.part_mut(part).0 = if p.log { p.min * (p.max / p.min).powf(u) } else { p.min + u * (p.max - p.min) };
    }

    /// The five values now, modulation included.
    pub fn values(&self, mods: &ModInputs) -> [f32; 5] {
        std::array::from_fn(|i| {
            let (v, m) = self.part(i);
            modulate(v, Self::PARTS[i].min, Self::PARTS[i].max, m, mods)
        })
    }

    /// Something moves it over time (a modulation on any value).
    pub fn is_animated(&self) -> bool {
        [&self.x_mod, &self.y_mod, &self.scale_x_mod, &self.scale_y_mod, &self.rotation_mod].iter().any(|m| m.is_some())
    }

    /// Does nothing (and nothing animates it).
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// `shape` moved, stretched and turned by this transform at `mods`, round its own centre (`aspect` = output width
    /// per height, so turning doesn't squash it).
    pub fn apply<S: CanvasShape>(&self, shape: &mut S, mods: &ModInputs, aspect: f32) {
        if self.is_identity() {
            return;
        }
        let [x, y, sx, sy, r] = self.values(mods);
        let (lo, hi) = shape.bounds();
        let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        shape.scale_about(centre, sx, sy);
        if r != 0.0 {
            shape.rotate_about(centre, r.to_radians(), aspect);
        }
        if x != 0.0 || y != 0.0 {
            shape.map_points(|p| [p[0] + x, p[1] + y]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::Mask;

    fn square() -> Mask {
        Mask { points: vec![[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]], ..Default::default() }
    }

    #[test]
    fn a_mask_moves_and_scales_round_its_centre() {
        let mut t = ShapeTransform { x: 0.1, scale_x: 2.0, ..Default::default() };
        let mut m = square();
        t.apply(&mut m, &ModInputs::default(), 1.0);
        let (lo, hi) = m.bounds();
        assert!((lo[0] - 0.4).abs() < 1e-5 && (hi[0] - 0.8).abs() < 1e-5 && (lo[1] - 0.4).abs() < 1e-5, "{lo:?} {hi:?}");
        t = ShapeTransform::default();
        let mut still = square();
        t.apply(&mut still, &ModInputs::default(), 1.0);
        assert_eq!(still.points, square().points, "identity leaves it alone");
    }

    #[test]
    fn controller_values_round_trip() {
        let mut t = ShapeTransform::default();
        for part in 0..5 {
            assert!((t.unit(part) - ShapeTransform::PARTS[part].default_unit()).abs() < 1e-4, "part {part}");
            t.set_unit(part, 0.25);
            assert!((t.unit(part) - 0.25).abs() < 1e-4, "part {part}");
        }
        t.set_unit(4, 1.0);
        assert_eq!(t.rotation, 180.0);
        // An old project's mask has no transform: it reads as none.
        let m: Mask = serde_json::from_str(r#"{"id":1,"points":[[0,0],[1,0],[1,1]]}"#).unwrap();
        assert!(m.transform.is_identity());
    }

    #[test]
    fn an_animated_mask_follows_its_modulation() {
        let mut t = ShapeTransform::default();
        t.x_mod = Some(Modulation { source: crate::ModSource::LfoSaw, amount: 0.5, rate: 1.0, ..Default::default() });
        assert!(!t.is_identity());
        let at = |time: f32| t.values(&ModInputs { time, ..Default::default() })[0];
        assert!((at(0.1) - at(0.6)).abs() > 0.1, "the LFO moves it: {} vs {}", at(0.1), at(0.6));
    }
}
