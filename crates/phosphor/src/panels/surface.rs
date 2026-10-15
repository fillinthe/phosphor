//! Control Surface: your own buttons, toggles, faders, sliders and XY pads (VDMX-style). Each
//! widget drives any number of targets, and can itself be mapped to MIDI / OSC / keys.

use super::*;
use phosphor_engine::{SurfaceWidget, WidgetKind};

const PALETTE: [[u8; 3]; 8] = [[0, 200, 190], [255, 196, 70], [255, 105, 97], [160, 120, 255], [90, 170, 255], [120, 220, 110], [255, 140, 200], [200, 200, 200]];

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

#[derive(Clone, Copy)]
enum Grab {
    Move { id: u64, start: egui::Pos2, x: f32, y: f32 },
    Resize { id: u64, start: egui::Pos2, w: f32, h: f32 },
}

pub(crate) fn surface(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    let edit_id = egui::Id::new("surface_edit");
    let sel_id = egui::Id::new("surface_selected");
    let grab_id = egui::Id::new("surface_grab");
    let mut edit: bool = ui.ctx().data(|d| d.get_temp(edit_id)).unwrap_or(st.comp.surface.widgets.is_empty());
    let mut selected: Option<u64> = ui.ctx().data(|d| d.get_temp(sel_id)).flatten();

    // Toolbar.
    ui.horizontal(|ui| {
        ui.toggle_value(&mut edit, label(icon::PENCIL_SIMPLE, "Edit")).on_hover_text("Edit: add, move (drag), resize (drag the corner) and wire up widgets. Off: play them.");
        if edit {
            ui.menu_button(label(icon::PLUS, "Add"), |ui| {
                for kind in WidgetKind::ALL {
                    if ui.button(kind.name()).clicked() {
                        let (w, h) = kind.default_size();
                        let (x, y) = st.comp.surface.free_spot(w, h);
                        selected = Some(st.comp.surface.add(kind, x, y));
                        ui.close();
                    }
                }
            });
            ui.label(RichText::new("Columns").small().color(Color32::GRAY));
            guarded!(ui, st.comp.surface.columns, egui::DragValue::new(&mut st.comp.surface.columns).range(4..=32));
            hint(ui, "Drag to move · corner to resize · right-click to map");
        } else if st.comp.surface.widgets.is_empty() {
            hint(ui, "Empty. Turn on Edit and add buttons, faders and pads.");
        }
    });

    let targets = crate::control::all_targets(&st.comp);
    let inspector = edit && selected.is_some_and(|id| st.comp.surface.widget(id).is_some());
    let mut sets: Vec<(ControlTarget, f32)> = Vec::new();
    let mut grab: Option<Grab> = ui.ctx().data(|d| d.get_temp(grab_id)).flatten();
    let mut open_editor: Option<u64> = None;

    ui.horizontal_top(|ui| {
        let canvas_w = (ui.available_width() - if inspector { 330.0 } else { 0.0 }).max(200.0);
        let cols = st.comp.surface.columns.max(1) as f32;
        let used_rows = st.comp.surface.widgets.iter().map(|w| w.y + w.h).fold(0.0f32, f32::max);
        let rows = used_rows + if edit { 2.0 } else { 0.0 };
        // Square cells that fit the whole surface in the panel (across and down); scroll only when tiny.
        let avail_h = ui.available_height() - 4.0;
        let cell = (canvas_w / cols).min(avail_h / rows.max(4.0)).max(24.0);
        let height = (rows * cell).max(avail_h);
        egui::ScrollArea::vertical().id_salt("surface_scroll").max_width(canvas_w).auto_shrink([false, false]).show(ui, |ui| {
            let (canvas, _) = ui.allocate_exact_size(egui::vec2(canvas_w, height), Sense::hover());
            let painter = ui.painter_at(canvas);
            if edit {
                for gx in 0..=cols as i32 {
                    for gy in 0..=(height / cell) as i32 {
                        painter.circle_filled(canvas.min + egui::vec2(gx as f32 * cell, gy as f32 * cell), 1.0, Color32::from_gray(55));
                    }
                }
            }
            let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
            for w in &st.comp.surface.widgets {
                let rect = egui::Rect::from_min_size(canvas.min + egui::vec2(w.x * cell, w.y * cell), egui::vec2(w.w * cell, w.h * cell)).shrink(3.0);
                let id = egui::Id::new(("surface_widget", w.id));
                let target = ControlTarget::Surface(w.id);
                if edit {
                    let body = ui.interact(rect, id, Sense::click_and_drag());
                    let handle = egui::Rect::from_min_size(rect.max - egui::vec2(14.0, 14.0), egui::vec2(14.0, 14.0));
                    let corner = ui.interact(handle, id.with("resize"), Sense::drag());
                    if body.clicked() || body.drag_started() {
                        selected = Some(w.id);
                    }
                    if let Some(p) = pointer {
                        if body.drag_started() {
                            grab = Some(Grab::Move { id: w.id, start: p, x: w.x, y: w.y });
                        }
                        if corner.drag_started() {
                            grab = Some(Grab::Resize { id: w.id, start: p, w: w.w, h: w.h });
                            selected = Some(w.id);
                        }
                    }
                    draw_widget(&painter, w, rect, true);
                    let outline = if selected == Some(w.id) { Stroke::new(2.0, ACCENT) } else { Stroke::new(1.0, Color32::from_gray(90)) };
                    painter.rect_stroke(rect, 6.0, outline, StrokeKind::Outside);
                    painter.text(handle.center(), egui::Align2::CENTER_CENTER, icon::ARROWS_OUT_SIMPLE, egui::FontId::proportional(11.0), Color32::from_gray(160));
                    if w.bindings.is_empty() && w.bindings_y.is_empty() && w.kind != WidgetKind::Label {
                        painter.text(rect.left_bottom() + egui::vec2(4.0, -3.0), egui::Align2::LEFT_BOTTOM, "not wired", egui::FontId::proportional(10.0), WARN);
                    }
                    continue;
                }
                // Play.
                let resp = ui.interact(rect, id, Sense::click_and_drag());
                let pos = |p: egui::Pos2| (((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0), (1.0 - (p.y - rect.top()) / rect.height()).clamp(0.0, 1.0));
                match w.kind {
                    WidgetKind::Button => {
                        let down = resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down());
                        if down != (w.value > 0.5) {
                            sets.push((target.clone(), down as u8 as f32));
                        }
                    }
                    WidgetKind::Toggle => {
                        if resp.clicked() {
                            sets.push((target.clone(), crate::control::FLIP));
                        }
                    }
                    WidgetKind::Fader | WidgetKind::Slider | WidgetKind::XyPad => {
                        if (resp.dragged_by(egui::PointerButton::Primary) || resp.clicked())
                            && let Some(p) = pointer
                        {
                            let (x, y) = pos(p);
                            match w.kind {
                                WidgetKind::Fader => sets.push((target.clone(), y)),
                                WidgetKind::Slider => sets.push((target.clone(), x)),
                                _ => {
                                    sets.push((target.clone(), x));
                                    sets.push((ControlTarget::SurfaceY(w.id), y));
                                }
                            }
                        }
                    }
                    WidgetKind::Label => {}
                }
                draw_widget(&painter, w, rect, false);
                if w.kind == WidgetKind::Label {
                    continue;
                }
                mapped_badge(ui, &resp, &target);
                let is_xy = w.kind == WidgetKind::XyPad;
                let wid = w.id;
                resp.on_hover_text(binding_summary(w, &targets)).right_menu(|ui| {
                    if is_xy {
                        ui.label(RichText::new("Horizontal (X)").small().color(Color32::GRAY));
                    }
                    learn_items(ui, &target);
                    if is_xy {
                        ui.separator();
                        ui.label(RichText::new("Vertical (Y)").small().color(Color32::GRAY));
                        learn_items(ui, &ControlTarget::SurfaceY(wid));
                    }
                    ui.separator();
                    if ui.button(label(icon::PENCIL_SIMPLE, "Edit widget…")).clicked() {
                        open_editor = Some(wid);
                        ui.close();
                    }
                });
            }
            // Dragging in edit mode: snap to the grid.
            if let (Some(g), Some(p)) = (grab, pointer) {
                if ui.input(|i| i.pointer.primary_down()) {
                    let surface = &mut st.comp.surface;
                    match g {
                        Grab::Move { id, start, x, y } => {
                            if let Some(w) = surface.widget_mut(id) {
                                w.x = (x + ((p.x - start.x) / cell).round()).clamp(0.0, (cols - w.w).max(0.0));
                                w.y = (y + ((p.y - start.y) / cell).round()).max(0.0);
                            }
                        }
                        Grab::Resize { id, start, w: w0, h: h0 } => {
                            if let Some(w) = surface.widget_mut(id) {
                                w.w = (w0 + ((p.x - start.x) / cell).round()).clamp(1.0, (cols - w.x).max(1.0));
                                w.h = (h0 + ((p.y - start.y) / cell).round()).max(1.0);
                            }
                        }
                    }
                } else {
                    grab = None;
                }
            }
        });
        if inspector && let Some(id) = selected {
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(310.0);
                if !widget_editor(ui, st, id, &targets) {
                    selected = None;
                }
            });
        }
    });

    if let Some(id) = open_editor {
        edit = true;
        selected = Some(id);
    }
    for (t, v) in sets {
        st.set_control(&t, v, now);
    }
    ui.ctx().data_mut(|d| {
        d.insert_temp(edit_id, edit);
        d.insert_temp(sel_id, selected);
        d.insert_temp(grab_id, grab);
    });
}

/// "Drives: Layer 1 · opacity, Master fade" for the hover text.
fn binding_summary(w: &SurfaceWidget, targets: &[(ControlTarget, String)]) -> String {
    let names = |b: &[phosphor_engine::Binding]| b.iter().map(|b| targets.iter().find(|(t, _)| *t == b.target).map_or("(removed)".to_string(), |(_, n)| n.clone())).collect::<Vec<_>>().join(", ");
    let mut s = if w.bindings.is_empty() { "Not wired to anything yet (Edit → select it)".to_string() } else { format!("Drives: {}", names(&w.bindings)) };
    if !w.bindings_y.is_empty() {
        s += &format!("\nY drives: {}", names(&w.bindings_y));
    }
    s + "\nRight-click to map it to MIDI / OSC / a key"
}

fn draw_widget(p: &egui::Painter, w: &SurfaceWidget, rect: egui::Rect, edit: bool) {
    let c = rgb(w.color);
    let font = egui::FontId::proportional(12.0);
    match w.kind {
        WidgetKind::Button | WidgetKind::Toggle => {
            let on = w.value > 0.5;
            p.rect_filled(rect, 6.0, if on { c } else { c.gamma_multiply(0.18) });
            p.rect_stroke(rect, 6.0, Stroke::new(1.0, c.gamma_multiply(0.6)), StrokeKind::Inside);
            let text = if on { Color32::BLACK } else { Color32::WHITE };
            p.text(rect.center(), egui::Align2::CENTER_CENTER, &w.label, font, text);
        }
        WidgetKind::Fader | WidgetKind::Slider => {
            p.rect_filled(rect, 6.0, Color32::from_gray(28));
            let mut fill = rect;
            if w.kind == WidgetKind::Fader {
                fill.set_top(rect.bottom() - rect.height() * w.value);
            } else {
                fill.set_right(rect.left() + rect.width() * w.value);
            }
            p.rect_filled(fill, 6.0, c.gamma_multiply(0.85));
            p.rect_stroke(rect, 6.0, Stroke::new(1.0, c.gamma_multiply(0.5)), StrokeKind::Inside);
            let anchor = if w.kind == WidgetKind::Fader { (rect.center_top() + egui::vec2(0.0, 4.0), egui::Align2::CENTER_TOP) } else { (rect.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER) };
            p.text(anchor.0, anchor.1, &w.label, font.clone(), Color32::WHITE);
            if !edit {
                let corner = if w.kind == WidgetKind::Fader { (rect.center_bottom() - egui::vec2(0.0, 4.0), egui::Align2::CENTER_BOTTOM) } else { (rect.right_center() - egui::vec2(6.0, 0.0), egui::Align2::RIGHT_CENTER) };
                p.text(corner.0, corner.1, format!("{:.0}%", w.value * 100.0), egui::FontId::monospace(10.0), Color32::from_gray(220));
            }
        }
        WidgetKind::XyPad => {
            p.rect_filled(rect, 6.0, Color32::from_gray(24));
            p.rect_stroke(rect, 6.0, Stroke::new(1.0, c.gamma_multiply(0.5)), StrokeKind::Inside);
            let pt = egui::pos2(rect.left() + rect.width() * w.value, rect.bottom() - rect.height() * w.value_y);
            let guide = Stroke::new(1.0, c.gamma_multiply(0.35));
            p.line_segment([egui::pos2(rect.left(), pt.y), egui::pos2(rect.right(), pt.y)], guide);
            p.line_segment([egui::pos2(pt.x, rect.top()), egui::pos2(pt.x, rect.bottom())], guide);
            p.circle_filled(pt, 7.0, c);
            p.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, &w.label, font, Color32::WHITE);
        }
        WidgetKind::Label => {
            if edit {
                p.rect_filled(rect, 4.0, Color32::from_gray(26));
            }
            p.text(rect.center(), egui::Align2::CENTER_CENTER, &w.label, egui::FontId::proportional(14.0), c);
        }
    }
}

/// Edit mode side panel for the selected widget. Returns false once it's deleted.
fn widget_editor(ui: &mut egui::Ui, st: &mut State, id: u64, targets: &[(ControlTarget, String)]) -> bool {
    let mut delete = false;
    let mut duplicate = false;
    let Some(w) = st.comp.surface.widget_mut(id) else { return false };
    ui.horizontal(|ui| {
        ui.label(RichText::new(w.kind.name()).strong().color(ACCENT));
        ui.add(egui::TextEdit::singleline(&mut w.label).desired_width(170.0));
    });
    ui.horizontal(|ui| {
        choice_combo(ui, ("widget_kind", id), &mut w.kind, 90.0, false);
        for c in PALETTE {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::click());
            ui.painter().circle_filled(r.center(), 7.0, rgb(c));
            if w.color == c {
                ui.painter().circle_stroke(r.center(), 8.5, Stroke::new(1.5, Color32::WHITE));
            }
            if resp.clicked() {
                w.color = c;
            }
        }
    });
    if w.kind != WidgetKind::Label {
        ui.add_space(4.0);
        let axis = if w.kind == WidgetKind::XyPad { "Horizontal (X) drives" } else { "Drives" };
        ui.label(RichText::new(axis).strong());
        hint(ui, "Each target gets this widget's value, rescaled to its from–to range.");
        binding_list(ui, ("bindings", id), targets, &mut w.bindings);
        if w.kind == WidgetKind::XyPad {
            ui.add_space(4.0);
            ui.label(RichText::new("Vertical (Y) drives").strong());
            binding_list(ui, ("bindings_y", id), targets, &mut w.bindings_y);
        }
        ui.add_space(4.0);
        hint(ui, format!("OSC: {}", crate::control::default_osc_address(&ControlTarget::Surface(id))));
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        duplicate = ui.button(label(icon::COPY, "Duplicate")).clicked();
        delete = ui.button(label(icon::TRASH, "Delete")).clicked();
    });
    if duplicate {
        let mut copy = st.comp.surface.widget(id).cloned().unwrap_or_default();
        let (x, y) = st.comp.surface.free_spot(copy.w, copy.h);
        copy.id = st.comp.surface.next_id;
        st.comp.surface.next_id += 1;
        (copy.x, copy.y) = (x, y);
        st.comp.surface.widgets.push(copy);
    }
    if delete {
        st.comp.surface.widgets.retain(|w| w.id != id);
        // Mappings that pointed at it go too.
        st.comp.control_maps.retain(|m| !matches!(m.target, ControlTarget::Surface(i) | ControlTarget::SurfaceY(i) if i == id));
        return false;
    }
    true
}
