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
}

impl Default for GridOverlay {
    fn default() -> Self {
        Self { on: false, cells: 8, crosshair: true, color: [255, 255, 255], width: 2.0 }
    }
}

impl GridOverlay {
    /// What the `fs_grid` pass needs: p[0] = cells, line width, crosshair, 0; p[1] = colour.
    pub fn uniforms(&self) -> [[f32; 4]; 2] {
        [[self.cells.max(1) as f32, self.width.max(0.5), if self.crosshair { 1.0 } else { 0.0 }, 0.0], [self.color[0] as f32 / 255.0, self.color[1] as f32 / 255.0, self.color[2] as f32 / 255.0, 1.0]]
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self { id: 0, name: "Output".into(), enabled: true, width: 0, height: 0, mapping: Mapping::default(), syphon: false, color: ColorGrade::default(), screen: String::new(), grid: GridOverlay::default() }
    }
}

impl OutputConfig {
    pub fn size(&self, canvas: (u32, u32)) -> (u32, u32) {
        if self.width == 0 || self.height == 0 { canvas } else { (self.width, self.height) }
    }
}
