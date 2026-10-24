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
        toggle(ui, &mut edit, label(icon::PENCIL_SIMPLE, "Edit")).on_hover_text("Edit: add, move (drag), resize (drag the corner) and wire up widgets. Off: play them.");
        if edit {
            ui.menu_button(label(icon::PLUS, "Add"), |ui| {
                for &kind in <WidgetKind as phosphor_engine::Choice>::OPTIONS {
                    if crate::ui::command(ui, phosphor_engine::Choice::label(kind)).clicked() {
                        selected = Some(st.comp.surface.add(kind));
                        ui.close();
                    }
                }
            });
            ui.label(RichText::new("OSC out").small().color(Color32::GRAY));
            ui.add(egui::TextEdit::singleline(&mut st.comp.surface.osc_host).hint_text("IP, e.g. 192.168.1.20").desired_width(110.0))
                .on_hover_text("Widgets with a Send OSC address send their values here (another app on this Mac: 127.0.0.1). Empty = off.");
            guarded!(ui, st.comp.surface.osc_port, egui::DragValue::new(&mut st.comp.surface.osc_port).range(1..=65535)).on_hover_text("Port the other app listens on");
            ui.label(RichText::new("Columns").small().color(Color32::GRAY));
            guarded!(ui, st.comp.surface.columns, egui::DragValue::new(&mut st.comp.surface.columns).range(4..=32));
            hint(ui, "Drag to move · corner to resize · right-click to map");
        } else if st.comp.surface.on_page().next().is_none() {
            hint(ui, "Empty. Turn on Edit and add buttons, faders and pads.");
        }
        let State { surface_presets, surface_preset_name, comp, .. } = &mut *st;
        let picked = crate::ui::saves_menu(ui, &label(icon::FLOPPY_DISK, "Saved"), "Control Surface pages by name: reuse them in any project (Load replaces this project's surface; Undo brings it back).", surface_presets, surface_preset_name, "surface", || serde_json::to_string(&comp.surface).unwrap_or_default());
        match picked {
            Some(crate::ui::SaveAction::Load(json)) => match serde_json::from_str::<phosphor_engine::ControlSurface>(&json) {
                Ok(sf) => st.comp.surface = sf,
                Err(e) => st.notice = Some(format!("Couldn't load that surface: {e}")),
            },
            Some(crate::ui::SaveAction::Error(e)) => st.notice = Some(e),
            None => {}
        }
    });

    page_tabs(ui, &mut st.comp.surface, edit);

    let targets = crate::control::all_targets(&st.comp);
    let inspector = edit && selected.is_some_and(|id| st.comp.surface.widget(id).is_some());
    let mut sets: Vec<(ControlTarget, f32)> = Vec::new();
    let mut grab: Option<Grab> = ui.ctx().data(|d| d.get_temp(grab_id)).flatten();
    let mut open_editor: Option<u64> = None;
    // A colour widget's brightness strip was dragged: (widget, brightness).
    let mut brightness: Option<(u64, f32)> = None;

    ui.horizontal_top(|ui| {
        let canvas_w = (ui.available_width() - if inspector { 330.0 } else { 0.0 }).max(200.0);
        let cols = st.comp.surface.columns.max(1) as f32;
        let used_rows = st.comp.surface.on_page().map(|w| w.y + w.h).fold(0.0f32, f32::max);
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
            let mut text_sets: Vec<(u64, String)> = Vec::new();
            let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
            for w in st.comp.surface.on_page() {
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
                    let outline = if selected == Some(w.id) { Stroke::new(2.0, accent()) } else { Stroke::new(1.0, Color32::from_gray(90)) };
                    painter.rect_stroke(rect, 6.0, outline, StrokeKind::Outside);
                    painter.text(handle.center(), egui::Align2::CENTER_CENTER, icon::ARROWS_OUT_SIMPLE, egui::FontId::proportional(11.0), Color32::from_gray(160));
                    let wired = !w.bindings.is_empty() || !w.bindings_y.is_empty() || w.bars.iter().any(|b| !b.bindings.is_empty());
                    if !wired && w.kind != WidgetKind::Label {
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
                    // Draw across the bars: each one under the pointer takes its height.
                    WidgetKind::Multi => {
                        if (resp.dragged_by(egui::PointerButton::Primary) || resp.clicked())
                            && let Some(p) = pointer
                            && !w.bars.is_empty()
                        {
                            let (x, y) = pos(p);
                            sets.push((ControlTarget::SurfaceBar { widget: w.id, bar: w.bar_at(x) as u8 }, y));
                        }
                    }
                    WidgetKind::Color => {
                        if (resp.dragged_by(egui::PointerButton::Primary) || resp.clicked())
                            && let Some(p) = pointer
                        {
                            let (x, _) = pos(p);
                            let strip = color_strip(rect);
                            if strip.contains(resp.interact_pointer_pos().unwrap_or(p)) {
                                brightness = Some((w.id, x));
                            } else {
                                // Saturation measured over the square above the strip.
                                let sq = color_square(rect);
                                let sat = (1.0 - (p.y - sq.top()) / sq.height()).clamp(0.0, 1.0);
                                sets.push((target.clone(), x));
                                sets.push((ControlTarget::SurfaceY(w.id), sat));
                            }
                        }
                    }
                    WidgetKind::Choices => {
                        if resp.clicked()
                            && let Some(p) = pointer
                        {
                            let n = w.choice_count();
                            let (x, _) = pos(p);
                            sets.push((target.clone(), w.choice_value(((x * n as f32) as usize).min(n - 1))));
                        }
                    }
                    WidgetKind::Label | WidgetKind::Computed => {}
                    // Typed words go on the text layer when Enter is pressed (or the field is left).
                    WidgetKind::Text => {
                        let key = id.with("draft");
                        let mut draft: String = ui.data_mut(|d| d.get_temp(key)).unwrap_or_else(|| w.text.clone());
                        let field = ui.put(rect.shrink(4.0), egui::TextEdit::singleline(&mut draft).hint_text(if w.label.is_empty() { "type, then Enter" } else { &w.label }));
                        if field.lost_focus() && draft != w.text {
                            text_sets.push((w.id, draft.clone()));
                        }
                        ui.data_mut(|d| d.insert_temp(key, draft));
                        continue;
                    }
                }
                draw_widget(&painter, w, rect, false);
                if w.kind == WidgetKind::Label {
                    continue;
                }
                mapped_badge(ui, &resp, &target);
                let is_xy = matches!(w.kind, WidgetKind::XyPad | WidgetKind::Color);
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
                    if crate::ui::command(ui, label(icon::PENCIL_SIMPLE, "Edit widget…")).clicked() {
                        open_editor = Some(wid);
                        ui.close();
                    }
                });
            }
            for (wid, text) in text_sets {
                st.comp.set_surface_text(wid, &text);
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
    if let Some((id, b)) = brightness
        && let Some(w) = st.comp.surface.widget_mut(id)
    {
        w.value_z = b;
        // Re-send the colour at the new brightness.
        sets.push((ControlTarget::Surface(id), w.value));
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
    let mut s = if w.bindings.is_empty() { format!("Not wired to anything yet (Edit {} select it)", icon::ARROW_RIGHT) } else { format!("Drives: {}", names(&w.bindings)) };
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
            let text = if on { Color32::BLACK } else { p.ctx().global_style().visuals.text_color() };
            p.text(rect.center(), egui::Align2::CENTER_CENTER, &w.label, font, text);
        }
        WidgetKind::Fader | WidgetKind::Slider => {
            p.rect_filled(rect, 6.0, p.ctx().global_style().visuals.widgets.inactive.weak_bg_fill);
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
            p.rect_filled(rect, 6.0, p.ctx().global_style().visuals.widgets.inactive.weak_bg_fill);
            p.rect_stroke(rect, 6.0, Stroke::new(1.0, c.gamma_multiply(0.5)), StrokeKind::Inside);
            let pt = egui::pos2(rect.left() + rect.width() * w.value, rect.bottom() - rect.height() * w.value_y);
            let guide = Stroke::new(1.0, c.gamma_multiply(0.35));
            p.line_segment([egui::pos2(rect.left(), pt.y), egui::pos2(rect.right(), pt.y)], guide);
            p.line_segment([egui::pos2(pt.x, rect.top()), egui::pos2(pt.x, rect.bottom())], guide);
            p.circle_filled(pt, 7.0, c);
            p.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, &w.label, font, Color32::WHITE);
        }
        WidgetKind::Color => {
            let sq = color_square(rect);
            // Hue across, saturation up (at the widget's brightness).
            let mut mesh = egui::Mesh::default();
            let (cols, rows) = (12, 2);
            let probe = |h: f32, s: f32| {
                let mut t = w.clone();
                (t.value, t.value_y) = (h, s);
                let [r, g, b] = t.rgb();
                Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
            };
            for j in 0..=rows {
                for i in 0..=cols {
                    let (u, v) = (i as f32 / cols as f32, j as f32 / rows as f32);
                    mesh.colored_vertex(egui::pos2(sq.left() + u * sq.width(), sq.bottom() - v * sq.height()), probe(u, v));
                }
            }
            for j in 0..rows {
                for i in 0..cols {
                    let a = j * (cols + 1) + i;
                    mesh.add_triangle(a, a + 1, a + cols + 2);
                    mesh.add_triangle(a, a + cols + 2, a + cols + 1);
                }
            }
            p.add(egui::Shape::mesh(mesh));
            let strip = color_strip(rect);
            let [r, g, b] = { let mut t = w.clone(); t.value_z = 1.0; t.rgb() };
            let full = Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
            let mut m2 = egui::Mesh::default();
            m2.colored_vertex(strip.left_top(), Color32::BLACK);
            m2.colored_vertex(strip.right_top(), full);
            m2.colored_vertex(strip.right_bottom(), full);
            m2.colored_vertex(strip.left_bottom(), Color32::BLACK);
            m2.add_triangle(0, 1, 2);
            m2.add_triangle(0, 2, 3);
            p.add(egui::Shape::mesh(m2));
            let pt = egui::pos2(sq.left() + sq.width() * w.value, sq.bottom() - sq.height() * w.value_y);
            p.circle_stroke(pt, 6.0, Stroke::new(2.0, Color32::WHITE));
            let bx = strip.left() + strip.width() * w.value_z;
            p.line_segment([egui::pos2(bx, strip.top()), egui::pos2(bx, strip.bottom())], Stroke::new(2.0, Color32::WHITE));
            p.rect_stroke(rect, 6.0, Stroke::new(1.0, c.gamma_multiply(0.5)), StrokeKind::Inside);
            p.text(sq.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, &w.label, font, Color32::WHITE);
        }
        WidgetKind::Choices => {
            let n = w.choice_count();
            let picked = w.picked();
            let seg = rect.width() / n as f32;
            for i in 0..n {
                let r = egui::Rect::from_min_size(egui::pos2(rect.left() + i as f32 * seg, rect.top()), egui::vec2(seg, rect.height())).shrink(1.5);
                let on = i == picked;
                p.rect_filled(r, 5.0, if on { c } else { c.gamma_multiply(0.15) });
                let name = w.items.get(i).cloned().unwrap_or_else(|| (i + 1).to_string());
                p.text(r.center(), egui::Align2::CENTER_CENTER, name, font.clone(), if on { Color32::BLACK } else { p.ctx().global_style().visuals.text_color() });
            }
        }
        WidgetKind::Computed => {
            p.rect_filled(rect, 6.0, p.ctx().global_style().visuals.widgets.inactive.weak_bg_fill);
            let mut fill = rect;
            fill.set_right(rect.left() + rect.width() * w.value);
            p.rect_filled(fill, 6.0, c.gamma_multiply(0.35));
            p.rect_stroke(rect, 6.0, Stroke::new(1.0, c.gamma_multiply(0.5)), StrokeKind::Inside);
            let shown = if w.formula.trim().is_empty() { "(no formula)" } else { w.formula.trim() };
            p.text(rect.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, format!("{} = {shown}", w.label), font.clone(), Color32::WHITE);
            p.text(rect.right_center() - egui::vec2(6.0, 0.0), egui::Align2::RIGHT_CENTER, format!("{:.0}%", w.value * 100.0), egui::FontId::monospace(10.0), Color32::from_gray(220));
        }
        WidgetKind::Multi => {
            p.rect_filled(rect, 6.0, p.ctx().global_style().visuals.widgets.inactive.weak_bg_fill);
            let n = w.bars.len().max(1);
            let seg = rect.width() / n as f32;
            for (i, b) in w.bars.iter().enumerate() {
                let col = egui::Rect::from_min_size(egui::pos2(rect.left() + i as f32 * seg, rect.top()), egui::vec2(seg, rect.height())).shrink2(egui::vec2(1.5, 2.0));
                let mut fill = col;
                fill.set_top(col.bottom() - col.height() * b.value);
                p.rect_filled(fill, 2.0, if b.bindings.is_empty() { c.gamma_multiply(0.4) } else { c.gamma_multiply(0.85) });
            }
            p.rect_stroke(rect, 6.0, Stroke::new(1.0, c.gamma_multiply(0.5)), StrokeKind::Inside);
            p.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, &w.label, font, Color32::WHITE);
        }
        WidgetKind::Text => {
            p.rect_stroke(rect, 4.0, Stroke::new(1.0, c), StrokeKind::Inside);
            if edit {
                p.text(rect.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, if w.text.is_empty() { &w.label } else { &w.text }, font, c);
            }
        }
        WidgetKind::Label => {
            if edit {
                p.rect_filled(rect, 4.0, p.ctx().global_style().visuals.widgets.inactive.weak_bg_fill);
            }
            p.text(rect.center(), egui::Align2::CENTER_CENTER, &w.label, egui::FontId::proportional(14.0), c);
        }
    }
}

/// The widget the Control Surface has selected (edit mode), if any.
pub(crate) fn selected_widget(ctx: &egui::Context) -> Option<u64> {
    ctx.data(|d| d.get_temp::<Option<u64>>(egui::Id::new("surface_selected"))).flatten()
}

/// Edit mode side panel for the selected widget (also the Inspector). Returns false once it's deleted.
pub(crate) fn widget_editor(ui: &mut egui::Ui, st: &mut State, id: u64, targets: &[(ControlTarget, String)]) -> bool {
    let mut delete = false;
    let mut duplicate = false;
    let text_layers: Vec<(u64, String)> = st.comp.layers.iter().filter(|l| matches!(l.source, phosphor_engine::SourceSpec::Text(_))).map(|l| (l.id, l.name.clone())).collect();
    let Some(w) = st.comp.surface.widget_mut(id) else { return false };
    ui.horizontal(|ui| {
        ui.label(RichText::new(w.kind.name()).strong().color(accent()));
        ui.add(egui::TextEdit::singleline(&mut w.label).desired_width(170.0));
    });
    ui.horizontal(|ui| {
        choice_combo(ui, ("widget_kind", id), &mut w.kind, 90.0, false);
        if w.kind == WidgetKind::Multi && w.bars.is_empty() {
            w.set_bar_count(phosphor_engine::DEFAULT_BARS);
        }
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
    if w.kind == WidgetKind::Text {
        ui.add_space(4.0);
        crate::ui::heading(ui, "Writes on");
        let current = w.text_layer.and_then(|t| text_layers.iter().find(|(id, _)| *id == t)).map_or("(pick a text layer)".to_string(), |(_, n)| n.clone());
        ComboBox::from_id_salt(("text_layer", id)).width(180.0).selected_text(current).show_ui(ui, |ui| {
            for (lid, name) in &text_layers {
                crate::ui::pick_value(ui, &mut w.text_layer, Some(*lid), name);
            }
        });
        if text_layers.is_empty() {
            hint(ui, "Add a text layer first (a layer's source › Text).");
        }
    }
    if !matches!(w.kind, WidgetKind::Label | WidgetKind::Text) {
        ui.add_space(4.0);
        let axis = match w.kind {
            WidgetKind::XyPad => "Horizontal (X) drives",
            WidgetKind::Color => "Red drives",
            _ => "Drives",
        };
        if w.kind == WidgetKind::Computed {
            crate::ui::heading(ui, "Formula");
            crate::ui::text_field(ui, &mut w.formula, "a * b, max(a, b), sin(beat)…", Some(280.0))
                .on_hover_text("a, b, c… = the controls below (0..1); t = seconds, beat = beats, val = its own value. + - * / %, min, max, clamp, mix, pow, sin, cos (a turn per 1.0), abs, floor, fract. The answer is kept in 0..1.");
            let mut remove = None;
            for (i, t) in w.inputs.iter_mut().enumerate() {
                crate::ui::toolbar(ui, |ui| {
                    ui.label(RichText::new(((b'a' + i as u8) as char).to_string()).strong().monospace());
                    if let Some(picked) = target_picker(ui, ("computed_in", id, i), targets, Some(t), 220.0) {
                        *t = picked;
                    }
                    if crate::ui::small_command(ui, icon::X).on_hover_text("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                w.inputs.remove(i);
            }
            if w.inputs.len() < 8 {
                crate::ui::toolbar(ui, |ui| {
                    ui.label(RichText::new(((b'a' + w.inputs.len() as u8) as char).to_string()).monospace().color(Color32::GRAY));
                    if let Some(picked) = target_picker(ui, ("computed_new", id), targets, None, 220.0) {
                        w.inputs.push(picked);
                    }
                });
            }
            ui.add_space(4.0);
        }
        if w.kind == WidgetKind::Multi {
            crate::ui::toolbar(ui, |ui| {
                ui.label("Bars");
                let mut n = w.bars.len();
                if guarded!(ui, n, egui::DragValue::new(&mut n).range(1..=64)).changed() {
                    w.set_bar_count(n);
                }
            });
            hint(ui, "Each bar drives its own targets. Map a controller to the widget to move every bar at once, or to one bar (Surface › bar N in any target list).");
            for (i, b) in w.bars.iter_mut().enumerate() {
                let title = format!("Bar {} drives{}", i + 1, if b.bindings.is_empty() { "" } else { " ·" });
                crate::ui::section(ui, ("bar", id, i), title, i == 0, |ui| {
                    binding_list(ui, ("bar_bindings", id, i), targets, &mut b.bindings);
                });
            }
        } else {
            crate::ui::heading(ui, axis);
            hint(ui, "Each target gets this widget's value, rescaled to its from–to range.");
            binding_list(ui, ("bindings", id), targets, &mut w.bindings);
        }
        if w.kind == WidgetKind::Color {
            ui.add_space(4.0);
            crate::ui::heading(ui, "Green drives");
            binding_list(ui, ("bindings_y", id), targets, &mut w.bindings_y);
            ui.add_space(4.0);
            crate::ui::heading(ui, "Blue drives");
            binding_list(ui, ("bindings_z", id), targets, &mut w.bindings_z);
            hint(ui, "A controller on this widget moves the hue (its Y mapping: saturation).");
        }
        if w.kind == WidgetKind::XyPad {
            ui.add_space(4.0);
            crate::ui::heading(ui, "Vertical (Y) drives");
            binding_list(ui, ("bindings_y", id), targets, &mut w.bindings_y);
        }
        if w.kind == WidgetKind::Choices {
            ui.add_space(4.0);
            let key = ui.id().with(("choice_items", id));
            let mut text = ui.data_mut(|d| d.get_temp::<String>(key)).unwrap_or_else(|| w.items.join(", "));
            if crate::ui::text_field(ui, &mut text, "Names, with commas: Normal, Add, Screen", None)
                .on_hover_text("One button each. For a pick-one control (blend mode, direction…) give it the same number of choices it has.")
                .changed()
            {
                w.items = text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
            ui.data_mut(|d| d.insert_temp(key, text));
        }
        ui.add_space(4.0);
        hint(ui, format!("OSC in: {}", crate::control::default_osc_address(&ControlTarget::Surface(id))));
        ui.horizontal(|ui| {
            ui.label("Send OSC");
            ui.add(egui::TextEdit::singleline(&mut w.osc_out).hint_text("/address (to the OSC out app)").desired_width(170.0))
                .on_hover_text("Also send this widget's value to another app (TouchDesigner, lights, Resolume…) whenever it moves, whoever moves it. Set where it goes with OSC out at the top of the panel.");
        });
        // Audience play: whether guests get it, and how far they can push it.
        ui.horizontal(|ui| {
            toggle(ui, &mut w.guests, "Guests can use it").on_hover_text("Audience play: off hides it from guests' phones (only you and remotes use it)");
            if w.guests && matches!(w.kind, WidgetKind::Fader | WidgetKind::Slider | WidgetKind::XyPad) {
                let [mut lo, mut hi] = w.guest_range;
                Level::percent(&mut lo, "Guests from").default(0.0).width(110.0).show(ui);
                Level::percent(&mut hi, "up to").default(1.0).width(110.0).show(ui);
                w.guest_range = [lo.min(hi), hi.max(lo)];
            }
        });
    }
    ui.add_space(6.0);
    crate::ui::toolbar(ui, |ui| {
        duplicate = crate::ui::command(ui, label(icon::COPY, "Duplicate")).clicked();
        delete = crate::ui::command(ui, label(icon::TRASH, "Delete")).clicked();
    });
    if duplicate {
        st.comp.surface.duplicate(id);
    }
    if delete {
        st.comp.surface.widgets.retain(|w| w.id != id);
        // Mappings that pointed at it go too.
        st.comp.control_maps.retain(|m| !matches!(m.target, ControlTarget::Surface(i) | ControlTarget::SurfaceY(i) | ControlTarget::SurfaceBar { widget: i, .. } if i == id));
        return false;
    }
    true
}

/// Page tabs above the surface: click to switch; in Edit, add pages and right-click one to rename or delete it.
fn page_tabs(ui: &mut egui::Ui, sf: &mut phosphor_engine::ControlSurface, edit: bool) {
    if sf.pages.is_empty() {
        sf.pages.push("Main".into());
    }
    if sf.pages.len() < 2 && !edit {
        return;
    }
    let mut remove = None;
    ui.horizontal_wrapped(|ui| {
        let current = sf.current_page();
        for i in 0..sf.pages.len() {
            let r = crate::ui::pick_item(ui, current == i, &sf.pages[i]);
            map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::SurfacePage));
            if r.clicked() {
                sf.page = i;
            }
            if edit {
                r.right_menu(|ui| {
                    ui.label(RichText::new("Page name").small());
                    ui.text_edit_singleline(&mut sf.pages[i]);
                    if sf.pages.len() > 1 && crate::ui::command(ui, label(icon::TRASH, "Delete page and its widgets")).clicked() {
                        remove = Some(i);
                        ui.close();
                    }
                });
            }
        }
        if edit && crate::ui::small_command(ui, label(icon::PLUS, "Page")).on_hover_text("Add a page (right-click a page to rename or delete it)").clicked() {
            sf.add_page();
        }
    });
    if let Some(i) = remove {
        sf.remove_page(i);
    }
}

/// A colour widget's hue / saturation square (above the brightness strip).
fn color_square(rect: egui::Rect) -> egui::Rect {
    let mut r = rect.shrink(3.0);
    r.set_bottom(r.bottom() - (rect.height() * 0.16).clamp(10.0, 22.0) - 3.0);
    r
}

/// A colour widget's brightness strip along the bottom.
fn color_strip(rect: egui::Rect) -> egui::Rect {
    let r = rect.shrink(3.0);
    egui::Rect::from_min_max(egui::pos2(r.left(), r.bottom() - (rect.height() * 0.16).clamp(10.0, 22.0)), r.max)
}
