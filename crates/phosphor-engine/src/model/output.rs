//! Outputs: projectors/screens, colour correction, NDI.

use serde::{Deserialize, Serialize};

use crate::mapping::Mapping;

/// NDI network video output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NdiSettings {
    pub enabled: bool,
    /// Shown to receivers as "COMPUTER (name)".
    pub name: String,
    pub fps: u32,
}

impl Default for NdiSettings {
    fn default() -> Self {
        Self { enabled: false, name: "Phosphor".into(), fps: 60 }
    }
}

/// Colour correction for an output or a mapping surface, so projectors, screens and materials can be matched.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorGrade {
    /// -1..1, 0 = unchanged.
    pub brightness: f32,
    /// 0..3, 1 = unchanged.
    pub contrast: f32,
    /// 0.2..3, 1 = unchanged (higher = brighter mid-tones).
    pub gamma: f32,
    /// 0..2, 1 = unchanged, 0 = black and white.
    pub saturation: f32,
    /// Red, green, blue gain 0..2 (white balance), 1 = unchanged.
    pub gains: [f32; 3],
}

impl Default for ColorGrade {
    fn default() -> Self {
        Self { brightness: 0.0, contrast: 1.0, gamma: 1.0, saturation: 1.0, gains: [1.0; 3] }
    }
}

impl ColorGrade {
    /// Shader uniform form: two vec4s where all zeros means "no change" (`grade_with()` in common.wgsl).
    pub fn uniforms(&self) -> [[f32; 4]; 2] {
        [
            [self.brightness, self.contrast - 1.0, self.gamma - 1.0, self.saturation - 1.0],
            [self.gains[0] - 1.0, self.gains[1] - 1.0, self.gains[2] - 1.0, 0.0],
        ]
    }

    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }
}

/// One physical output (projector, screen, LED wall): its own size and mapping surfaces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    /// 0 × 0 follows the canvas size.
    pub width: u32,
    pub height: u32,
    pub mapping: Mapping,
    /// Also publish this output (after mapping) as its own Syphon source.
    pub syphon: bool,
    /// Colour correction for this output.
    pub color: ColorGrade,
    /// The screen/projector this output goes to (`screens::screen_key`), "" = not chosen. Saved with the project.
    pub screen: String,
    /// Alignment grid drawn over this output (lining up projectors); off for the show.
    pub grid: GridOverlay,
    /// A photo of the stage from the projector's point of view, shown behind the editor only ("" = none).
    pub stage_photo: String,
    /// How strongly the stage photo shows over the output in the editor (0..1).
    pub stage_photo_opacity: f32,
}

/// A test grid over an output: cells across, a centre crosshair and circle, in one colour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GridOverlay {
    pub on: bool,
    /// Cells across the width (the height follows the aspect so cells stay square).
    pub cells: u32,
    pub crosshair: bool,
    pub color: [u8; 3],
    /// Line width in pixels.
    pub width: f32,
    /// Which picture: the see-through grid, or a full-frame test pattern.
    pub pattern: TestPattern,
}

/// Test pictures for lining up projectors (MadMapper's TestCard / Test Pattern / Grid Generator).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestPattern {
    /// Grid lines, crosshair and border over the show (see-through).
    #[default]
    Grid,
    /// Grid, colour bars, a grey ramp and coloured corners (red top left, green top right, blue bottom right,
    /// yellow bottom left) so a flipped or rotated projector is obvious.
    Card,
    /// Seven colour bars.
    Bars,
    /// All white: brightness and overlap checks.
    White,
    /// Black and white squares (the grid's cells).
    Checker,
    /// Black to white in ten steps: soft-edge and gamma checks.
    Ramp,
}

impl TestPattern {
    pub const ALL: [TestPattern; 6] = [TestPattern::Grid, TestPattern::Card, TestPattern::Bars, TestPattern::White, TestPattern::Checker, TestPattern::Ramp];

    pub fn name(self) -> &'static str {
        match self {
            TestPattern::Grid => "Grid",
            TestPattern::Card => "Test card",
            TestPattern::Bars => "Colour bars",
            TestPattern::White => "White",
            TestPattern::Checker => "Checkerboard",
            TestPattern::Ramp => "Grey ramp",
        }
    }
}

impl Default for GridOverlay {
    fn default() -> Self {
        Self { on: false, cells: 8, crosshair: true, color: [255, 255, 255], width: 2.0, pattern: TestPattern::Grid }
    }
}

impl GridOverlay {
    /// What the `fs_grid` pass needs: p[0] = cells, line width, crosshair, pattern; p[1] = colour.
    pub fn uniforms(&self) -> [[f32; 4]; 2] {
        [[self.cells.max(1) as f32, self.width.max(0.5), if self.crosshair { 1.0 } else { 0.0 }, self.pattern as u32 as f32], [self.color[0] as f32 / 255.0, self.color[1] as f32 / 255.0, self.color[2] as f32 / 255.0, 1.0]]
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self { id: 0, name: "Output".into(), enabled: true, width: 0, height: 0, mapping: Mapping::default(), syphon: false, color: ColorGrade::default(), screen: String::new(), grid: GridOverlay::default(), stage_photo: String::new(), stage_photo_opacity: 0.5 }
    }
}

impl OutputConfig {
    pub fn size(&self, canvas: (u32, u32)) -> (u32, u32) {
        if self.width == 0 || self.height == 0 { canvas } else { (self.width, self.height) }
    }
}
