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
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self { id: 0, name: "Output".into(), enabled: true, width: 0, height: 0, mapping: Mapping::default(), syphon: false, color: ColorGrade::default(), screen: String::new() }
    }
}

impl OutputConfig {
    pub fn size(&self, canvas: (u32, u32)) -> (u32, u32) {
        if self.width == 0 || self.height == 0 { canvas } else { (self.width, self.height) }
    }
}
