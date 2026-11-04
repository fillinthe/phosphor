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
    /// A row of named buttons, one picked at a time (a blend mode, a clip, a direction): its value is the middle of
    /// the picked one's slice of 0..1, the way pick-one controls read it.
    Choices,
    /// Words you type, shown on a text layer (messages on screen during a show).
    Text,
    /// A row of bars side by side, each with its own value and targets (draw across them in one stroke): eight
    /// layer levels, an EQ, a step pattern.
    Multi,
    /// A value worked out from other controls by a formula (`a * b`, `max(a, b)`, `sin(beat)`…), kept up to date
    /// by itself; it drives its targets like a fader.
    Computed,
}

impl WidgetKind {
    pub const ALL: [WidgetKind; 11] = [WidgetKind::Button, WidgetKind::Toggle, WidgetKind::Fader, WidgetKind::Slider, WidgetKind::Multi, WidgetKind::XyPad, WidgetKind::Color, WidgetKind::Choices, WidgetKind::Computed, WidgetKind::Text, WidgetKind::Label];

    pub fn name(self) -> &'static str {
        match self {
            WidgetKind::Button => "Button",
            WidgetKind::Toggle => "Toggle",
            WidgetKind::Fader => "Fader",
            WidgetKind::Slider => "Slider",
            WidgetKind::XyPad => "XY Pad",
            WidgetKind::Label => "Label",
            WidgetKind::Color => "Colour",
            WidgetKind::Choices => "Choices",
            WidgetKind::Text => "Text field",
            WidgetKind::Multi => "Multi-slider",
            WidgetKind::Computed => "Computed value",
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
            WidgetKind::Choices => (6.0, 1.0),
            WidgetKind::Text => (6.0, 1.0),
            WidgetKind::Multi => (6.0, 3.0),
            WidgetKind::Computed => (4.0, 1.0),
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
    /// Also send its value as OSC to `ControlSurface::osc_host` at this address ("" = don't).
    pub osc_out: String,
    /// Audience play: guests can use it (off = it's hidden from them).
    pub guests: bool,
    /// Audience play: the part of its range guests can reach (low, high; 0..1), e.g. a strobe up to 30%.
    pub guest_range: [f32; 2],
    /// Choices: the buttons' names, in order.
    pub items: Vec<String>,
    /// Text field: the words, and the text layer they go on.
    pub text: String,
    pub text_layer: Option<u64>,
    /// Multi-slider: its bars, left to right.
    pub bars: Vec<SurfaceBar>,
    /// Computed value: the formula, and the controls it reads (`a`, `b`, `c`… in order).
    pub formula: String,
    pub inputs: Vec<super::ControlTarget>,
    /// Fader / slider: moves by itself (LFO, sound, beat…) around where it's set; its targets follow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion: Option<crate::Modulation>,
    /// Where a moving fader is right now (what its targets got).
    #[serde(skip)]
    pub live: f32,
}

/// One bar of a multi-slider: its value and what it drives.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfaceBar {
    pub value: f32,
    pub bindings: Vec<Binding>,
}

/// Bars a new multi-slider starts with.
pub const DEFAULT_BARS: usize = 8;

impl Default for SurfaceWidget {
    fn default() -> Self {
        Self { id: 0, kind: WidgetKind::Button, x: 0.0, y: 0.0, w: 2.0, h: 2.0, label: String::new(), color: [0, 200, 190], bindings: Vec::new(), bindings_y: Vec::new(), bindings_z: Vec::new(), value: 0.0, value_y: 0.0, value_z: 1.0, page: 0, osc_out: String::new(), guests: true, guest_range: [0.0, 1.0], items: Vec::new(), text: String::new(), text_layer: None, bars: Vec::new(), formula: String::new(), inputs: Vec::new(), motion: None, live: 0.0 }
    }
}

impl SurfaceWidget {
    /// A value a guest sent, kept inside their range (-1 = a toggle's flip, left alone).
    /// Back inside `cols` columns and above the top, at least a cell big (after x / y / w / h were typed).
    pub fn keep_inside(&mut self, cols: f32) {
        let cols = cols.max(1.0);
        self.w = self.w.clamp(1.0, cols);
        self.h = self.h.max(1.0);
        self.x = self.x.clamp(0.0, cols - self.w);
        self.y = self.y.max(0.0);
    }

    pub fn guest_value(&self, v: f32) -> f32 {
        if v < 0.0 {
            return v;
        }
        let [lo, hi] = self.guest_range;
        v.clamp(lo.min(hi), hi.max(lo))
    }

    /// Computed value: the formula worked out with its inputs' values (`a`, `b`… 0..1), the show's time in seconds
    /// (`t`), the beat count (`beat`) and its own value (`val`); clamped to 0..1. None when it doesn't add up.
    pub fn compute(&self, inputs: &[f32], time: f64, beats: f64) -> Option<f32> {
        let var = |name: &str| -> Option<f64> {
            match name {
                "t" => Some(time),
                "beat" => Some(beats),
                "val" | "$VAL" | "VAL" => Some(self.value as f64),
                _ => {
                    let mut c = name.chars();
                    let (Some(ch @ 'a'..='h'), None) = (c.next(), c.next()) else { return None };
                    Some(inputs.get(ch as usize - 'a' as usize).copied().unwrap_or(0.0) as f64)
                }
            }
        };
        let v = crate::expr::eval(self.formula.trim(), &var)?;
        v.is_finite().then(|| (v as f32).clamp(0.0, 1.0))
    }

    /// Multi-slider: makes it `n` bars (1..=64), keeping the ones it has.
    pub fn set_bar_count(&mut self, n: usize) {
        self.bars.resize_with(n.clamp(1, 64), Default::default);
    }

    /// Multi-slider: the bar under `x` (0..1 across the widget).
    pub fn bar_at(&self, x: f32) -> usize {
        ((x.clamp(0.0, 1.0) * self.bars.len() as f32) as usize).min(self.bars.len().saturating_sub(1))
    }

    /// Choices: how many there are (at least 2), the one picked, and the value that picks choice `i`.
    pub fn choice_count(&self) -> usize {
        self.items.len().max(2)
    }

    pub fn picked(&self) -> usize {
        crate::pick(self.value, self.choice_count())
    }

    pub fn choice_value(&self, i: usize) -> f32 {
        crate::pick_unit(i, self.choice_count())
    }

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

/// How several selected widgets get lined up (`ControlSurface::arrange`). Sizes copy the first one picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrange {
    Left,
    Centre,
    Right,
    Top,
    Middle,
    Bottom,
    /// Even gaps between them, left to right (the outer two stay).
    SpreadAcross,
    /// Even gaps, top to bottom.
    SpreadDown,
    SameWidth,
    SameHeight,
    SameSize,
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
    /// Where widgets with an OSC address send their values (another app or computer; "" = nowhere).
    pub osc_host: String,
    pub osc_port: u16,
}

impl Default for ControlSurface {
    fn default() -> Self {
        Self { widgets: Vec::new(), columns: 12, next_id: 1, pages: vec!["Main".into()], page: 0, osc_host: String::new(), osc_port: 9000 }
    }
}

impl ControlSurface {
    /// What widgets with an OSC address send: (widget id, address, values). XY pads send x y, colours r g b.
    pub fn osc_messages(&self) -> Vec<(u64, String, Vec<f32>)> {
        if self.osc_host.trim().is_empty() {
            return Vec::new();
        }
        self.widgets
            .iter()
            .filter(|w| w.osc_out.starts_with('/') && w.kind != WidgetKind::Label)
            .map(|w| {
                let values = match w.kind {
                    WidgetKind::XyPad => vec![w.value, w.value_y],
                    WidgetKind::Color => w.rgb().to_vec(),
                    WidgetKind::Multi => w.bars.iter().map(|b| b.value).collect(),
                    _ => vec![w.value],
                };
                (w.id, w.osc_out.clone(), values)
            })
            .collect()
    }

    /// Adds a widget to the page showing, at the first free spot there.
    pub fn add(&mut self, kind: WidgetKind) -> u64 {
        self.add_at(kind, None)
    }

    /// Adds a widget to the page showing at a spot (in cells; kept inside the columns), or at the first free spot.
    pub fn add_at(&mut self, kind: WidgetKind, at: Option<(f32, f32)>) -> u64 {
        let (w, h) = kind.default_size();
        let mut widget = SurfaceWidget { kind, w, h, ..Default::default() };
        if kind == WidgetKind::Multi {
            widget.set_bar_count(DEFAULT_BARS);
        }
        let id = self.place(widget, at);
        if let Some(w) = self.widget_mut(id) {
            w.label = format!("{} {id}", kind.name());
        }
        id
    }

    /// A copy of a widget at the next free spot on the page showing.
    pub fn duplicate(&mut self, id: u64) -> Option<u64> {
        let copy = self.widget(id)?.clone();
        Some(self.place(copy, None))
    }

    /// Puts a widget (new, a copy, or one pasted from another project) on the page showing with a fresh id, at a spot
    /// (in cells; kept inside the columns) or at the first free spot. Returns its id.
    pub fn place(&mut self, mut widget: SurfaceWidget, at: Option<(f32, f32)>) -> u64 {
        widget.id = self.next_id;
        self.next_id += 1;
        widget.page = self.current_page();
        (widget.x, widget.y) = match at {
            Some((x, y)) => self.inside(x, y, widget.w),
            None => self.free_spot(widget.w, widget.h),
        };
        self.widgets.push(widget);
        self.next_id - 1
    }

    /// Removes a widget. Mappings that pointed at it are the app's to drop (`Composition::remove_widget`).
    pub fn remove(&mut self, id: u64) -> Option<SurfaceWidget> {
        let i = self.widgets.iter().position(|w| w.id == id)?;
        Some(self.widgets.remove(i))
    }

    /// Moves a widget by (dx, dy) cells, kept inside the columns and above the top.
    pub fn move_by(&mut self, id: u64, dx: f32, dy: f32) {
        let cols = self.columns.max(1) as f32;
        if let Some(w) = self.widget_mut(id) {
            w.x = (w.x + dx).clamp(0.0, (cols - w.w).max(0.0));
            w.y = (w.y + dy).max(0.0);
        }
    }

    /// Draws a widget over the others (last drawn = on top) or under them.
    pub fn to_front(&mut self, id: u64) {
        if let Some(w) = self.remove(id) {
            self.widgets.push(w);
        }
    }

    pub fn to_back(&mut self, id: u64) {
        if let Some(w) = self.remove(id) {
            self.widgets.insert(0, w);
        }
    }

    /// Moves several widgets together by (dx, dy) cells: the group keeps its shape and stops at the edges as one.
    pub fn move_many(&mut self, ids: &[u64], dx: f32, dy: f32) {
        let cols = self.columns.max(1) as f32;
        let picked = || self.widgets.iter().filter(|w| ids.contains(&w.id));
        let (mut left, mut right, mut top) = (f32::MAX, f32::MIN, f32::MAX);
        for w in picked() {
            left = left.min(w.x);
            right = right.max(w.x + w.w);
            top = top.min(w.y);
        }
        if left == f32::MAX {
            return;
        }
        let dx = dx.clamp(-left, (cols - right).max(-left));
        let dy = dy.max(-top);
        for w in self.widgets.iter_mut().filter(|w| ids.contains(&w.id)) {
            w.x += dx;
            w.y += dy;
        }
    }

    /// Several widgets pasted together (a copy of a selection): they keep their shape, with the group's top-left
    /// corner at `at` (or at the first free spot for the whole group). Returns the new ids.
    pub fn place_many(&mut self, widgets: Vec<SurfaceWidget>, at: Option<(f32, f32)>) -> Vec<u64> {
        let left = widgets.iter().map(|w| w.x).fold(f32::MAX, f32::min);
        let top = widgets.iter().map(|w| w.y).fold(f32::MAX, f32::min);
        let right = widgets.iter().map(|w| w.x + w.w).fold(f32::MIN, f32::max);
        let bottom = widgets.iter().map(|w| w.y + w.h).fold(f32::MIN, f32::max);
        if widgets.is_empty() {
            return Vec::new();
        }
        let (x0, y0) = match at {
            Some((x, y)) => self.inside(x, y, right - left),
            None => self.free_spot(right - left, bottom - top),
        };
        widgets.into_iter().map(|w| {
            let spot = (x0 + w.x - left, y0 + w.y - top);
            self.place(w, Some(spot))
        }).collect()
    }

    /// Puts a widget back inside the columns and above the top (after its x / y / w / h were typed).
    pub fn keep_inside(&mut self, id: u64) {
        let cols = self.columns.max(1) as f32;
        if let Some(w) = self.widget_mut(id) {
            w.keep_inside(cols);
        }
    }

    /// Lines up, spreads out or matches the size of several widgets (two or more; one does nothing).
    pub fn arrange(&mut self, ids: &[u64], how: Arrange) {
        let mut picked: Vec<usize> = self.widgets.iter().enumerate().filter(|(_, w)| ids.contains(&w.id)).map(|(i, _)| i).collect();
        if picked.len() < 2 {
            return;
        }
        let ws = &mut self.widgets;
        let left = picked.iter().map(|&i| ws[i].x).fold(f32::MAX, f32::min);
        let right = picked.iter().map(|&i| ws[i].x + ws[i].w).fold(f32::MIN, f32::max);
        let top = picked.iter().map(|&i| ws[i].y).fold(f32::MAX, f32::min);
        let bottom = picked.iter().map(|&i| ws[i].y + ws[i].h).fold(f32::MIN, f32::max);
        let first = ids.first().and_then(|id| ws.iter().find(|w| w.id == *id)).unwrap_or(&ws[picked[0]]).clone();
        match how {
            Arrange::Left => picked.iter().for_each(|&i| ws[i].x = left),
            Arrange::Right => picked.iter().for_each(|&i| ws[i].x = right - ws[i].w),
            Arrange::Centre => picked.iter().for_each(|&i| ws[i].x = (left + right - ws[i].w) / 2.0),
            Arrange::Top => picked.iter().for_each(|&i| ws[i].y = top),
            Arrange::Bottom => picked.iter().for_each(|&i| ws[i].y = bottom - ws[i].h),
            Arrange::Middle => picked.iter().for_each(|&i| ws[i].y = (top + bottom - ws[i].h) / 2.0),
            Arrange::SpreadAcross | Arrange::SpreadDown => {
                let across = how == Arrange::SpreadAcross;
                picked.sort_by(|&a, &b| {
                    let (ka, kb) = if across { (ws[a].x, ws[b].x) } else { (ws[a].y, ws[b].y) };
                    ka.total_cmp(&kb)
                });
                let total: f32 = picked.iter().map(|&i| if across { ws[i].w } else { ws[i].h }).sum();
                let span = if across { right - left } else { bottom - top };
                let gap = ((span - total) / (picked.len() - 1) as f32).max(0.0);
                let mut at = if across { left } else { top };
                for &i in &picked {
                    if across {
                        ws[i].x = at;
                        at += ws[i].w + gap;
                    } else {
                        ws[i].y = at;
                        at += ws[i].h + gap;
                    }
                }
            }
            Arrange::SameWidth => picked.iter().for_each(|&i| ws[i].w = first.w),
            Arrange::SameHeight => picked.iter().for_each(|&i| ws[i].h = first.h),
            Arrange::SameSize => picked.iter().for_each(|&i| (ws[i].w, ws[i].h) = (first.w, first.h)),
        }
        for &id in ids {
            self.keep_inside(id);
        }
    }

    /// A spot kept inside the columns (and above the top) for a widget this wide.
    fn inside(&self, x: f32, y: f32, w: f32) -> (f32, f32) {
        let cols = self.columns.max(1) as f32;
        (x.clamp(0.0, (cols - w).max(0.0)), y.max(0.0))
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

    #[test]
    fn widgets_are_placed_moved_ordered_and_removed() {
        let mut sf = ControlSurface { columns: 8, ..Default::default() };
        let a = sf.add_at(WidgetKind::XyPad, Some((6.0, 2.5)));
        assert_eq!(sf.widget(a).map(|w| (w.x, w.y)), Some((4.0, 2.5)), "kept inside the columns (4 wide on 8)");
        let b = sf.add_at(WidgetKind::Button, Some((-1.0, -1.0)));
        assert_eq!(sf.widget(b).map(|w| (w.x, w.y)), Some((0.0, 0.0)));
        sf.move_by(b, 0.25, 0.25);
        sf.move_by(b, 100.0, 0.0);
        assert_eq!(sf.widget(b).map(|w| (w.x, w.y)), Some((6.0, 0.25)), "nudged, then stopped at the right edge");
        sf.to_front(a);
        assert_eq!(sf.widgets.last().map(|w| w.id), Some(a));
        sf.to_back(a);
        assert_eq!(sf.widgets.first().map(|w| w.id), Some(a));
        let c = sf.place(sf.widget(a).unwrap().clone(), Some((1.0, 1.0)));
        assert_ne!(c, a);
        assert_eq!(sf.widget(c).map(|w| (w.kind, w.x, w.y)), Some((WidgetKind::XyPad, 1.0, 1.0)));
        assert!(sf.remove(a).is_some() && sf.remove(a).is_none());
        assert_eq!(sf.widgets.len(), 2);
    }

    #[test]
    fn groups_move_paste_and_line_up_together() {
        let mut sf = ControlSurface { columns: 8, ..Default::default() };
        // Buttons are 2 x 2.
        let a = sf.add_at(WidgetKind::Button, Some((0.0, 0.0)));
        let b = sf.add_at(WidgetKind::Button, Some((3.0, 2.0)));
        sf.move_many(&[a, b], 10.0, -5.0);
        assert_eq!((sf.widget(a).unwrap().x, sf.widget(b).unwrap().x), (3.0, 6.0), "the group stops at the right edge as one");
        assert_eq!((sf.widget(a).unwrap().y, sf.widget(b).unwrap().y), (0.0, 2.0), "and at the top");
        let copies = sf.place_many(vec![sf.widget(a).unwrap().clone(), sf.widget(b).unwrap().clone()], Some((7.0, 4.0)));
        assert_eq!(copies.len(), 2);
        let (ca, cb) = (sf.widget(copies[0]).unwrap(), sf.widget(copies[1]).unwrap());
        assert_eq!((ca.x, ca.y, cb.x, cb.y), (3.0, 4.0, 6.0, 6.0), "pasted with its shape, kept inside");
        sf.arrange(&[a, b], Arrange::Left);
        assert_eq!(sf.widget(b).unwrap().x, 3.0);
        sf.arrange(&[a, b], Arrange::Top);
        assert_eq!(sf.widget(b).unwrap().y, 0.0);
        if let Some(w) = sf.widget_mut(b) {
            w.x = 6.0;
        }
        let c = sf.add_at(WidgetKind::Fader, Some((4.0, 0.0))); // 1 x 4
        sf.arrange(&[a, b, c], Arrange::SpreadAcross);
        assert_eq!((sf.widget(a).unwrap().x, sf.widget(c).unwrap().x, sf.widget(b).unwrap().x), (3.0, 5.0, 6.0), "even gaps between the outer two (none fit here)");
        sf.arrange(&[c, a], Arrange::SameSize);
        assert_eq!((sf.widget(a).unwrap().w, sf.widget(a).unwrap().h), (1.0, 4.0), "sizes copy the first one picked");
        if let Some(w) = sf.widget_mut(c) {
            (w.x, w.w) = (6.0, 5.0);
        }
        sf.keep_inside(c);
        assert_eq!((sf.widget(c).unwrap().x, sf.widget(c).unwrap().w), (3.0, 5.0));
    }
}

#[cfg(test)]
mod osc_out_tests {
    use super::*;

    #[test]
    fn widgets_send_osc_only_with_a_host_and_address() {
        let mut s = ControlSurface::default();
        s.widgets.push(SurfaceWidget { id: 1, kind: WidgetKind::Fader, value: 0.25, osc_out: "/lights/dim".into(), ..Default::default() });
        s.widgets.push(SurfaceWidget { id: 2, kind: WidgetKind::XyPad, value: 0.1, value_y: 0.9, osc_out: "/td/xy".into(), ..Default::default() });
        s.widgets.push(SurfaceWidget { id: 3, kind: WidgetKind::Fader, ..Default::default() });
        assert!(s.osc_messages().is_empty(), "no host: nothing goes out");
        s.osc_host = "127.0.0.1".into();
        let m = s.osc_messages();
        assert_eq!(m, vec![(1, "/lights/dim".to_string(), vec![0.25]), (2, "/td/xy".to_string(), vec![0.1, 0.9])]);
    }
}

impl crate::Composition {
    /// Removes a surface widget and every controller mapping that pointed at it.
    pub fn remove_widget(&mut self, id: u64) {
        use super::ControlTarget;
        self.surface.remove(id);
        self.control_maps.retain(|m| !matches!(m.target, ControlTarget::Surface(i) | ControlTarget::SurfaceY(i) | ControlTarget::SurfaceBar { widget: i, .. } if i == id));
    }

    /// A Text field widget's words: kept on the widget and put on its text layer (live text off, so they stay).
    pub fn set_surface_text(&mut self, widget: u64, text: &str) {
        let Some(w) = self.surface.widget_mut(widget) else { return };
        w.text = text.to_string();
        let Some(layer) = w.text_layer else { return };
        if let Some(l) = self.layer_mut(layer)
            && let crate::SourceSpec::Text(style) = &mut l.source
        {
            style.text = text.to_string();
            style.live = crate::LiveText::Off;
        }
    }
}

#[cfg(test)]
mod text_tests {
    #[test]
    fn a_text_field_writes_on_its_layer() {
        let mut c = crate::Composition::default();
        c.layers.push(crate::LayerState { id: 4, source: crate::SourceSpec::Text(crate::TextStyle::default()), ..Default::default() });
        let id = c.surface.add(super::WidgetKind::Text);
        c.surface.widget_mut(id).unwrap().text_layer = Some(4);
        c.set_surface_text(id, "Happy birthday Sam");
        let crate::SourceSpec::Text(s) = &c.layers[0].source else { panic!() };
        assert_eq!(s.text, "Happy birthday Sam");
        assert_eq!(c.surface.widgets[0].text, "Happy birthday Sam");
    }
}

#[cfg(test)]
mod computed_tests {
    use super::*;

    #[test]
    fn a_computed_value_works_out_its_formula() {
        let w = |f: &str| SurfaceWidget { kind: WidgetKind::Computed, formula: f.into(), value: 0.3, ..Default::default() };
        assert_eq!(w("a * b").compute(&[0.5, 0.5], 0.0, 0.0), Some(0.25));
        assert_eq!(w("max(a, b)").compute(&[0.2, 0.7], 0.0, 0.0), Some(0.7));
        assert_eq!(w("1 - c").compute(&[0.0], 0.0, 0.0), Some(1.0), "a missing input reads 0");
        assert_eq!(w("a + b").compute(&[0.9, 0.9], 0.0, 0.0), Some(1.0), "kept in 0..1");
        assert_eq!(w("val").compute(&[], 0.0, 0.0), Some(0.3));
        assert_eq!(w("sin(beat)").compute(&[], 0.0, 0.25), Some(1.0), "a quarter turn");
        assert_eq!(w("fract(t / 4)").compute(&[], 5.0, 0.0), Some(0.25));
        assert_eq!(w("a / 0").compute(&[1.0], 0.0, 0.0), Some(0.0), "dividing by 0 gives 0");
        assert_eq!(w("nonsense(").compute(&[], 0.0, 0.0), None);
        assert_eq!(w("i").compute(&[], 0.0, 0.0), None, "only a..h are inputs");
    }
}
