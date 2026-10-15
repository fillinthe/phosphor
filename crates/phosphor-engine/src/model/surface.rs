//! Your own Control Surface: widgets and their layout.

use serde::{Deserialize, Serialize};

use super::*;

/// The kind of a widget on a Control Surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WidgetKind {
    /// On while held.
    #[default]
    Button,
    /// Click on, click off.
    Toggle,
    /// Vertical slider.
    Fader,
    /// Horizontal slider.
    Slider,
    /// Two values at once (X and Y).
    XyPad,
    /// Just text.
    Label,
}

impl WidgetKind {
    pub const ALL: [WidgetKind; 6] = [WidgetKind::Button, WidgetKind::Toggle, WidgetKind::Fader, WidgetKind::Slider, WidgetKind::XyPad, WidgetKind::Label];

    pub fn name(self) -> &'static str {
        match self {
            WidgetKind::Button => "Button",
            WidgetKind::Toggle => "Toggle",
            WidgetKind::Fader => "Fader",
            WidgetKind::Slider => "Slider",
            WidgetKind::XyPad => "XY Pad",
            WidgetKind::Label => "Label",
        }
    }

    /// Size in grid cells when added.
    pub fn default_size(self) -> (f32, f32) {
        match self {
            WidgetKind::Button | WidgetKind::Toggle => (2.0, 2.0),
            WidgetKind::Fader => (1.0, 4.0),
            WidgetKind::Slider => (4.0, 1.0),
            WidgetKind::XyPad => (4.0, 4.0),
            WidgetKind::Label => (3.0, 1.0),
        }
    }
}

/// A button, slider or pad you placed on a Control Surface. Its value drives every binding; it can
/// itself be mapped to a controller (`ControlTarget::Surface`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfaceWidget {
    pub id: u64,
    pub kind: WidgetKind,
    /// Position and size in grid cells.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: String,
    pub color: [u8; 3],
    pub bindings: Vec<Binding>,
    /// XY pad: what the vertical axis drives.
    pub bindings_y: Vec<Binding>,
    pub value: f32,
    pub value_y: f32,
}

impl Default for SurfaceWidget {
    fn default() -> Self {
        Self { id: 0, kind: WidgetKind::Button, x: 0.0, y: 0.0, w: 2.0, h: 2.0, label: String::new(), color: [0, 200, 190], bindings: Vec::new(), bindings_y: Vec::new(), value: 0.0, value_y: 0.0 }
    }
}

/// Your own page of controls (VDMX-style), saved with the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlSurface {
    pub widgets: Vec<SurfaceWidget>,
    /// Grid columns across the panel.
    pub columns: u32,
    pub next_id: u64,
}

impl Default for ControlSurface {
    fn default() -> Self {
        Self { widgets: Vec::new(), columns: 12, next_id: 1 }
    }
}

impl ControlSurface {
    pub fn add(&mut self, kind: WidgetKind, x: f32, y: f32) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let (w, h) = kind.default_size();
        let label = format!("{} {id}", kind.name());
        self.widgets.push(SurfaceWidget { id, kind, x, y, w, h, label, ..Default::default() });
        id
    }

    pub fn widget(&self, id: u64) -> Option<&SurfaceWidget> {
        self.widgets.iter().find(|w| w.id == id)
    }

    pub fn widget_mut(&mut self, id: u64) -> Option<&mut SurfaceWidget> {
        self.widgets.iter_mut().find(|w| w.id == id)
    }

    /// First free spot for a new widget (rows top to bottom).
    pub fn free_spot(&self, w: f32, h: f32) -> (f32, f32) {
        let cols = self.columns.max(1) as f32;
        for row in 0..200 {
            for col in 0..(cols - w + 1.0).max(1.0) as i32 {
                let (x, y) = (col as f32, row as f32);
                let clear = self.widgets.iter().all(|o| x + w <= o.x || o.x + o.w <= x || y + h <= o.y || o.y + o.h <= y);
                if clear {
                    return (x, y);
                }
            }
        }
        (0.0, 0.0)
    }
}
