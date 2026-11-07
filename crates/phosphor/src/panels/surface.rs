//! Control Surface: your own buttons, toggles, faders, sliders and XY pads (VDMX-style). Each
//! widget drives any number of targets, and can itself be mapped to MIDI / OSC / keys.

use super::*;
use phosphor_engine::{Arrange, SurfaceWidget, WidgetKind};

const PALETTE: [[u8; 3]; 8] = [[0, 200, 190], [255, 196, 70], [255, 105, 97], [160, 120, 255], [90, 170, 255], [120, 220, 110], [255, 140, 200], [200, 200, 200]];

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

#[derive(Clone)]
enum Grab {
    /// Moving every selected widget together: each one's id and where it started (cells).
    Move { ids: Vec<(u64, f32, f32)>, start: egui::Pos2 },
    Resize { id: u64, start: egui::Pos2, w: f32, h: f32 },
}

/// What the designer was asked to do (right-click menu, keys, Inspector), done once the canvas is drawn. Most act
/// on every selected widget.
#[derive(Clone)]
enum Act {
    Duplicate(Vec<u64>),
    Copy(Vec<u64>),
    ToFront(Vec<u64>),
    ToBack(Vec<u64>),
    Delete(Vec<u64>),
    /// Nudge by (dx, dy) cells, as a group.
    Move(Vec<u64>, f32, f32),
    /// Line up, spread out or match sizes.
    Arrange(Vec<u64>, Arrange),
    /// Lock in place (true) or free (false).
    Lock(Vec<u64>, bool),
    /// Add a widget at a spot (cells).
    Add(WidgetKind, f32, f32),
    /// Paste the copied widgets with their top-left corner at a spot.
    Paste(f32, f32),
    SelectAll,
    SelectNone,
}

/// Temp-data key: the copied widgets (Copy / Paste in the designer, across projects too).
const CLIPBOARD: &str = "surface_clipboard";
/// Temp-data key: the pass the designer last owned the keyboard in (pointer over its canvas in Edit).
const KEYS_PASS: &str = "surface_keys_pass";

/// True while the Control Surface designer uses the keys itself (Delete, arrows, Cmd+A/D/C/V): the app's layer
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
    let multi = crate::ui::Selection::new(("surface_multi", key));
    let ctx = ui.ctx().clone();
    let mut selected: Option<u64> = ctx.data(|d| d.get_temp(sel_id)).flatten();
    let mut edit: bool = ctx.data(|d| d.get_temp(edit_id)).unwrap_or_else(|| {
        // First frame. Bench pictures: PHOSPHOR_SURFACE_EDIT=1,2,3 opens Edit with those widgets selected.
        match std::env::var("PHOSPHOR_SURFACE_EDIT") {
            Ok(ids) if only.is_none() => {
                multi.set(&ctx, &mut selected, ids.split(',').filter_map(|i| i.trim().parse().ok()).collect());
                true
            }
            _ => st.comp.surface.widgets.is_empty(),
        }
    });
    let mut snap: bool = ctx.data(|d| d.get_temp(snap_id)).unwrap_or(true);
    // Everything selected (on this page only; the primary is the one whose editor shows when it's alone).
    let mut picked: Vec<u64> = multi.all(&ctx, selected).into_iter().filter(|id| st.comp.surface.widget(*id).is_some_and(|w| w.page == st.comp.surface.current_page())).collect();
    let mut act: Option<Act> = None;
    let copied: Vec<SurfaceWidget> = ctx.data(|d| d.get_temp(egui::Id::new(CLIPBOARD))).unwrap_or_default();

    // Toolbar.
    ui.horizontal(|ui| {
        toggle(ui, &mut edit, label(icon::PENCIL_SIMPLE, "Edit")).on_hover_text("Edit: add, move (drag), resize (drag the corner) and wire up widgets. Off: play them.");
        if edit {
            ui.menu_button(label(icon::PLUS, "Add"), |ui| {
                for &kind in <WidgetKind as phosphor_engine::Choice>::OPTIONS {
                    if crate::ui::command(ui, phosphor_engine::Choice::label(kind)).clicked() {
                        let id = st.comp.surface.add(kind);
                        multi.set(&ctx, &mut selected, vec![id]);
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
            if picked.len() >= 2 {
                ui.menu_button(label(icon::ALIGN_LEFT, "Arrange"), |ui| {
                    act = act.take().or(arrange_items(ui, &picked, true));
                })
                .response
                .on_hover_text("Line up, spread out or match the size of the selected widgets");
            }
            ui.label(RichText::new("OSC out").small().color(Color32::GRAY));
            ui.add(egui::TextEdit::singleline(&mut st.comp.surface.osc_host).hint_text("IP, e.g. 192.168.1.20").desired_width(110.0))
                .on_hover_text("Widgets with a Send OSC address send their values here (another app on this Mac: 127.0.0.1). Empty = off.");
            guarded!(ui, st.comp.surface.osc_port, egui::DragValue::new(&mut st.comp.surface.osc_port).range(1..=65535)).on_hover_text("Port the other app listens on");
            ui.label(RichText::new("Columns").small().color(Color32::GRAY));
            guarded!(ui, st.comp.surface.columns, egui::DragValue::new(&mut st.comp.surface.columns).range(4..=32));
            help(ui, "Drag to move · corner to resize · Shift-click or drag a band on empty space to select several (they move together) · right-click for more · Delete, arrows, Cmd+A / D / C / V / L (lock)");
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
    let inspector = edit && (picked.len() >= 2 || selected.is_some_and(|id| st.comp.surface.widget(id).is_some()));
    let mut sets: Vec<(ControlTarget, f32)> = Vec::new();
    let mut grab: Option<Grab> = ctx.data(|d| d.get_temp(grab_id)).flatten();
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
        crate::ui::column_scroll(ui).id_salt(("surface_scroll", key)).max_width(canvas_w).auto_shrink([false, false]).show(ui, |ui| {
            let (canvas, bg) = ui.allocate_exact_size(egui::vec2(canvas_w, height), if edit { Sense::click_and_drag() } else { Sense::hover() });
            let painter = ui.painter_at(canvas);
            let widget_rect = |w: &SurfaceWidget| egui::Rect::from_min_size(canvas.min + egui::vec2(w.x * cell, w.y * cell), egui::vec2(w.w * cell, w.h * cell)).shrink(3.0);
            // Cells under a point on the canvas (where a right-click or a paste lands).
            let cell_at = |p: egui::Pos2| (snapped((p.x - canvas.min.x) / cell, snap), snapped((p.y - canvas.min.y) / cell, snap));
            let pointer = ctx.input(|i| i.pointer.interact_pos());
            let extend = crate::ui::extending(ui);
            if edit {
                if bg.clicked() && !extend {
                    multi.clear(&ctx, &mut selected);
                    picked.clear();
                }
                // Right-click an empty spot: add something right there.
                let spot_id = egui::Id::new(("surface_spot", key));
                if bg.secondary_clicked() && let Some(p) = pointer {
                    ctx.data_mut(|d| d.insert_temp(spot_id, cell_at(p)));
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
                    if !copied.is_empty() {
                        ui.separator();
                        if crate::ui::command(ui, label(icon::CLIPBOARD_TEXT, &paste_label(&copied))).clicked() {
                            act = Some(Act::Paste(x, y));
                            ui.close();
                        }
                    }
                    if st.comp.surface.on_page().next().is_some() {
                        ui.separator();
                        if crate::ui::command(ui, label(icon::SELECTION_ALL, "Select all")).clicked() {
                            act = Some(Act::SelectAll);
                            ui.close();
                        }
                    }
                });
                // The keys are the designer's while the pointer is over it (the app's layer shortcuts stand back).
                if ui.rect_contains_pointer(canvas) {
                    let pass = ctx.cumulative_pass_nr();
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new(KEYS_PASS), pass));
                    if !ctx.egui_wants_keyboard_input() {
                        let all_locked = st.comp.surface.widgets.iter().filter(|w| picked.contains(&w.id)).all(|w| w.locked);
                        act = act.take().or(designer_keys(ui, &picked, all_locked, !copied.is_empty(), snap, pointer.map(cell_at)));
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
                let rect = widget_rect(w);
                let id = egui::Id::new(("surface_widget", w.id));
                let target = ControlTarget::Surface(w.id);
                if edit {
                    let body = ui.interact(rect, id, Sense::click_and_drag());
                    let handle = egui::Rect::from_min_size(rect.max - egui::vec2(14.0, 14.0), egui::vec2(14.0, 14.0));
                    let corner = ui.interact(handle, id.with("resize"), if w.locked { Sense::click() } else { Sense::drag() });
                    if body.clicked() {
                        multi.click(&ctx, &mut selected, w.id, extend);
                        picked = multi.all(&ctx, selected);
                    }
                    // A drag or a right-click on one outside the selection: that one alone.
                    if (body.drag_started() || body.secondary_clicked() || corner.drag_started()) && !picked.contains(&w.id) {
                        multi.click(&ctx, &mut selected, w.id, extend && body.drag_started());
                        picked = multi.all(&ctx, selected);
                    }
                    let wid = w.id;
                    let ids: Vec<u64> = if picked.contains(&wid) { picked.clone() } else { vec![wid] };
                    let all_locked = st.comp.surface.widgets.iter().filter(|o| ids.contains(&o.id)).all(|o| o.locked);
                    body.right_menu(|ui| {
                        if ids.len() == 1 {
                            if crate::ui::command(ui, label(icon::PENCIL_SIMPLE, "Edit widget…")).clicked() {
                                open_editor = Some(wid);
                                ui.close();
                            }
                        } else {
                            ui.label(RichText::new(format!("{} selected", ids.len())).small().color(Color32::GRAY));
                            ui.menu_button(label(icon::ALIGN_LEFT, "Arrange"), |ui| {
                                act = act.take().or(arrange_items(ui, &ids, true));
                            });
                        }
                        act = act.take().or(group_items(ui, &ids, all_locked, true));
                    });
                    if let Some(p) = pointer {
                        if body.drag_started() && !w.locked {
                            let ids = st.comp.surface.on_page().filter(|o| picked.contains(&o.id) && !o.locked).map(|o| (o.id, o.x, o.y)).collect();
                            grab = Some(Grab::Move { ids, start: p });
                        }
                        if corner.drag_started() {
                            grab = Some(Grab::Resize { id: w.id, start: p, w: w.w, h: w.h });
                        }
                    }
                    draw_widget(&painter, w, rect, true);
                    let outline = if selected == Some(w.id) {
                        Stroke::new(2.0, accent())
                    } else if picked.contains(&w.id) {
                        Stroke::new(1.5, accent().gamma_multiply(0.7))
                    } else {
                        Stroke::new(1.0, Color32::from_gray(90))
                    };
                    painter.rect_stroke(rect, 6.0, outline, StrokeKind::Outside);
                    painter.text(handle.center(), egui::Align2::CENTER_CENTER, if w.locked { icon::LOCK } else { icon::ARROWS_OUT_SIMPLE }, egui::FontId::proportional(11.0), Color32::from_gray(160));
                    // The padlock: click it (or right-click it) to unlock.
                    if w.locked {
                        if corner.clicked() {
                            act = Some(Act::Lock(vec![wid], false));
                        }
                        corner.right_menu(|ui| {
                            if crate::ui::command(ui, label(icon::LOCK_OPEN, "Unlock")).clicked() {
                                act = Some(Act::Lock(vec![wid], false));
                                ui.close();
                            }
                        });
                        corner.on_hover_text("Locked in place: click the padlock to unlock (or Cmd+L)");
                    }
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
            // A band dragged out on empty space selects what it touches (with Shift / Cmd: adds them).
            if edit && grab.is_none()
                && let Some(band) = crate::ui::rubber_band(ui, &bg, ("surface_band", key))
            {
                let mut ids: Vec<u64> = if extend { picked.clone() } else { Vec::new() };
                for w in st.comp.surface.on_page().filter(|w| band.intersects(widget_rect(w))) {
                    if !ids.contains(&w.id) {
                        ids.push(w.id);
                    }
                }
                multi.set(&ctx, &mut selected, ids);
                picked = multi.all(&ctx, selected);
            }
            // Dragging in edit mode: snap to the grid.
            if let (Some(g), Some(p)) = (grab.clone(), pointer) {
                if ui.input(|i| i.pointer.primary_down()) {
                    let surface = &mut st.comp.surface;
                    match g {
                        Grab::Move { ids, start } => {
                            let (dx, dy) = (snapped((p.x - start.x) / cell, snap), snapped((p.y - start.y) / cell, snap));
                            for (id, x, y) in &ids {
                                if let Some(w) = surface.widget_mut(*id) {
                                    (w.x, w.y) = (*x, *y);
                                }
                            }
                            surface.move_many(&ids.iter().map(|t| t.0).collect::<Vec<_>>(), dx, dy);
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
        if inspector {
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(310.0);
                if picked.len() >= 2 {
                    act = act.take().or(group_editor(ui, &picked, &st.comp.surface));
                } else if let Some(id) = selected
                    && !widget_editor(ui, st, id, &targets)
                {
                    multi.clear(&ctx, &mut selected);
                }
            });
        }
    });

    if let Some(id) = open_editor {
        edit = true;
        multi.set(&ctx, &mut selected, vec![id]);
    }
    match act {
        Some(Act::Duplicate(ids)) => {
            let copies: Vec<SurfaceWidget> = st.comp.surface.widgets.iter().filter(|w| ids.contains(&w.id)).cloned().collect();
            let new = st.comp.surface.place_many(copies, None);
            if !new.is_empty() {
                multi.set(&ctx, &mut selected, new);
            }
        }
        Some(Act::Copy(ids)) => {
            let ws: Vec<SurfaceWidget> = st.comp.surface.widgets.iter().filter(|w| ids.contains(&w.id)).cloned().collect();
            if !ws.is_empty() {
                st.notice = Some(format!("Copied {}: right-click an empty spot to paste", crate::ui::plural(ws.len(), "{0} widget", "{0} widgets")));
                ctx.data_mut(|d| d.insert_temp(egui::Id::new(CLIPBOARD), ws));
            }
        }
        Some(Act::ToFront(ids)) => {
            let order: Vec<u64> = st.comp.surface.widgets.iter().map(|w| w.id).filter(|id| ids.contains(id)).collect();
            order.into_iter().for_each(|id| st.comp.surface.to_front(id));
        }
        Some(Act::ToBack(ids)) => {
            let order: Vec<u64> = st.comp.surface.widgets.iter().map(|w| w.id).filter(|id| ids.contains(id)).collect();
            order.into_iter().rev().for_each(|id| st.comp.surface.to_back(id));
        }
        Some(Act::Delete(ids)) => {
            ids.iter().for_each(|id| st.comp.remove_widget(*id));
            multi.clear(&ctx, &mut selected);
        }
        Some(Act::Move(ids, dx, dy)) => st.comp.surface.move_many(&ids, dx, dy),
        Some(Act::Arrange(ids, how)) => st.comp.surface.arrange(&ids, how),
        Some(Act::Lock(ids, on)) => st.comp.surface.widgets.iter_mut().filter(|w| ids.contains(&w.id)).for_each(|w| w.locked = on),
        Some(Act::Add(kind, x, y)) => {
            let id = st.comp.surface.add_at(kind, Some((x, y)));
            multi.set(&ctx, &mut selected, vec![id]);
        }
        Some(Act::Paste(x, y)) => {
            let new = st.comp.surface.place_many(copied, Some((x, y)));
            if !new.is_empty() {
                multi.set(&ctx, &mut selected, new);
            }
        }
        Some(Act::SelectAll) => {
            let all: Vec<u64> = st.comp.surface.on_page().map(|w| w.id).collect();
            multi.set(&ctx, &mut selected, all);
        }
        Some(Act::SelectNone) => multi.clear(&ctx, &mut selected),
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
    ctx.data_mut(|d| {
        d.insert_temp(edit_id, edit);
        d.insert_temp(sel_id, selected);
        d.insert_temp(grab_id, grab);
        d.insert_temp(snap_id, snap);
    });
}

/// "Paste Fader 3" or "Paste 4 widgets".
fn paste_label(copied: &[SurfaceWidget]) -> String {
    match copied {
        [one] => format!("Paste {}", one.label),
        many => format!("Paste {} widgets", many.len()),
    }
}

/// Line up / spread out / same size for several widgets (a menu or the group editor). `menu`: close after a pick.
fn arrange_items(ui: &mut egui::Ui, ids: &[u64], menu: bool) -> Option<Act> {
    let mut picked = None;
    let items: [(&str, &str, Arrange); 11] = [
        (icon::ALIGN_LEFT, "Left edges", Arrange::Left),
        (icon::ALIGN_CENTER_HORIZONTAL, "Centres across", Arrange::Centre),
        (icon::ALIGN_RIGHT, "Right edges", Arrange::Right),
        (icon::ALIGN_TOP, "Top edges", Arrange::Top),
        (icon::ALIGN_CENTER_VERTICAL, "Middles down", Arrange::Middle),
        (icon::ALIGN_BOTTOM, "Bottom edges", Arrange::Bottom),
        (icon::ARROWS_OUT_LINE_HORIZONTAL, "Spread across", Arrange::SpreadAcross),
        (icon::ARROWS_OUT_LINE_VERTICAL, "Spread down", Arrange::SpreadDown),
        (icon::ARROWS_HORIZONTAL, "Same width", Arrange::SameWidth),
        (icon::ARROWS_VERTICAL, "Same height", Arrange::SameHeight),
        (icon::FRAME_CORNERS, "Same size", Arrange::SameSize),
    ];
    for (i, (ic, text, how)) in items.into_iter().enumerate() {
        if i == 3 || i == 6 || i == 8 {
            ui.separator();
        }
        if crate::ui::command(ui, label(ic, text)).clicked() {
            picked = Some(Act::Arrange(ids.to_vec(), how));
            if menu {
                ui.close();
            }
        }
    }
    help(ui, "Spread keeps the outer two where they are. Sizes copy the one you picked first.");
    picked
}

/// Duplicate / Copy / front / back / Lock / Delete for one widget or the whole selection (`all_locked`: they're all
/// locked, so the item offers Unlock).
fn group_items(ui: &mut egui::Ui, ids: &[u64], all_locked: bool, menu: bool) -> Option<Act> {
    let mut picked = None;
    let n = ids.len();
    let many = |one: &str| if n > 1 { format!("{one} {n}") } else { one.to_string() };
    let mut item = |ui: &mut egui::Ui, ic: &str, text: &str, hover: &str, make: fn(Vec<u64>) -> Act| {
        let r = crate::ui::command(ui, label(ic, &many(text)));
        if (if hover.is_empty() { r } else { r.on_hover_text(hover) }).clicked() {
            picked = Some(make(ids.to_vec()));
            if menu {
                ui.close();
            }
        }
    };
    item(ui, icon::COPY, "Duplicate", "", Act::Duplicate);
    item(ui, icon::CLIPBOARD_TEXT, "Copy", "Paste on another page or in another project (right-click an empty spot)", Act::Copy);
    ui.separator();
    item(ui, icon::ARROW_LINE_UP, "Bring to front", "", Act::ToFront);
    item(ui, icon::ARROW_LINE_DOWN, "Send to back", "", Act::ToBack);
    ui.separator();
    if all_locked {
        item(ui, icon::LOCK_OPEN, "Unlock", "Let it move and resize again", |ids| Act::Lock(ids, false));
    } else {
        item(ui, icon::LOCK, "Lock", "Keep it where it is: no moving, resizing or arranging until unlocked (Cmd+L)", |ids| Act::Lock(ids, true));
    }
    ui.separator();
    if crate::ui::danger_button(ui, label(icon::TRASH, &many("Delete"))).clicked() {
        picked = Some(Act::Delete(ids.to_vec()));
        if menu {
            ui.close();
        }
    }
    picked
}

/// The side panel while several widgets are selected: what they are, and everything that works on them together.
fn group_editor(ui: &mut egui::Ui, ids: &[u64], sf: &phosphor_engine::ControlSurface) -> Option<Act> {
    ui.label(RichText::new(format!("{} selected", crate::ui::plural(ids.len(), "{0} widget", "{0} widgets"))).strong().color(accent()));
    let names: Vec<String> = ids.iter().filter_map(|id| sf.widget(*id)).map(|w| w.label.clone()).collect();
    hint(ui, names.join(", "));
    help(ui, "Drag any of them to move them all. Click one to edit it alone; Shift-click takes one out.");
    ui.add_space(4.0);
    crate::ui::heading(ui, "Arrange");
    let mut act = arrange_items(ui, ids, false);
    ui.add_space(4.0);
    let all_locked = ids.iter().filter_map(|id| sf.widget(*id)).all(|w| w.locked);
    act = act.or(group_items(ui, ids, all_locked, false));
    act
}

/// Keys in Edit while the pointer is over the canvas: Delete / Backspace removes the selected widgets, arrows nudge
/// them (a cell with Snap on, a quarter without; Shift = a whole cell), Cmd+D duplicates, Cmd+C copies, Cmd+V pastes
/// the copied widgets under the pointer, Cmd+L locks or unlocks, Cmd+A selects the page, Escape selects nothing. The keys are taken so
/// nothing else reads them.
fn designer_keys(ui: &egui::Ui, picked: &[u64], all_locked: bool, copied: bool, snap: bool, under_pointer: Option<(f32, f32)>) -> Option<Act> {
    use egui::{Key, Modifiers};
    ui.ctx().input_mut(|i| {
        if copied && i.consume_key(Modifiers::COMMAND, Key::V) && let Some((x, y)) = under_pointer {
            return Some(Act::Paste(x, y));
        }
        if i.consume_key(Modifiers::COMMAND, Key::A) {
            return Some(Act::SelectAll);
        }
        if picked.is_empty() {
            return None;
        }
        if i.consume_key(Modifiers::NONE, Key::Escape) {
            return Some(Act::SelectNone);
        }
        if i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace) {
            return Some(Act::Delete(picked.to_vec()));
        }
        if i.consume_key(Modifiers::COMMAND, Key::D) {
            return Some(Act::Duplicate(picked.to_vec()));
        }
        if i.consume_key(Modifiers::COMMAND, Key::C) {
            return Some(Act::Copy(picked.to_vec()));
        }
        if i.consume_key(Modifiers::COMMAND, Key::L) {
            return Some(Act::Lock(picked.to_vec(), !all_locked));
        }
        let step = if i.modifiers.shift || snap { 1.0 } else { 0.25 };
        let mut k = |key| (i.consume_key(Modifiers::NONE, key) || i.consume_key(Modifiers::SHIFT, key)) as i32 as f32;
        let dx = (k(Key::ArrowRight) - k(Key::ArrowLeft)) * step;
        let dy = (k(Key::ArrowDown) - k(Key::ArrowUp)) * step;
        ((dx, dy) != (0.0, 0.0)).then_some(Act::Move(picked.to_vec(), dx, dy))
    })
}

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
    ctx.data(|d| d.get_temp::<Option<u64>>(egui::Id::new(("surface_selected", u64::MAX)))).flatten()
}

/// Edit mode side panel for the selected widget (also the Inspector). Returns false once it's deleted.
pub(crate) fn widget_editor(ui: &mut egui::Ui, st: &mut State, id: u64, targets: &[(ControlTarget, String)]) -> bool {
    let mut delete = false;
    let mut duplicate = false;
    let text_layers: Vec<(u64, String)> = st.comp.layers.iter().filter(|l| matches!(l.source, phosphor_engine::SourceSpec::Text(_))).map(|l| (l.id, l.name.clone())).collect();
    let cols = st.comp.surface.columns.max(1) as f32;
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
    // Exactly where it is and how big (cells; a quarter at a time).
    crate::ui::toolbar(ui, |ui| {
        let mut typed = false;
        for (name, v, hover) in [("X", &mut w.x, "Cells from the left"), ("Y", &mut w.y, "Cells from the top"), ("W", &mut w.w, "Cells wide"), ("H", &mut w.h, "Cells high")] {
            ui.label(RichText::new(name).small().color(Color32::GRAY));
            typed |= guarded!(ui, *v, egui::DragValue::new(v).speed(0.25).max_decimals(2)).on_hover_text(hover).changed();
        }
        if typed {
            w.keep_inside(cols);
        }
        toggle(ui, &mut w.locked, label(icon::LOCK, "Lock")).on_hover_text("Keep it where it is: no moving, resizing or arranging until unlocked (Cmd+L on the canvas)");
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
            crate::ui::heading_help(ui, axis, "Each target gets this widget's value, rescaled to its from–to range.");
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
