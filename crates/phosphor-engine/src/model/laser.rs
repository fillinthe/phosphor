//! Lasers: projectors (a laser and the controller box it's plugged into: Ether Dream, Helios, LaserCube, IDN) and
//! looks drawn in the Laser Designer (shapes: lines, circles, polygons, stars, waves, Lissajous figures, spirals,
//! beams). `crate::laser` turns a look into the points a laser draws; the app sends them while the lasers are armed.

use serde::{Deserialize, Serialize};

use crate::Modulation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaserShapeKind {
    Line,
    #[default]
    Circle,
    Polygon,
    Star,
    Wave,
    Lissajous,
    Spiral,
    /// Bright dots in a ring: beams in haze.
    Beams,
    /// Words in single-stroke letters (`LaserShape::text`, `font`).
    Text,
    /// A laser show file (.ild) playing (`LaserShape::file`).
    File,
    /// A layer's or the mix's picture, or an image file, traced into outlines (`LaserShape::source`, `image`).
    Trace,
    /// A line drawn by hand on the Designer's canvas (`LaserShape::points`), e.g. over an onion skin.
    Drawing,
}

impl LaserShapeKind {
    pub const ALL: [LaserShapeKind; 12] = [
        LaserShapeKind::Line,
        LaserShapeKind::Circle,
        LaserShapeKind::Polygon,
        LaserShapeKind::Star,
        LaserShapeKind::Wave,
        LaserShapeKind::Lissajous,
        LaserShapeKind::Spiral,
        LaserShapeKind::Beams,
        LaserShapeKind::Text,
        LaserShapeKind::File,
        LaserShapeKind::Trace,
        LaserShapeKind::Drawing,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LaserShapeKind::Line => "Line",
            LaserShapeKind::Circle => "Circle",
            LaserShapeKind::Polygon => "Polygon",
            LaserShapeKind::Star => "Star",
            LaserShapeKind::Wave => "Wave",
            LaserShapeKind::Lissajous => "Lissajous",
            LaserShapeKind::Spiral => "Spiral",
            LaserShapeKind::Beams => "Beams",
            LaserShapeKind::Text => "Text",
            LaserShapeKind::File => "Laser file",
            LaserShapeKind::Trace => "Video trace",
            LaserShapeKind::Drawing => "Drawing",
        }
    }

    /// What `count` means for this shape (None: not used), with its range.
    pub fn count_name(self) -> Option<(&'static str, f32, f32)> {
        match self {
            LaserShapeKind::Polygon => Some(("Sides", 3.0, 12.0)),
            LaserShapeKind::Star => Some(("Points", 3.0, 12.0)),
            LaserShapeKind::Wave => Some(("Waves", 0.5, 8.0)),
            LaserShapeKind::Lissajous => Some(("Across", 1.0, 7.0)),
            LaserShapeKind::Spiral => Some(("Turns", 1.0, 8.0)),
            LaserShapeKind::Beams => Some(("Beams", 1.0, 24.0)),
            LaserShapeKind::File => Some(("Frames a second", 1.0, 60.0)),
            LaserShapeKind::Trace => Some(("Outlines", 1.0, 40.0)),
            LaserShapeKind::Line | LaserShapeKind::Circle | LaserShapeKind::Text | LaserShapeKind::Drawing => None,
        }
    }

    /// What `shape` means for this shape (None: not used), 0..1.
    pub fn shape_name(self) -> Option<&'static str> {
        match self {
            LaserShapeKind::Star => Some("Inner size"),
            LaserShapeKind::Wave => Some("Height"),
            LaserShapeKind::Lissajous => Some("Up and down"),
            LaserShapeKind::Beams => Some("Ring size"),
            LaserShapeKind::Trace => Some("Level"),
            _ => None,
        }
    }
}

/// Letters for Text shapes: Hershey's single-stroke fonts (a laser draws each stroke once, no outlines).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaserFont {
    #[default]
    Simple,
    Bold,
    Serif,
    Script,
    Italic,
    Gothic,
}

impl LaserFont {
    pub const ALL: [LaserFont; 6] = [LaserFont::Simple, LaserFont::Bold, LaserFont::Serif, LaserFont::Script, LaserFont::Italic, LaserFont::Gothic];

    pub fn name(self) -> &'static str {
        match self {
            LaserFont::Simple => "Simple",
            LaserFont::Bold => "Bold",
            LaserFont::Serif => "Serif",
            LaserFont::Script => "Script",
            LaserFont::Italic => "Italic",
            LaserFont::Gothic => "Gothic",
        }
    }
}

/// One shape in a look. Positions are in the laser's scan area: -1..1 across and up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserShape {
    pub id: u64,
    pub kind: LaserShapeKind,
    pub enabled: bool,
    pub x: f32,
    pub y: f32,
    /// Half its width, as a part of the scan area (0..1).
    pub size: f32,
    /// Degrees.
    pub rotate: f32,
    /// Turns a second.
    pub spin: f32,
    pub color: [f32; 3],
    /// Colours running along the shape: 0 = one colour, 1 = a whole rainbow around it.
    pub rainbow: f32,
    /// Sides, points, waves… (`LaserShapeKind::count_name`).
    pub count: f32,
    /// A second setting for the shape (`LaserShapeKind::shape_name`), 0..1.
    pub shape: f32,
    /// Movement inside the shape (a wave scrolling, a Lissajous figure turning), cycles a second.
    pub motion: f32,
    /// How much of it is drawn, 0..1 (draw it on by moving this).
    pub draw: f32,
    pub mods: LaserMods,
    /// Text shapes: the words (a new line = `\n`) and their letters.
    pub text: String,
    pub font: LaserFont,
    /// Laser file shapes: the .ild file.
    pub file: String,
    /// Video trace shapes: the layer, group or comp traced (0 = the whole mix), and whether the lines take the
    /// picture's colours (off: the shape's colour).
    pub source: u64,
    pub picture_colors: bool,
    /// Video trace shapes: an image file traced instead of `source` (empty = trace `source`).
    pub image: String,
    /// Drawing shapes: the corners drawn (-1..1 before the shape's place, size and turn), and whether the line
    /// comes back to its start.
    pub points: Vec<[f32; 2]>,
    pub closed: bool,
}

/// Modulation (LFO, sound, beat…) of a shape's settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserMods {
    pub x: Option<Modulation>,
    pub y: Option<Modulation>,
    pub size: Option<Modulation>,
    pub rotate: Option<Modulation>,
    pub draw: Option<Modulation>,
}

impl Default for LaserShape {
    fn default() -> Self {
        Self {
            id: 0,
            kind: LaserShapeKind::Circle,
            enabled: true,
            x: 0.0,
            y: 0.0,
            size: 0.5,
            rotate: 0.0,
            spin: 0.0,
            color: [0.0, 1.0, 0.3],
            rainbow: 0.0,
            count: 5.0,
            shape: 0.45,
            motion: 0.25,
            draw: 1.0,
            mods: LaserMods::default(),
            text: "PHOSPHOR".into(),
            font: LaserFont::Simple,
            file: String::new(),
            source: 0,
            picture_colors: true,
            image: String::new(),
            points: Vec::new(),
            closed: false,
        }
    }
}

/// The settings a controller, OSC, a cue or an AI can move (`ControlTarget::LaserShape`): (name, min, max).
pub const LASER_FIELDS: [(&str, f32, f32); 9] = [
    ("x", -1.0, 1.0),
    ("y", -1.0, 1.0),
    ("size", 0.0, 1.0),
    ("rotate", -180.0, 180.0),
    ("spin", -2.0, 2.0),
    ("draw", 0.0, 1.0),
    ("rainbow", 0.0, 1.0),
    ("motion", -2.0, 2.0),
    ("shape", 0.0, 1.0),
];

impl LaserShape {
    pub fn field(&self, i: usize) -> Option<f32> {
        Some(match i {
            0 => self.x,
            1 => self.y,
            2 => self.size,
            3 => self.rotate,
            4 => self.spin,
            5 => self.draw,
            6 => self.rainbow,
            7 => self.motion,
            8 => self.shape,
            _ => return None,
        })
    }

    pub fn field_mut(&mut self, i: usize) -> Option<&mut f32> {
        Some(match i {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.size,
            3 => &mut self.rotate,
            4 => &mut self.spin,
            5 => &mut self.draw,
            6 => &mut self.rainbow,
            7 => &mut self.motion,
            8 => &mut self.shape,
            _ => return None,
        })
    }
}

/// A laser look: shapes drawn together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserLook {
    pub id: u64,
    pub name: String,
    pub shapes: Vec<LaserShape>,
    /// Onion skin: a picture shown faintly behind the Designer's canvas to draw over (never sent to a laser), fitted
    /// like a traced picture (its longer side across -1..1), and how strongly it shows.
    pub onion: String,
    pub onion_opacity: f32,
}

impl Default for LaserLook {
    fn default() -> Self {
        Self { id: 0, name: "Look".into(), shapes: Vec::new(), onion: String::new(), onion_opacity: 0.35 }
    }
}

/// A laser projector and where its picture goes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserProjector {
    pub id: u64,
    pub name: String,
    /// The controller box's id (as the app finds it: "etherdream-…", "idn-…"); empty = not chosen yet.
    pub device: String,
    /// Off: this projector stays dark even while the lasers are armed.
    pub enabled: bool,
    /// The look it draws (0 = the first one).
    pub look: u64,
    /// Points a second the laser's scanners take (30 000 is a common, safe choice; see the projector's manual).
    pub pps: u32,
    /// How bright, 0..1 (on top of the master).
    pub brightness: f32,
    /// Size of the picture in the scan area, 0..1.
    pub size: f32,
    pub x: f32,
    pub y: f32,
    pub rotate: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    /// Colour balance: how strong red, green and blue are (lasers' colours differ in power).
    pub color: [f32; 3],
    /// Audience safety: nothing is drawn below this height (-1 = the bottom of the scan area, no limit).
    pub floor: f32,
    /// Draws the test pattern instead of its look (lining the laser up). Never saved.
    #[serde(skip)]
    pub test_pattern: bool,
    /// Zones: parts of its area, each showing a look (none = its look fills the area).
    pub zones: Vec<LaserZone>,
    /// Areas the beam never lights (a balcony, a doorway), in its scan area.
    pub dark: Vec<LaserArea>,
}

/// A part of a projector's area showing its own look, placed and straightened (its corners pinned onto a surface
/// seen at an angle).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserZone {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    /// The look it shows (0 = the projector's look).
    pub look: u64,
    pub brightness: f32,
    /// The look inside the zone: position, size, turn.
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub rotate: f32,
    /// Where the zone's corners land in the scan area: bottom left, bottom right, top right, top left.
    pub corners: [[f32; 2]; 4],
}

impl Default for LaserZone {
    fn default() -> Self {
        Self { id: 0, name: "Zone 1".into(), enabled: true, look: 0, brightness: 1.0, x: 0.0, y: 0.0, size: 1.0, rotate: 0.0, corners: FULL_AREA }
    }
}

/// The whole scan area's corners (bottom left, bottom right, top right, top left).
pub const FULL_AREA: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];

/// A rectangle in a projector's scan area (-1..1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserArea {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl Default for LaserArea {
    fn default() -> Self {
        Self { left: -0.2, bottom: -0.2, right: 0.2, top: 0.2 }
    }
}

impl LaserArea {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        (self.left.min(self.right)..=self.left.max(self.right)).contains(&x) && (self.bottom.min(self.top)..=self.bottom.max(self.top)).contains(&y)
    }
}

impl Default for LaserProjector {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Laser 1".into(),
            device: String::new(),
            enabled: true,
            look: 0,
            pps: 30_000,
            brightness: 0.5,
            size: 1.0,
            x: 0.0,
            y: 0.0,
            rotate: 0.0,
            flip_x: false,
            flip_y: false,
            color: [1.0; 3],
            floor: -1.0,
            test_pattern: false,
            zones: Vec::new(),
            dark: Vec::new(),
        }
    }
}

/// Every laser in a project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lasers {
    pub projectors: Vec<LaserProjector>,
    pub looks: Vec<LaserLook>,
    /// All lasers' brightness, 0..1.
    pub master: f32,
    /// Lasers only light up while armed. Never saved: a project always opens with its lasers dark.
    #[serde(skip)]
    pub armed: bool,
}

impl Default for Lasers {
    fn default() -> Self {
        Self { projectors: Vec::new(), looks: Vec::new(), master: 1.0, armed: false }
    }
}

impl LaserLook {
    /// A look to start from: a spinning rainbow circle, a star, a Lissajous figure and a ring of beams
    /// (`new_id` hands out ids).
    pub fn starter(mut new_id: impl FnMut() -> u64) -> LaserLook {
        let shape = |id, kind, x, y, size, color: [f32; 3]| LaserShape { id, kind, x, y, size, color, ..Default::default() };
        let shapes = vec![
            LaserShape { spin: 0.2, rainbow: 1.0, ..shape(new_id(), LaserShapeKind::Circle, 0.0, 0.0, 0.8, [1.0, 0.0, 0.0]) },
            LaserShape { spin: -0.15, count: 5.0, ..shape(new_id(), LaserShapeKind::Star, 0.0, 0.0, 0.45, [0.0, 0.4, 1.0]) },
            LaserShape { count: 3.0, shape: 0.2, motion: 0.1, ..shape(new_id(), LaserShapeKind::Lissajous, 0.0, 0.0, 0.25, [0.0, 1.0, 0.3]) },
        ];
        LaserLook { id: new_id(), name: "Starter".into(), shapes, ..Default::default() }
    }
}

impl Lasers {
    pub fn look(&self, id: u64) -> Option<&LaserLook> {
        self.looks.iter().find(|l| l.id == id).or_else(|| (id == 0).then(|| self.looks.first()).flatten())
    }

    pub fn look_mut(&mut self, id: u64) -> Option<&mut LaserLook> {
        let i = self.looks.iter().position(|l| l.id == id).or_else(|| (id == 0 && !self.looks.is_empty()).then_some(0))?;
        self.looks.get_mut(i)
    }

    pub fn shape(&self, id: u64) -> Option<&LaserShape> {
        self.looks.iter().flat_map(|l| &l.shapes).find(|s| s.id == id)
    }

    pub fn shape_mut(&mut self, id: u64) -> Option<&mut LaserShape> {
        self.looks.iter_mut().flat_map(|l| &mut l.shapes).find(|s| s.id == id)
    }

    pub fn projector_mut(&mut self, id: u64) -> Option<&mut LaserProjector> {
        self.projectors.iter_mut().find(|p| p.id == id)
    }

    /// A projector's or a zone's brightness (`ControlTarget::LaserBrightness` takes either id).
    pub fn brightness(&self, id: u64) -> Option<f32> {
        self.projectors.iter().find_map(|p| if p.id == id { Some(p.brightness) } else { p.zones.iter().find(|z| z.id == id).map(|z| z.brightness) })
    }

    pub fn brightness_mut(&mut self, id: u64) -> Option<&mut f32> {
        for p in &mut self.projectors {
            if p.id == id {
                return Some(&mut p.brightness);
            }
            if let Some(z) = p.zones.iter_mut().find(|z| z.id == id) {
                return Some(&mut z.brightness);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A saved project keeps its lasers and looks but always opens with them disarmed; old projects get none.
    #[test]
    fn projects_open_with_lasers_off() {
        let mut comp = crate::Composition::default();
        let mut n = 0;
        let look = LaserLook::starter(|| {
            n += 1;
            n
        });
        let zones = vec![LaserZone { id: 60, look: look.id, corners: [[-1.0, -1.0], [0.0, -0.9], [0.0, 1.0], [-1.0, 0.8]], ..Default::default() }];
        comp.lasers.projectors.push(LaserProjector { id: 50, look: look.id, floor: -0.4, zones, dark: vec![LaserArea::default()], ..Default::default() });
        comp.lasers.looks.push(look);
        comp.lasers.armed = true;
        let back: crate::Composition = serde_json::from_str(&serde_json::to_string(&comp).unwrap()).unwrap();
        assert!(!back.lasers.armed, "never opens armed");
        assert_eq!(back.lasers.projectors, comp.lasers.projectors);
        assert_eq!(back.lasers.looks, comp.lasers.looks);
        let old: crate::Composition = serde_json::from_str("{}").unwrap();
        assert!(old.lasers.projectors.is_empty() && old.lasers.master == 1.0);
    }
}
