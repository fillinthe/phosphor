//! Small shared widgets: right-click menus that work on sliders, the right-click-proof slider
//! guard, cards, texture painting, labels, QR codes, activity lights.

use super::*;

/// Right-click menus that also open on sliders and other draggable controls. egui treats a
/// right-press on those as a drag, so its own context menu never opens; this opens on release.
pub trait RightMenu {
    fn right_menu<R>(&self, add: impl FnOnce(&mut egui::Ui) -> R) -> Option<egui::InnerResponse<R>>;
}

impl RightMenu for egui::Response {
    fn right_menu<R>(&self, add: impl FnOnce(&mut egui::Ui) -> R) -> Option<egui::InnerResponse<R>> {
        use egui::SetOpenCommand;
        let released = self.ctx.input(|i| i.pointer.button_released(egui::PointerButton::Secondary)) && self.contains_pointer();
        let cmd = if self.secondary_clicked() || released {
            Some(SetOpenCommand::Bool(true))
        } else if self.clicked() {
            Some(SetOpenCommand::Bool(false))
        } else {
            None
        };
        egui::Popup::menu(self).open_memory(cmd).at_pointer_fixed().show(add)
    }
}

/// Adds a slider or drag value, but undoes any change made with the right or middle mouse button,
/// so right-clicking opens the control's menu without moving it.
#[allow(unused_macros)]
macro_rules! guarded {
    ($ui:expr, $v:expr, $w:expr) => {{
        let before = $v;
        let r = $ui.add($w);
        if $crate::ui::not_primary(&r) {
            $v = before;
        }
        r
    }};
}
pub(crate) use guarded;

/// True while the right/middle button is pressing or releasing on this widget.
pub(crate) fn not_primary(r: &egui::Response) -> bool {
    use egui::PointerButton::{Middle, Secondary};
    let other = r.ctx.input(|i| {
        let p = &i.pointer;
        !p.primary_down() && [Secondary, Middle].iter().any(|b| p.button_down(*b) || p.button_pressed(*b) || p.button_released(*b))
    });
    other && (r.hovered() || r.contains_pointer() || r.is_pointer_button_down_on())
}

pub fn fit_rect(area: egui::Rect, aspect: f32) -> egui::Rect {
    let size = if area.width() / area.height().max(1.0) > aspect {
        egui::vec2(area.height() * aspect, area.height())
    } else {
        egui::vec2(area.width(), area.width() / aspect)
    };
    egui::Rect::from_center_size(area.center(), size)
}

pub(crate) const FULL_UV: egui::Rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

pub fn paint_texture(painter: &egui::Painter, tex: egui::TextureId, rect: egui::Rect) {
    painter.image(tex, rect, FULL_UV, Color32::WHITE);
}

pub fn label(icon: &str, text: &str) -> String {
    format!("{icon} {text}")
}

/// A click / right-click area *behind* a card, registered before the card's contents so its own
/// buttons, menus and sliders stay clickable (egui gives clicks to the widget registered last).
/// Uses the card's rect from the previous frame; `None` on the very first frame.
pub(crate) fn card_background(ui: &mut egui::Ui, id: egui::Id) -> Option<egui::Response> {
    let rect = ui.ctx().data(|d| d.get_temp::<egui::Rect>(id))?;
    Some(ui.interact(rect, id.with("bg"), Sense::click()))
}

pub(crate) fn remember_card(ui: &egui::Ui, id: egui::Id, rect: egui::Rect) {
    ui.ctx().data_mut(|d| d.insert_temp(id, rect));
}

pub(crate) fn card_frame(ui: &egui::Ui, is_selected: bool) -> egui::Frame {
    let stroke = if is_selected { Stroke::new(1.5, accent()) } else { ui.visuals().widgets.noninteractive.bg_stroke };
    let fill = if is_selected { crate::theme::tint(ui.visuals().faint_bg_color, 0.1) } else { ui.visuals().faint_bg_color };
    egui::Frame::new().fill(fill).stroke(stroke).corner_radius(6).inner_margin(egui::Margin::same(8))
}

/// Draws a QR code for `text` (no image needed: one filled square per dark module).
/// QR modules for `text`: side length and the dark cells, row by row.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn qr_cells(text: &str) -> Option<(usize, Vec<bool>)> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let n = code.width();
    Some((n, code.to_colors().iter().map(|c| *c == qrcode::Color::Dark).collect()))
}

/// The code as a picture about `size` px wide: `fg` modules on `bg` (transparent = [0,0,0,0]), quiet zone included.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn qr_image(text: &str, size: u32, fg: [u8; 4], bg: [u8; 4]) -> Option<image::RgbaImage> {
    let (n, dark) = qr_cells(text)?;
    let quiet = 4;
    let cell = (size / (n as u32 + quiet * 2)).max(1);
    let side = cell * (n as u32 + quiet * 2);
    let mut img = image::RgbaImage::from_pixel(side, side, image::Rgba(bg));
    for (i, d) in dark.iter().enumerate() {
        if *d {
            let (x, y) = ((i % n) as u32 + quiet, (i / n) as u32 + quiet);
            for dy in 0..cell {
                for dx in 0..cell {
                    img.put_pixel(x * cell + dx, y * cell + dy, image::Rgba(fg));
                }
            }
        }
    }
    Some(img)
}

pub(crate) fn qr_code(ui: &mut egui::Ui, text: &str, size: f32) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some((n, dark)) = qr_cells(text) else { return };
        let quiet = 2.0;
        let cell = size / (n as f32 + quiet * 2.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 4.0, Color32::WHITE);
        for (i, c) in dark.iter().enumerate() {
            if *c {
                let (x, y) = ((i % n) as f32, (i / n) as f32);
                let min = rect.min + egui::vec2((x + quiet) * cell, (y + quiet) * cell);
                p.rect_filled(egui::Rect::from_min_size(min, egui::vec2(cell + 0.5, cell + 0.5)), 0.0, Color32::BLACK);
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (ui, text, size);
}

// Devices: everything connected to this Mac, with disconnect / block / disable.

pub(crate) fn ago(t: std::time::Instant) -> String {
    short_duration(t.elapsed().as_secs())
}

/// "5m ago", "3h ago", "2d ago" for a Unix time in seconds (server timestamps).
pub(crate) fn ago_secs(unix: i64) -> String {
    let now = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let s = (now - unix).max(0) as u64;
    if s >= 86_400 { format!("{}d ago", s / 86_400) } else { format!("{} ago", short_duration(s)) }
}

/// 42s · 5m · 3h07m
pub(crate) fn short_duration(s: u64) -> String {
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else {
        format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
    }
}

pub(crate) fn activity_dot(ui: &mut egui::Ui, last: Option<std::time::Instant>) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
    let lit = last.map_or(0.0, |t| (1.0 - t.elapsed().as_secs_f32() / 1.5).clamp(0.0, 1.0));
    let c = if lit > 0.0 { accent().gamma_multiply(0.3 + 0.7 * lit) } else { Color32::from_gray(50) };
    ui.painter().circle_filled(r.center(), 4.0, c);
}

/// The screens/projectors to send the main output to, plus Rescan.
pub(crate) fn screen_menu(ui: &mut egui::Ui, st: &mut State) {
    if st.displays.is_empty() {
        st.displays = crate::screens::displays();
    }
    for d in st.displays.clone() {
        if ui.button(d.label()).clicked() {
            st.send_output_to(d);
            ui.close();
        }
    }
    if ui.button("Rescan displays").clicked() {
        st.displays = crate::screens::displays();
    }
}

/// How an output (Syphon, NDI, …) is doing, shown next to its toggle.
pub(crate) enum OutputStatus<'a> {
    Off,
    /// Running, nobody receiving yet.
    Waiting(&'a str),
    Receiving(usize),
    Error(&'a str),
}

/// An output's on/off toggle, mappable to a controller (right-click / Map mode).
pub(crate) fn output_toggle(ui: &mut egui::Ui, on: &mut bool, glyph: &str, name: &str, hover: &str, control: phosphor_engine::AppControl) -> egui::Response {
    super::toggle_mapped(ui, on, label(glyph, name), ControlTarget::App(control)).on_hover_text(hover)
}

pub(crate) fn output_status(ui: &mut egui::Ui, status: OutputStatus) {
    match status {
        OutputStatus::Off => {
            hint(ui, "off");
        }
        OutputStatus::Waiting(text) => {
            hint(ui, text);
        }
        OutputStatus::Receiving(n) => {
            ui.label(RichText::new(format!("{} {n} receiving", icon::CHECK_CIRCLE)).small().color(accent()));
        }
        OutputStatus::Error(e) => {
            ui.colored_label(Color32::from_rgb(255, 120, 90), e);
        }
    }
}

/// A video scrub bar: shows where the clip is, click or drag to jump. Returns the time asked for.
/// With `region` (loop in/out as 0..1 of the clip), the loop is shaded and its two edges can be dragged.
pub(crate) fn scrub_bar(ui: &mut egui::Ui, pos: f64, dur: f64, region: Option<(&mut f32, &mut f32)>) -> Option<f64> {
    let width = ui.available_width().max(120.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 18.0), Sense::click_and_drag());
    let frac = if dur > 0.0 { (pos / dur).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let x_of = |f: f32| rect.left() + rect.width() * f.clamp(0.0, 1.0);
    let f_of = |x: f32| ((x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, Color32::from_gray(32));
    let mut done = rect;
    done.set_width(rect.width() * frac);
    p.rect_filled(done, 0.0, accent().gamma_multiply(0.5));
    if let Some((lo, hi)) = region {
        if *lo > 0.0 || *hi < 1.0 {
            let r = egui::Rect::from_x_y_ranges(x_of(*lo)..=x_of(*hi), rect.y_range());
            p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(255, 196, 70, 40));
        }
        for (i, v) in [lo, hi].into_iter().enumerate() {
            let x = x_of(*v);
            let grab = egui::Rect::from_center_size(egui::pos2(x, rect.center().y), egui::vec2(10.0, rect.height() + 6.0));
            let h = ui.interact(grab, ui.id().with(("loop_edge", i)), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            if h.dragged()
                && let Some(at) = h.interact_pointer_pos()
            {
                *v = f_of(at.x);
            }
            let color = if h.hovered() || h.dragged() { Color32::WHITE } else { Color32::from_rgb(255, 196, 70) };
            p.line_segment([egui::pos2(x, rect.top() - 2.0), egui::pos2(x, rect.bottom() + 2.0)], Stroke::new(2.0, color));
            let tip = if i == 0 { [egui::pos2(x, rect.top() - 2.0), egui::pos2(x + 6.0, rect.top() - 2.0), egui::pos2(x, rect.top() + 4.0)] } else { [egui::pos2(x, rect.top() - 2.0), egui::pos2(x - 6.0, rect.top() - 2.0), egui::pos2(x, rect.top() + 4.0)] };
            p.add(egui::Shape::convex_polygon(tip.to_vec(), color, Stroke::NONE));
        }
    }
    let x = x_of(frac);
    p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(2.0, Color32::WHITE));
    let fmt = |t: f64| format!("{}:{:04.1}", (t / 60.0) as u32, t % 60.0);
    p.text(rect.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, fmt(pos), egui::FontId::monospace(10.0), Color32::WHITE);
    p.text(rect.right_center() - egui::vec2(6.0, 0.0), egui::Align2::RIGHT_CENTER, fmt(dur), egui::FontId::monospace(10.0), Color32::GRAY);
    if (resp.clicked() || resp.dragged_by(egui::PointerButton::Primary))
        && let Some(at) = resp.interact_pointer_pos()
    {
        return Some(f_of(at.x) as f64 * dur);
    }
    None
}

/// While a slider is being dragged, shows its value in a small bubble at the pointer.
pub(crate) fn value_bubble(ui: &egui::Ui, slider: &egui::Response, text: impl Into<String>) {
    if !slider.dragged() {
        return;
    }
    let Some(at) = slider.ctx.pointer_interact_pos() else { return };
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("value_bubble")));
    let galley = painter.layout_no_wrap(text.into(), egui::FontId::monospace(11.0), Color32::WHITE);
    let rect = egui::Rect::from_center_size(at - egui::vec2(0.0, 18.0), galley.size() + egui::vec2(10.0, 4.0));
    painter.rect_filled(rect, 4.0, Color32::from_rgb(20, 22, 24));
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, accent()), StrokeKind::Outside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

/// Grey help text. In a row it shrinks to the space left (full text on hover) instead of
/// overlapping its neighbours; on its own line it wraps.
pub(crate) fn hint(ui: &mut egui::Ui, text: impl Into<egui::RichText>) -> egui::Response {
    let label = egui::Label::new(text.into().weak());
    if ui.layout().main_dir().is_horizontal() {
        ui.add(label.truncate())
    } else {
        ui.add(label.wrap())
    }
}

/// A drop-down for any `Choice` setting (blend mode, fit, transition…). `small` for tight spots (Mixer strips).
pub(crate) fn choice_combo<T: phosphor_engine::Choice>(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut T, width: f32, small: bool) -> egui::Response {
    let text = |s: &str| if small { egui::RichText::new(s).small() } else { egui::RichText::new(s) };
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(text(value.label()))
        .show_ui(ui, |ui| {
            for &o in T::OPTIONS {
                ui.selectable_value(value, o, o.label());
            }
        })
        .response
}

/// A row of buttons for any `Choice` setting; `each` gets every button (to attach mapping menus, hover text…).
pub(crate) fn choice_buttons<T: phosphor_engine::Choice>(ui: &mut egui::Ui, value: &mut T, mut each: impl FnMut(&mut egui::Ui, &egui::Response)) {
    let options: Vec<(T, String)> = T::OPTIONS.iter().map(|&o| (o, o.label().to_string())).collect();
    super::segmented(ui, value, &options, |ui, r, _| each(ui, r));
}

/// A layer's name for pickers ("(removed layer)" if it's gone).
pub fn layer_name(layers: &[phosphor_engine::LayerState], id: u64) -> String {
    layers.iter().find(|l| l.id == id).map_or("(removed layer)".into(), |l| l.name.clone())
}

/// One choice per layer (top of the stack first) inside a combo box or menu; `make` turns a layer id into the value.
pub fn layer_items<T: PartialEq>(ui: &mut egui::Ui, layers: &[phosphor_engine::LayerState], value: &mut T, make: impl Fn(u64) -> T) {
    for l in layers.iter().rev() {
        ui.selectable_value(value, make(l.id), format!("Layer: {}", l.name));
    }
}

/// A level meter or progress bar with square corners (like every slider: the owner's choice). Chain `.text(…)`,
/// `.fill(…)`, `.desired_width(…)` as on an egui ProgressBar.
pub(crate) fn meter(value: f32) -> egui::ProgressBar {
    egui::ProgressBar::new(value).corner_radius(0)
}

/// A small history graph: `values` as a line (scaled to their peak), oldest left, filling `slots` columns from the
/// right so it scrolls as samples arrive; `marks` paints a red bar on the columns where something went wrong.
pub(crate) fn history_graph(ui: &mut egui::Ui, size: egui::Vec2, values: &[f32], marks: &[bool], slots: usize) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    let slots = slots.max(2);
    let step = rect.width() / (slots - 1) as f32;
    let first = slots.saturating_sub(values.len());
    let x = |i: usize| rect.right() - (slots - 1 - (first + i)) as f32 * step;
    for (i, &bad) in marks.iter().enumerate().take(values.len()) {
        if bad {
            let r = egui::Rect::from_center_size(egui::pos2(x(i), rect.center().y), egui::vec2(step.max(2.0), rect.height()));
            painter.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(230, 70, 60, 110));
        }
    }
    let peak = values.iter().copied().fold(0.0f32, f32::max);
    if peak > 0.0 && values.len() >= 2 {
        let pad = 3.0;
        let points: Vec<egui::Pos2> =
            values.iter().enumerate().map(|(i, v)| egui::pos2(x(i), rect.bottom() - pad - (v / peak).clamp(0.0, 1.0) * (rect.height() - 2.0 * pad))).collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, crate::theme::accent())));
    }
    response
}

/// A row of steps to draw on (step sequencers): drag across to set levels, or click / drag to paint gates on and
/// off. `playing` lights the step that's playing; steps are shaded in groups of four (one beat of sixteenths).
/// Returns true when a step changed.
pub(crate) fn step_bars(ui: &mut egui::Ui, steps: &mut [f32], playing: Option<usize>, gate: bool, height: f32) -> bool {
    let n = steps.len().max(1);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click_and_drag());
    let w = rect.width() / n as f32;
    let paint_id = resp.id.with("paint");
    let mut changed = false;
    let primary = ui.input(|i| i.pointer.primary_down() || i.pointer.primary_released());
    if primary
        && (resp.dragged() || resp.clicked() || resp.drag_started())
        && let Some(pos) = resp.interact_pointer_pos()
        && rect.expand(w).contains(pos)
    {
        let i = (((pos.x - rect.left()) / w).max(0.0) as usize).min(n - 1);
        let v = if gate {
            // The first step touched decides: paint on, or paint off.
            if resp.drag_started() || resp.clicked() {
                let on = steps[i] < 0.5;
                ui.ctx().data_mut(|d| d.insert_temp(paint_id, on));
            }
            let on = ui.ctx().data(|d| d.get_temp::<bool>(paint_id)).unwrap_or(true);
            on as u8 as f32
        } else {
            (1.0 - (pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0)
        };
        if steps[i] != v {
            steps[i] = v;
            changed = true;
        }
    }
    let p = ui.painter();
    for (i, v) in steps.iter().enumerate() {
        let x0 = rect.left() + i as f32 * w + 1.0;
        let cell = egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x0 + w - 2.0, rect.bottom()));
        p.rect_filled(cell, 0.0, Color32::from_gray(if (i / 4) % 2 == 0 { 30 } else { 22 }));
        let lit = playing == Some(i);
        let color = if lit { accent() } else { accent().gamma_multiply(0.5) };
        if gate {
            if *v >= 0.5 {
                p.rect_filled(cell.shrink(2.0), 0.0, color);
            }
        } else {
            let bar = egui::Rect::from_min_max(egui::pos2(cell.left(), rect.bottom() - v * rect.height()), cell.right_bottom());
            p.rect_filled(bar, 0.0, color);
        }
        if lit {
            p.rect_stroke(cell, 0.0, Stroke::new(1.0, Color32::WHITE), StrokeKind::Inside);
        }
    }
    changed
}
