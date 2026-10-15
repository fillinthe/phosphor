//! What feeds a layer (`SourceSpec`), text layers, and clip slots.

use serde::{Deserialize, Serialize};

use crate::shaders;

/// What feeds a layer. Media is referenced, never embedded: the app resolves it into
/// GPU textures via `Engine::upload_rgba` / `Engine::upload_nv12`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum SourceSpec {
    #[default]
    None,
    Generator(String),
    Image { name: String, path: Option<String> },
    Movie { name: String, path: Option<String> },
    /// A live input: camera, capture card, or a network stream. `format` is the FFmpeg input
    /// ("avfoundation" for cameras, empty to detect from `url`), `url` the device name or address.
    Live { name: String, format: String, url: String },
    /// Words drawn by the app into the layer's picture (see `TextStyle`).
    Text(TextStyle),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// A text layer: what it says and how it looks. Drawn at the composition size whenever it changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub text: String,
    /// "builtin:sans" / "builtin:mono", or a font file path with an optional "#index" (collections).
    pub font: String,
    /// Letter height as a fraction of the picture height.
    pub size: f32,
    pub color: [u8; 4],
    pub align: TextAlign,
    /// Dark outline width as a fraction of the letter height (0 = none).
    pub outline: f32,
    pub line_spacing: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self { text: "PHOSPHOR".into(), font: "builtin:sans".into(), size: 0.18, color: [255, 255, 255, 255], align: TextAlign::Center, outline: 0.0, line_spacing: 1.0 }
    }
}

impl SourceSpec {
    pub fn label(&self) -> String {
        match self {
            SourceSpec::None => "No source".into(),
            SourceSpec::Generator(id) => shaders::generator(id).map_or(id.clone(), |g| g.name.to_string()),
            SourceSpec::Image { name, .. } | SourceSpec::Movie { name, .. } | SourceSpec::Live { name, .. } => name.clone(),
            SourceSpec::Text(t) => {
                let first = t.text.lines().next().unwrap_or("").trim();
                let short: String = first.chars().take(24).collect();
                format!("Text: {}{}", short, if first.chars().count() > 24 { "…" } else { "" })
            }
        }
    }
}

/// A clip slot: a source ready to be triggered onto its layer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    pub id: u64,
    pub name: String,
    pub source: SourceSpec,
    pub generator_values: Vec<f32>,
}

impl Clip {
    pub fn is_empty(&self) -> bool {
        self.source == SourceSpec::None
    }
}
