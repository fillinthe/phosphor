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

/// What the designer was asked to do to a widget (right-click menu, keys), done once the canvas is drawn.
#[derive(Clone, Copy)]
enum Act {
    Duplicate(u64),
    Copy(u64),
    ToFront(u64),
    ToBack(u64),
    Delete(u64),
    /// Nudge by (dx, dy) cells.
    Move(u64, f32, f32),
    /// Add a widget at a spot (cells).
    Add(WidgetKind, f32, f32),
    /// Paste the copied widget at a spot.
    Paste(f32, f32),
}

/// Temp-data key: the copied widget (Copy / Paste in the designer, across projects too).
const CLIPBOARD: &str = "surface_clipboard";
/// Temp-data key: the pass the designer last owned the keyboard in (pointer over its canvas in Edit).
const KEYS_PASS: &str = "surface_keys_pass";

/// True while the Control Surface designer uses the keys itself (Delete, arrows, Cmd+D/C/V): the app's layer
/// shortcuts stand back. Set a frame earlier (the designer draws after the shortcuts run).
pub(crate) fn designer_owns_keys(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<u64>(egui::Id::new(KEYS_PASS))).is_some_and(|pass| ctx.cumulative_pass_nr() <= pass + 1)
}

/// Where a drag or a click lands: a whole cell with Snap on, a quarter cell without.
fn snapped(cells: f32, snap: bool) -> f32 {
    let step = if snap { 1.0 } else { 0.25 };
    (cells / step).round() * step
}

pub(crate) fn surface(ui: &mut egui::Ui, st: &mut State) {
    surface_page(ui, st, None);
}

/// One page of the surface as a panel of its own (View › New Panel): your own buttons and sliders, wired to
/// anything and to OSC, docked or floating anywhere.
pub(crate) fn my_panel(ui: &mut egui::Ui, st: &mut State, page: usize) {
    if page >= st.comp.surface.pages.len() {
        help(ui, "This panel was deleted: close it.");
        return;
    }
    // The page showing is this one while the panel draws (adding, duplicating and placing all work on it).
    let showing = std::mem::replace(&mut st.comp.surface.page, page);
    surface_page(ui, st, Some(page));
    st.comp.surface.page = showing;
}

/// The surface (`only` = None: every page, with page tabs) or one page of it.
fn surface_page(ui: &mut egui::Ui, st: &mut State, only: Option<usize>) {
    let now = ui.ctx().input(|i| i.time);
    let key = only.map_or(u64::MAX, |p| p as u64);
    let edit_id = egui::Id::new(("surface_edit", key));
    let sel_id = egui::Id::new(("surface_selected", key));
    let grab_id = egui::Id::new(("surface_grab", key));
    let snap_id = egui::Id::new("surface_snap");
    let mut edit: bool = ui.ctx().data(|d| d.get_temp(edit_id)).unwrap_or(st.comp.surface.widgets.is_empty());
    let mut snap: bool = ui.ctx().data(|d| d.get_temp(snap_id)).unwrap_or(true);
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
                if !st.comp.lasers.projectors.is_empty() && !st.comp.lasers.looks.is_empty() {
                    ui.separator();
                    if crate::ui::command(ui, label(icon::PROJECTOR_SCREEN, "Laser look buttons")).on_hover_text("A button per laser look (per laser), lit while it shows").clicked() {
                        add_laser_buttons(&mut st.comp);
                        ui.close();
                    }
                }
            });
            toggle(ui, &mut snap, label(icon::MAGNET, "Snap")).on_hover_text("On: widgets land on whole grid cells. Off: anywhere, to a quarter of a cell (arrow keys nudge by the same amount; Shift = a whole cell).");
            ui.label(RichText::new("OSC out").small().color(Color32::GRAY));
            ui.add(egui::TextEdit::singleline(&mut st.comp.surface.osc_host).hint_text("IP, e.g. 192.168.1.20").desired_width(110.0))
                .on_hover_text("Widgets with a Send OSC address send their values here (another app on this Mac: 127.0.0.1). Empty = off.");
            guarded!(ui, st.comp.surface.osc_port, egui::DragValue::new(&mut st.comp.surface.osc_port).range(1..=65535)).on_hover_text("Port the other app listens on");
            ui.label(RichText::new("Columns").small().color(Color32::GRAY));
            guarded!(ui, st.comp.surface.columns, egui::DragValue::new(&mut st.comp.surface.columns).range(4..=32));
            help(ui, "Drag to move · corner to resize · right-click a widget or an empty spot for more · Delete, arrows, Cmd+D / C / V work on the selected one");
        } else if st.comp.surface.on_page().next().is_none() {
            crate::ui::empty_note(ui, "Empty. Turn on Edit and add buttons, faders and pads.");
        }
        if only.is_some() {
            return;
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

    if only.is_none() {
        page_tabs(ui, &mut st.comp.surface, edit);
    }

    let targets = crate::control::all_targets(&st.comp);
    let inspector = edit && selected.is_some_and(|id| st.comp.surface.widget(id).is_some());
    let mut sets: Vec<(ControlTarget, f32)> = Vec::new();
    let mut grab: Option<Grab> = ui.ctx().data(|d| d.get_temp(grab_id)).flatten();
    let mut open_editor: Option<u64> = None;
    // A colour widget's brightness strip was dragged: (widget, brightness).
    let mut brightness: Option<(u64, f32)> = None;
    let mut act: Option<Act> = None;
    let copied: Option<SurfaceWidget> = ui.ctx().data(|d| d.get_temp(egui::Id::new(CLIPBOARD)));

    ui.horizontal_top(|ui| {
        let canvas_w = (ui.available_width() - if inspector { 330.0 } else { 0.0 }).max(200.0);
        let cols = st.comp.surface.columns.max(1) as f32;
        let used_rows = st.comp.surface.on_page().map(|w| w.y + w.h).fold(0.0f32, f32::max);
        let rows = used_rows + if edit { 2.0 } else { 0.0 };
        // Square cells that fit the whole surface in the panel (across and down); scroll only when tiny.
        let avail_h = ui.available_height() - 4.0;
        let cell = (canvas_w / cols).min(avail_h / rows.max(4.0)).max(24.0);
        let height = (rows * cell).max(avail_h);
        crate::ui::column_scroll(ui).id_salt(("surface_scroll", key)).max_width(canvas_w).auto_shrink([false, false]).show(ui, |ui| {
            let (canvas, bg) = ui.allocate_exact_size(egui::vec2(canvas_w, height), if edit { Sense::click() } else { Sense::hover() });
            let painter = ui.painter_at(canvas);
            // Cells under a point on the canvas (where a right-click or a paste lands).
            let cell_at = |p: egui::Pos2| (snapped((p.x - canvas.min.x) / cell, snap), snapped((p.y - canvas.min.y) / cell, snap));
            let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
            if edit {
                if bg.clicked() {
                    selected = None;
                }
                // Right-click an empty spot: add something right there.
                let spot_id = egui::Id::new(("surface_spot", key));
                if bg.secondary_clicked() && let Some(p) = pointer {
                    ui.ctx().data_mut(|d| d.insert_temp(spot_id, cell_at(p)));
                }
                bg.right_menu(|ui| {
                    let (x, y) = ui.ctx().data(|d| d.get_temp::<(f32, f32)>(spot_id)).unwrap_or((0.0, 0.0));
                    ui.label(RichText::new("Add here").small().color(Color32::GRAY));
                    for &kind in <WidgetKind as phosphor_engine::Choice>::OPTIONS {
                        if crate::ui::command(ui, phosphor_engine::Choice::label(kind)).clicked() {
                            act = Some(Act::Add(kind, x, y));
                            ui.close();
                        }
                    }
                    if !st.comp.lasers.projectors.is_empty() && !st.comp.lasers.looks.is_empty() {
                        ui.separator();
                        if crate::ui::command(ui, label(icon::PROJECTOR_SCREEN, "Laser look buttons")).on_hover_text("A button per laser look (per laser), lit while it shows").clicked() {
                            add_laser_buttons(&mut st.comp);
                            ui.close();
                        }
                    }
                    if let Some(c) = &copied {
                        ui.separator();
                        if crate::ui::command(ui, label(icon::CLIPBOARD_TEXT, &format!("Paste {}", c.label))).clicked() {
                            act = Some(Act::Paste(x, y));
                            ui.close();
                        }
                    }
                });
                // The keys are the designer's while the pointer is over it (the app's layer shortcuts stand back).
                if ui.rect_contains_pointer(canvas) {
                    let pass = ui.ctx().cumulative_pass_nr();
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(KEYS_PASS), pass));
                    if !ui.ctx().egui_wants_keyboard_input() {
                        act = act.or(designer_keys(ui, selected, copied.is_some(), snap, pointer.map(cell_at)));
                    }
                }
            }
            if edit {
                for gx in 0..=cols as i32 {
                    for gy in 0..=(height / cell) as i32 {
                        painter.circle_filled(canvas.min + egui::vec2(gx as f32 * cell, gy as f32 * cell), 1.0, Color32::from_gray(55));
                    }
                }
            }
            if edit && st.comp.surface.on_page().next().is_none() {
                painter.text(canvas.min + egui::vec2(cell * 0.5, cell * 0.5), egui::Align2::LEFT_TOP, "Right-click anywhere here to add a button, fader or pad (or use Add).", egui::FontId::proportional(13.0), Color32::from_gray(130));
            }
            let mut text_sets: Vec<(u64, String)> = Vec::new();
            for w in st.comp.surface.on_page() {
                let rect = egui::Rect::from_min_size(canvas.min + egui::vec2(w.x * cell, w.y * cell), egui::vec2(w.w * cell, w.h * cell)).shrink(3.0);
                let id = egui::Id::new(("surface_widget", w.id));
                let target = ControlTarget::Surface(w.id);
                if edit {
                    let body = ui.interact(rect, id, Sense::click_and_drag());
                    let handle = egui::Rect::from_min_size(rect.max - egui::vec2(14.0, 14.0), egui::vec2(14.0, 14.0));
                    let corner = ui.interact(handle, id.with("resize"), Sense::drag());
                    if body.clicked() || body.drag_started() || body.secondary_clicked() {
                        selected = Some(w.id);
                    }
                    let wid = w.id;
                    body.right_menu(|ui| {
                        if crate::ui::command(ui, label(icon::PENCIL_SIMPLE, "Edit widget…")).clicked() {
                            open_editor = Some(wid);
                            ui.close();
                        }
                        if crate::ui::command(ui, label(icon::COPY, "Duplicate")).clicked() {
                            act = Some(Act::Duplicate(wid));
                            ui.close();
                        }
                        if crate::ui::command(ui, label(icon::CLIPBOARD_TEXT, "Copy")).on_hover_text("Paste it on another page or in another project (right-click an empty spot)").clicked() {
                            act = Some(Act::Copy(wid));
                            ui.close();
                        }
                        ui.separator();
                        if crate::ui::command(ui, label(icon::ARROW_LINE_UP, "Bring to front")).clicked() {
                            act = Some(Act::ToFront(wid));
                            ui.close();
                        }
                        if crate::ui::command(ui, label(icon::ARROW_LINE_DOWN, "Send to back")).clicked() {
                            act = Some(Act::ToBack(wid));
                            ui.close();
                        }
                        ui.separator();
                        if crate::ui::danger_button(ui, label(icon::TRASH, "Delete")).clicked() {
                            act = Some(Act::Delete(wid));
                            ui.close();
                        }
                    });
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
                                w.x = (x + snapped((p.x - start.x) / cell, snap)).clamp(0.0, (cols - w.w).max(0.0));
                                w.y = (y + snapped((p.y - start.y) / cell, snap)).max(0.0);
                            }
                        }
                        Grab::Resize { id, start, w: w0, h: h0 } => {
                            if let Some(w) = surface.widget_mut(id) {
                                w.w = (w0 + snapped((p.x - start.x) / cell, snap)).clamp(1.0, (cols - w.x).max(1.0));
                                w.h = (h0 + snapped((p.y - start.y) / cell, snap)).max(1.0);
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
    match act {
        Some(Act::Duplicate(id)) => selected = st.comp.surface.duplicate(id).or(selected),
        Some(Act::Copy(id)) => {
            if let Some(w) = st.comp.surface.widget(id).cloned() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(CLIPBOARD), w));
                st.notice = Some("Copied: right-click an empty spot to paste it".into());
            }
        }
        Some(Act::ToFront(id)) => st.comp.surface.to_front(id),
        Some(Act::ToBack(id)) => st.comp.surface.to_back(id),
        Some(Act::Delete(id)) => {
            st.comp.remove_widget(id);
            if selected == Some(id) {
                selected = None;
            }
        }
        Some(Act::Move(id, dx, dy)) => st.comp.surface.move_by(id, dx, dy),
        Some(Act::Add(kind, x, y)) => selected = Some(st.comp.surface.add_at(kind, Some((x, y)))),
        Some(Act::Paste(x, y)) => {
            if let Some(w) = copied {
                selected = Some(st.comp.surface.place(w, Some((x, y))));
            }
        }
        None => {}
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
        d.insert_temp(snap_id, snap);
    });
}

/// Keys in Edit while the pointer is over the canvas: Delete / Backspace removes the selected widget, arrows nudge it
/// (a cell with Snap on, a quarter without; Shift = a whole cell), Cmd+D duplicates, Cmd+C copies, Cmd+V pastes the
/// copied widget under the pointer. The keys are taken so nothing else reads them.
fn designer_keys(ui: &egui::Ui, selected: Option<u64>, copied: bool, snap: bool, under_pointer: Option<(f32, f32)>) -> Option<Act> {
    use egui::{Key, Modifiers};
    ui.ctx().input_mut(|i| {
        if copied && i.consume_key(Modifiers::COMMAND, Key::V) && let Some((x, y)) = under_pointer {
            return Some(Act::Paste(x, y));
        }
        let id = selected?;
        if i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace) {
            return Some(Act::Delete(id));
        }
        if i.consume_key(Modifiers::COMMAND, Key::D) {
            return Some(Act::Duplicate(id));
        }
        if i.consume_key(Modifiers::COMMAND, Key::C) {
            return Some(Act::Copy(id));
        }
        let step = if i.modifiers.shift || snap { 1.0 } else { 0.25 };
        let mut k = |key| (i.consume_key(Modifiers::NONE, key) || i.consume_key(Modifiers::SHIFT, key)) as i32 as f32;
        let dx = (k(Key::ArrowRight) - k(Key::ArrowLeft)) * step;
        let dy = (k(Key::ArrowDown) - k(Key::ArrowUp)) * step;
        ((dx, dy) != (0.0, 0.0)).then_some(Act::Move(id, dx, dy))
    })
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
            if w.motion.is_some() {
                // Where it's moving right now.
                let line = if w.kind == WidgetKind::Fader {
                    let y = rect.bottom() - rect.height() * w.live;
                    [egui::pos2(rect.left() + 3.0, y), egui::pos2(rect.right() - 3.0, y)]
                } else {
                    let x = rect.left() + rect.width() * w.live;
                    [egui::pos2(x, rect.top() + 3.0), egui::pos2(x, rect.bottom() - 3.0)]
                };
                p.line_segment(line, Stroke::new(2.5, Color32::WHITE));
                p.ctx().request_repaint();
            }
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
        crate::ui::dropdown(ui, ("text_layer", id), 180.0).selected_text(current).show_ui(ui, |ui| {
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
            help(ui, "Each bar drives its own targets. Map a controller to the widget to move every bar at once, or to one bar (Surface › bar N in any target list).");
            for (i, b) in w.bars.iter_mut().enumerate() {
                let title = format!("Bar {} drives{}", i + 1, if b.bindings.is_empty() { "" } else { " ·" });
                crate::ui::section(ui, ("bar", id, i), title, i == 0, |ui| {
                    binding_list(ui, ("bar_bindings", id, i), targets, &mut b.bindings);
                });
            }
        } else {
            if matches!(w.kind, WidgetKind::Fader | WidgetKind::Slider) {
                crate::ui::toolbar(ui, |ui| {
                    ui.label("Moves by itself");
                    crate::ui::mod_button(ui, &mut w.motion);
                });
                help(ui, "An LFO, the sound or the beat moves it around where you set it, and its targets follow (the white line shows where it is).");
            }
            crate::ui::heading(ui, axis);
            help(ui, "Each target gets this widget's value, rescaled to its from–to range.");
            binding_list(ui, ("bindings", id), targets, &mut w.bindings);
        }
        if w.kind == WidgetKind::Color {
            ui.add_space(4.0);
            crate::ui::heading(ui, "Green drives");
            binding_list(ui, ("bindings_y", id), targets, &mut w.bindings_y);
            ui.add_space(4.0);
            crate::ui::heading(ui, "Blue drives");
            binding_list(ui, ("bindings_z", id), targets, &mut w.bindings_z);
            help(ui, "A controller on this widget moves the hue (its Y mapping: saturation).");
        }
        if w.kind == WidgetKind::XyPad {
            ui.add_space(4.0);
            crate::ui::heading(ui, "Vertical (Y) drives");
            binding_list(ui, ("bindings_y", id), targets, &mut w.bindings_y);
        }
        if w.kind == WidgetKind::Choices {
            ui.add_space(4.0);
            crate::ui::list_field(ui, ("choice_items", id), &mut w.items, "Names, with commas: Normal, Add, Screen", None, |items| items.join(", "), |text| {
                text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
            })
            .on_hover_text("One button each. For a pick-one control (blend mode, direction…) give it the same number of choices it has.");
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
        st.comp.remove_widget(id);
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
                r.right_field_menu(|ui| {
                    ui.label(RichText::new("Page name").small());
                    ui.text_edit_singleline(&mut sf.pages[i]);
                    if crate::ui::command(ui, label(icon::ARROW_SQUARE_OUT, "Open as its own panel")).on_hover_text("This page as a panel you can dock anywhere or float").clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(OPEN_MY_PANEL), i));
                        ui.close();
                    }
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
        remove_page(ui.ctx(), sf, i);
    }
}

/// Temp-data key: open page N as its own panel (the app has the dock).
pub(crate) const OPEN_MY_PANEL: &str = "phosphor_open_my_panel";
/// Temp-data key: page N was deleted (its panel closes, later panels move down one).
pub(crate) const MY_PANEL_GONE: &str = "phosphor_my_panel_gone";

/// Deletes a page (and its widgets); its panel closes.
pub(crate) fn remove_page(ctx: &egui::Context, sf: &mut phosphor_engine::ControlSurface, i: usize) {
    let before = sf.pages.len();
    sf.remove_page(i);
    if sf.pages.len() < before {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(MY_PANEL_GONE), i));
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

/// A button per laser look for every laser projector, wired to switch to it (`ControlTarget::LaserLook`), in the
/// look's colour (as in the clip grid and the Timeline).
pub(crate) fn add_laser_buttons(comp: &mut phosphor_engine::Composition) {
    let looks: Vec<(u64, String)> = comp.lasers.looks.iter().map(|l| (l.id, l.name.clone())).collect();
    let many = comp.lasers.projectors.len() > 1;
    for p in &comp.lasers.projectors {
        for (look, name) in &looks {
            let id = comp.surface.add(WidgetKind::Button);
            if let Some(w) = comp.surface.widget_mut(id) {
                w.label = if many { format!("{} · {name}", p.name) } else { name.clone() };
                w.color = super::lasers::look_colour(&looks, *look).to_array()[..3].try_into().unwrap_or([0, 200, 190]);
                w.bindings = vec![phosphor_engine::Binding { target: ControlTarget::LaserLook { projector: p.id, look: *look }, range: Default::default() }];
            }
        }
    }
}
