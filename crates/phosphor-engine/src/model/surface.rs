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
    /// A colour (hue across, saturation up, brightness strip); drives red, green and blue targets.
    Color,
}

impl WidgetKind {
    pub const ALL: [WidgetKind; 7] = [WidgetKind::Button, WidgetKind::Toggle, WidgetKind::Fader, WidgetKind::Slider, WidgetKind::XyPad, WidgetKind::Color, WidgetKind::Label];

    pub fn name(self) -> &'static str {
        match self {
            WidgetKind::Button => "Button",
            WidgetKind::Toggle => "Toggle",
            WidgetKind::Fader => "Fader",
            WidgetKind::Slider => "Slider",
            WidgetKind::XyPad => "XY Pad",
            WidgetKind::Label => "Label",
            WidgetKind::Color => "Colour",
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
            WidgetKind::Color => (4.0, 4.0),
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
    /// XY pad: what the vertical axis drives. Colour: green (and `bindings` red, `bindings_z` blue).
    pub bindings_y: Vec<Binding>,
    pub bindings_z: Vec<Binding>,
    pub value: f32,
    pub value_y: f32,
    /// Colour: brightness (hue = `value`, saturation = `value_y`).
    pub value_z: f32,
    /// Which page of the surface it's on (index into `ControlSurface::pages`).
    pub page: usize,
}

impl Default for SurfaceWidget {
    fn default() -> Self {
        Self { id: 0, kind: WidgetKind::Button, x: 0.0, y: 0.0, w: 2.0, h: 2.0, label: String::new(), color: [0, 200, 190], bindings: Vec::new(), bindings_y: Vec::new(), bindings_z: Vec::new(), value: 0.0, value_y: 0.0, value_z: 1.0, page: 0 }
    }
}

impl SurfaceWidget {
    /// A colour widget's colour, red / green / blue 0..1 (hue, saturation, brightness → RGB).
    pub fn rgb(&self) -> [f32; 3] {
        let (h, s, v) = (self.value.rem_euclid(1.0) * 6.0, self.value_y.clamp(0.0, 1.0), self.value_z.clamp(0.0, 1.0));
        let f = |n: f32| {
            let k = (n + h) % 6.0;
            v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
        };
        [f(5.0), f(3.0), f(1.0)]
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
    /// Page names (at least one); `page` is the one showing on the Mac.
    pub pages: Vec<String>,
    pub page: usize,
}

impl Default for ControlSurface {
    fn default() -> Self {
        Self { widgets: Vec::new(), columns: 12, next_id: 1, pages: vec!["Main".into()], page: 0 }
    }
}

impl ControlSurface {
    /// Adds a widget to the page showing, at the first free spot there.
    pub fn add(&mut self, kind: WidgetKind) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let (w, h) = kind.default_size();
        let (x, y) = self.free_spot(w, h);
        let label = format!("{} {id}", kind.name());
        let page = self.current_page();
        self.widgets.push(SurfaceWidget { id, kind, x, y, w, h, label, page, ..Default::default() });
        id
    }

    /// A copy of a widget at the next free spot on the page showing.
    pub fn duplicate(&mut self, id: u64) -> Option<u64> {
        let mut copy = self.widget(id)?.clone();
        (copy.x, copy.y) = self.free_spot(copy.w, copy.h);
        copy.id = self.next_id;
        copy.page = self.current_page();
        self.next_id += 1;
        self.widgets.push(copy);
        Some(self.next_id - 1)
    }

    /// The page showing (always a valid index).
    pub fn current_page(&self) -> usize {
        self.page.min(self.pages.len().saturating_sub(1))
    }

    /// Widgets on the page showing.
    pub fn on_page(&self) -> impl Iterator<Item = &SurfaceWidget> {
        let page = self.current_page();
        self.widgets.iter().filter(move |w| w.page == page)
    }

    pub fn add_page(&mut self) -> usize {
        self.pages.push(format!("Page {}", self.pages.len() + 1));
        self.page = self.pages.len() - 1;
        self.page
    }

    /// Removes a page and its widgets (the last page can't go); later pages move up.
    pub fn remove_page(&mut self, i: usize) {
        if self.pages.len() <= 1 || i >= self.pages.len() {
            return;
        }
        self.pages.remove(i);
        self.widgets.retain(|w| w.page != i);
        for w in &mut self.widgets {
            if w.page > i {
                w.page -= 1;
            }
        }
        self.page = self.current_page();
    }

    pub fn widget(&self, id: u64) -> Option<&SurfaceWidget> {
        self.widgets.iter().find(|w| w.id == id)
    }

    pub fn widget_mut(&mut self, id: u64) -> Option<&mut SurfaceWidget> {
        self.widgets.iter_mut().find(|w| w.id == id)
    }

    /// First free spot on the page showing for a new widget (rows top to bottom).
    pub fn free_spot(&self, w: f32, h: f32) -> (f32, f32) {
        let cols = self.columns.max(1) as f32;
        for row in 0..200 {
            for col in 0..(cols - w + 1.0).max(1.0) as i32 {
                let (x, y) = (col as f32, row as f32);
                let clear = self.on_page().all(|o| x + w <= o.x || o.x + o.w <= x || y + h <= o.y || o.y + o.h <= y);
                if clear {
                    return (x, y);
                }
            }
        }
        (0.0, 0.0)
    }
}

#[cfg(test)]
mod page_tests {
    use super::*;

    #[test]
    fn widgets_live_on_pages_and_go_with_them() {
        let mut sf = ControlSurface::default();
        let a = sf.add(WidgetKind::Button);
        sf.add_page();
        let b = sf.add(WidgetKind::Fader);
        assert_eq!(sf.widget(b).unwrap().page, 1);
        assert_eq!(sf.widget(b).map(|w| (w.x, w.y)), Some((0.0, 0.0)), "each page has its own free spots");
        assert_eq!(sf.on_page().count(), 1);
        let c = sf.duplicate(a).unwrap();
        assert_eq!(sf.widget(c).unwrap().page, 1, "a copy lands on the page showing");
        sf.remove_page(0);
        assert!(sf.widget(a).is_none() && sf.widget(b).unwrap().page == 0, "page 1 moved up");
        sf.remove_page(0);
        assert_eq!(sf.pages.len(), 1, "the last page stays");
    }
}
