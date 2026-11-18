//! Stage: the whole show at once. **Layout** (the usual): a big black canvas of the venue with every output live where
//! it really is (a center screen, two wings, an LED wall…), dragged into place and sized by its corner, edges snapping
//! to each other, a picture of the venue behind them if you like; pan and zoom like the Mapping Studio. **Tiles**:
//! every output side by side, as big as fits. Click an output to pick it in the Mapping Studio. Only for seeing and
//! planning: nothing here changes what an output shows. LED / DMX fixtures show here too, each light in the color
//! it's being sent (so a recording playing back, Flash / Chase, or the picture), placed like the outputs.

use super::*;
use eframe::egui::UiBuilder;
use phosphor_engine::StagePlace;

const TILES: &str = "phosphor_stage_tiles";

/// The layout canvas's view: screen pixels per stage unit, and where stage (0, 0) is from the canvas's top-left.
#[derive(Clone, Copy)]
struct View {
    scale: f32,
    origin: egui::Vec2,
    /// Fit everything in next frame (first look, Fit all).
    fit: bool,
}

impl Default for View {
    fn default() -> Self {
        Self { scale: 100.0, origin: egui::Vec2::ZERO, fit: true }
    }
}

pub(crate) fn stage(ui: &mut egui::Ui, st: &mut State) {
    st.comp.ensure_outputs();
    let mut tiles: bool = ui.ctx().data(|d| d.get_temp(egui::Id::new(TILES))).unwrap_or(false);
    let view_id = egui::Id::new("stage_view");
    let mut view: View = ui.ctx().data(|d| d.get_temp(view_id)).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        segmented(ui, &mut tiles, &[(false, "Layout".to_string()), (true, "Tiles".to_string())], |_, r, t| {
            r.clone().on_hover_text(if *t { "Every output side by side, as big as fits" } else { "Every output where it is in the venue: drag them into place" });
        });
        if tiles {
            return;
        }
        if crate::ui::command(ui, label(icon::CORNERS_OUT, "Fit all")).on_hover_text("Show every output").clicked() {
            view.fit = true;
        }
        if crate::ui::command(ui, label(icon::ROWS, "Line up")).on_hover_text("Every output back in one row, in order (sizes kept), and the LED fixtures in a row under them").clicked() {
            st.comp.line_up_stage();
            view.fit = true;
        }
        venue_menu(ui, st);
        help(ui, "Drag an output (or an LED fixture, shown lit with what it's sent) to where it is in the venue; its corner sizes it, edges snap to the others. Scroll or drag the black to move around, pinch or Cmd-scroll to zoom. Click one to pick it in the Mapping Studio.");
    });
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(TILES), tiles));
    if tiles {
        tiles_view(ui, st);
    } else {
        layout_view(ui, st, &mut view);
    }
    ui.ctx().data_mut(|d| d.insert_temp(view_id, view));
}

/// The venue picture behind the outputs: choose, how strongly it shows, remove.
fn venue_menu(ui: &mut egui::Ui, st: &mut State) {
    let has = !st.comp.stage.backdrop.is_empty();
    crate::ui::field_menu(ui, label(icon::IMAGE, if has { "Venue picture" } else { "Venue picture…" }), |ui| {
        ui.set_min_width(240.0);
        help(ui, "A photo or drawing of the venue behind the outputs, to place them where they really are. Drag it, its corner sizes it. Only shows here.");
        #[cfg(not(target_arch = "wasm32"))]
        if crate::ui::command(ui, label(icon::FOLDER_OPEN, if has { "Another picture…" } else { "Choose a picture…" })).clicked() {
            st.dialogs.pick_file(crate::dialogs::Purpose::StageBackdrop, Some(("Picture", &["png", "jpg", "jpeg", "heic", "tif", "tiff", "webp", "svg"])));
            ui.close();
        }
        if has {
            ui.horizontal(|ui| {
                Level::percent(&mut st.comp.stage.backdrop_opacity, "Shows").default(0.6).show(ui);
            });
            if danger_button(ui, label(icon::X, "Remove the picture")).clicked() {
                st.comp.stage.backdrop.clear();
                st.comp.stage.backdrop_place = None;
                ui.close();
            }
        }
    });
}

fn layout_view(ui: &mut egui::Ui, st: &mut State, view: &mut View) {
    let area = ui.available_rect_before_wrap();
    if area.width() < 40.0 || area.height() < 40.0 {
        return;
    }
    let canvas = (st.comp.width, st.comp.height);
    let places = st.comp.stage_places();
    // Each output: id, its rect on the stage (x, y, w, h), its picture, name, size, on.
    let items: Vec<(u64, [f32; 4], Option<egui::TextureId>, String, (u32, u32), bool)> = st
        .comp
        .outputs
        .iter()
        .zip(&places)
        .map(|(o, (_, p))| {
            let (w, h) = o.size(canvas);
            (o.id, p.rect(w, h), st.output_tex(o.id), o.name.clone(), (w, h), o.enabled)
        })
        .collect();
    // The venue picture: as placed, else around the outputs with room to spare.
    let lights = fixture_items(st);
    let bounds = items
        .iter()
        .map(|(_, r, ..)| *r)
        .chain(lights.iter().map(|l| l.rect))
        .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, r| [b[0].min(r[0]), b[1].min(r[1]), b[2].max(r[0] + r[2]), b[3].max(r[1] + r[3])]);
    let backdrop = (!st.comp.stage.backdrop.is_empty()).then(|| st.thumbs.get_sized(&st.comp.stage.backdrop, 2048)).flatten();
    let backdrop_aspect = st.thumbs.aspect_sized(&st.comp.stage.backdrop, 2048).unwrap_or(16.0 / 9.0);
    let backdrop_place = st.comp.stage.backdrop_place.unwrap_or_else(|| {
        let w = (bounds[2] - bounds[0]) * 1.5;
        let h = w / backdrop_aspect;
        StagePlace { x: (bounds[0] + bounds[2]) * 0.5 - w * 0.5, y: (bounds[1] + bounds[3]) * 0.5 - h * 0.5, w }
    });
    let backdrop_rect = [backdrop_place.x, backdrop_place.y, backdrop_place.w, backdrop_place.w / backdrop_aspect];

    // Pan and zoom (the empty canvas behind everything).
    let bg = ui.interact(area, ui.id().with("stage_bg"), Sense::click_and_drag());
    if view.fit {
        view.fit = false;
        let mut b = bounds;
        if backdrop.is_some() {
            b = [b[0].min(backdrop_rect[0]), b[1].min(backdrop_rect[1]), b[2].max(backdrop_rect[0] + backdrop_rect[2]), b[3].max(backdrop_rect[1] + backdrop_rect[3])];
        }
        let (bw, bh) = ((b[2] - b[0]).max(0.1), (b[3] - b[1]).max(0.1));
        // Room for the names above the outputs.
        view.scale = ((area.width() - 60.0) / bw).min((area.height() - 80.0) / bh).max(1.0);
        let centre = egui::vec2((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5) * view.scale;
        view.origin = area.size() * 0.5 - centre + egui::vec2(0.0, 10.0);
    }
    if ui.rect_contains_pointer(area) {
        let (zoom, scroll, pointer) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
        if zoom != 1.0 {
            let new = (view.scale * zoom).clamp(2.0, 5000.0);
            if let Some(p) = pointer {
                let at = p - area.min;
                view.origin = at - (at - view.origin) * (new / view.scale);
            }
            view.scale = new;
        } else {
            view.origin += scroll;
        }
    }
    if bg.dragged() {
        view.origin += bg.drag_delta();
    }
    let to_screen = |x: f32, y: f32| area.min + view.origin + egui::vec2(x, y) * view.scale;
    let rect_of = |r: [f32; 4]| egui::Rect::from_min_size(to_screen(r[0], r[1]), egui::vec2(r[2], r[3]) * view.scale);

    let mut child = ui.new_child(UiBuilder::new().max_rect(area));
    child.set_clip_rect(area.intersect(ui.clip_rect()));
    let p = child.painter().clone();
    p.rect_filled(area, 4.0, Color32::BLACK);
    // A faint grid, one line a stage unit (thinned out when zoomed far out).
    let step = (1..).map(|k| 2f32.powi(k - 1)).find(|s| s * view.scale >= 24.0).unwrap_or(1.0);
    let (x0, y0) = (((-view.origin.x) / view.scale / step).floor() as i32, ((-view.origin.y) / view.scale / step).floor() as i32);
    let (nx, ny) = ((area.width() / view.scale / step) as i32 + 2, (area.height() / view.scale / step) as i32 + 2);
    let grid = Stroke::new(1.0, crate::theme::line().gamma_multiply(0.35));
    for i in x0..x0 + nx {
        let x = to_screen(i as f32 * step, 0.0).x;
        p.line_segment([egui::pos2(x, area.top()), egui::pos2(x, area.bottom())], grid);
    }
    for j in y0..y0 + ny {
        let y = to_screen(0.0, j as f32 * step).y;
        p.line_segment([egui::pos2(area.left(), y), egui::pos2(area.right(), y)], grid);
    }

    // The venue picture first (underneath), then the outputs on top (they win the pointer where they overlap it).
    if let Some(t) = backdrop {
        let r = rect_of(backdrop_rect);
        p.image(t, r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE.gamma_multiply(st.comp.stage.backdrop_opacity.clamp(0.0, 1.0)));
        if let Some(moved) = placed_item(&child, ("stage_backdrop", 0u64), r, backdrop_place, view.scale, &[], None) {
            st.comp.stage.backdrop_place = Some(moved);
        }
    }
    let others: Vec<(u64, [f32; 4])> = items.iter().map(|(id, r, ..)| (*id, *r)).collect();
    let mut pick = None;
    for (i, (id, r, tex, name, (w, h), on)) in items.iter().enumerate() {
        let screen = rect_of(*r);
        p.rect_filled(screen, 2.0, crate::theme::PICTURE_BG);
        match tex.filter(|_| *on) {
            Some(t) => paint_texture(&p, t, screen),
            None => {
                p.text(screen.center(), egui::Align2::CENTER_CENTER, crate::i18n::tr("Off"), egui::FontId::proportional(14.0), Color32::GRAY);
            }
        }
        let selected = st.mapping_output == *id;
        let place = places[i].1;
        let snaps: Vec<[f32; 4]> = others.iter().filter(|(o, _)| o != id).map(|(_, r)| *r).collect();
        let (moved, clicked, hovered) = {
            let resp = child.interact(screen, child.id().with(("stage_out", *id)), Sense::click_and_drag());
            let moved = placed_item(&child, ("stage_out_drag", *id), screen, place, view.scale, &snaps, Some(&resp));
            (moved, resp.clicked(), resp.hovered())
        };
        if let Some(m) = moved {
            // The first output moved pins every other one where it shows (unplaced ones line up after the placed: they
            // would follow it otherwise).
            for (o, (_, at)) in st.comp.outputs.iter_mut().zip(&places) {
                o.stage = Some(if o.id == *id { m } else { o.stage.unwrap_or(*at) });
            }
        }
        if clicked {
            pick = Some(*id);
        }
        let border = if selected { accent() } else if hovered { crate::theme::faint() } else { crate::theme::line() };
        p.rect_stroke(screen, 2.0, Stroke::new(if selected { 2.0 } else { 1.0 }, border), StrokeKind::Outside);
        // Its name above it.
        let words = format!("{}  {name}  {w}\u{d7}{h}", i + 1);
        // (Cut to its own width: narrow outputs side by side would write over each other.)
        let color = if selected { accent() } else { crate::theme::faint() };
        let mut job = egui::text::LayoutJob::single_section(words, egui::TextFormat::simple(egui::FontId::proportional(12.0), color));
        job.wrap = egui::text::TextWrapping::truncate_at_width(screen.width().max(24.0));
        let galley = p.layout_job(job);
        p.galley(screen.left_top() - egui::vec2(0.0, 4.0 + galley.size().y), galley, color);
    }
    if let Some(id) = pick {
        st.mapping_output = id;
    }
    draw_fixtures(&child, st, &lights, &rect_of, view.scale);
    ui.allocate_rect(area, Sense::hover());
}

/// One LED / DMX fixture on the stage: its rect, its lights (place in the box, 0..1) and what they are sent.
struct Light {
    id: u64,
    name: String,
    place: StagePlace,
    rect: [f32; 4],
    dots: Vec<[f32; 2]>,
    on: bool,
}

fn fixture_items(st: &State) -> Vec<Light> {
    let aspect = st.comp.width.max(1) as f32 / st.comp.height.max(1) as f32;
    st.comp
        .dmx
        .fixtures
        .iter()
        .zip(st.comp.fixture_places())
        .map(|(f, (_, place))| {
            let (dots, shape) = phosphor_engine::fixture_dots(f, aspect);
            Light { id: f.id, name: f.name.clone(), place, rect: [place.x, place.y, place.w, place.w / shape.max(0.01)], dots, on: f.enabled && st.comp.dmx.enabled }
        })
        .collect()
}

/// The fixtures, each light in the color it's being sent (dark gray while LED / DMX is off), dragged and sized like
/// the outputs. The first one moved pins the others where they show.
fn draw_fixtures(ui: &egui::Ui, st: &mut State, lights: &[Light], rect_of: &dyn Fn([f32; 4]) -> egui::Rect, scale: f32) {
    if lights.is_empty() {
        return;
    }
    // What goes out is copied for us only while something watches it.
    #[cfg(not(target_arch = "wasm32"))]
    {
        st.dmx.watching = true;
    }
    #[cfg(not(target_arch = "wasm32"))]
    let sent = st.dmx.sent.clone();
    #[cfg(target_arch = "wasm32")]
    let sent = std::collections::BTreeMap::new();
    let p = ui.painter();
    let snaps: Vec<(u64, [f32; 4])> = lights.iter().map(|l| (l.id, l.rect)).collect();
    let mut moved_to = None;
    for l in lights {
        let screen = rect_of(l.rect);
        if !ui.clip_rect().intersects(screen) {
            continue;
        }
        let colors = st.comp.dmx.fixtures.iter().find(|f| f.id == l.id).map(|f| f.read(&sent)).unwrap_or_default();
        let r = (screen.width().min(screen.height()) * 0.35).clamp(1.0, 6.0);
        let step = (l.dots.len() / 2000).max(1);
        for (i, d) in l.dots.iter().enumerate().step_by(step) {
            let at = egui::pos2(screen.left() + d[0] * screen.width(), screen.top() + d[1] * screen.height());
            let c = colors.get(i).filter(|_| l.on).map_or(crate::theme::PICTURE_GRID, |c| crate::theme::show_color(*c));
            p.circle_filled(at, r, c);
        }
        let resp = ui.interact(screen, ui.id().with(("stage_fix", l.id)), Sense::click_and_drag());
        let others: Vec<[f32; 4]> = snaps.iter().filter(|(o, _)| *o != l.id).map(|(_, r)| *r).collect();
        if let Some(m) = placed_item(ui, ("stage_fix_drag", l.id), screen, l.place, scale, &others, Some(&resp)) {
            moved_to = Some((l.id, m));
        }
        if resp.hovered() || resp.dragged() {
            p.rect_stroke(screen, 2.0, Stroke::new(1.0, crate::theme::faint()), StrokeKind::Outside);
        }
        let color = crate::theme::faint();
        let mut job = egui::text::LayoutJob::single_section(format!("{} {}", icon::LIGHTBULB, l.name), egui::TextFormat::simple(egui::FontId::proportional(11.0), color));
        job.wrap = egui::text::TextWrapping::truncate_at_width(screen.width().max(120.0));
        let galley = p.layout_job(job);
        p.galley(screen.left_bottom() + egui::vec2(0.0, 3.0), galley, color);
        resp.on_hover_text(format!("{}: drag it to where it is, its corner sizes it. Lit with what it's being sent.", l.name));
    }
    if let Some((id, m)) = moved_to {
        for (f, l) in st.comp.dmx.fixtures.iter_mut().zip(lights) {
            f.stage = Some(if f.id == id { m } else { f.stage.unwrap_or(l.place) });
        }
    }
}

/// Something on the stage that moves (drag it) and sizes (drag its bottom-right corner, shape kept). `resp` = its own
/// response when the caller made one (outputs: clicks pick them); else one is made here. Moving snaps its edges to
/// `snaps` (other items' x, y, w, h) within a few pixels. Returns its new place when it moved.
fn placed_item(ui: &egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, screen: egui::Rect, place: StagePlace, scale: f32, snaps: &[[f32; 4]], resp: Option<&egui::Response>) -> Option<StagePlace> {
    let own;
    let resp = match resp {
        Some(r) => r,
        None => {
            own = ui.interact(screen, ui.id().with(&id), Sense::drag());
            &own
        }
    };
    let corner = egui::Rect::from_center_size(screen.right_bottom(), egui::vec2(12.0, 12.0));
    let handle = ui.interact(corner, ui.id().with((&id, "corner")), Sense::drag());
    if handle.hovered() || handle.dragged() || resp.hovered() {
        ui.painter().rect_filled(corner.shrink(2.0), 1.0, accent());
    }
    if handle.hovered() || handle.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
    }
    if handle.dragged() {
        let w = (place.w + handle.drag_delta().x / scale).max(0.2);
        return Some(StagePlace { w, ..place });
    }
    if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let d = resp.drag_delta() / scale;
        let mut p = StagePlace { x: place.x + d.x, y: place.y + d.y, ..place };
        let h = screen.height() / scale;
        let tol = 8.0 / scale;
        p.x = snap(p.x, p.w, snaps.iter().flat_map(|r| [r[0], r[0] + r[2]]), tol);
        p.y = snap(p.y, h, snaps.iter().flat_map(|r| [r[1], r[1] + r[3]]), tol);
        return Some(p);
    }
    None
}

/// A span starting at `at`, `len` long, moved so either end lands on the nearest edge within `tol` (else as it is).
fn snap(at: f32, len: f32, edges: impl Iterator<Item = f32> + Clone, tol: f32) -> f32 {
    let best = edges.flat_map(|e| [e - at, e - (at + len)]).filter(|d| d.abs() <= tol).min_by(|a, b| a.abs().total_cmp(&b.abs()));
    at + best.unwrap_or(0.0)
}

/// Every output side by side, as big as fits (the earlier Stage view).
fn tiles_view(ui: &mut egui::Ui, st: &mut State) {
    let canvas = (st.comp.width, st.comp.height);
    let n = st.comp.outputs.len().max(1);
    let full = ui.available_rect_before_wrap();
    // As many columns as keeps the tiles closest to square-ish for the room there is.
    let cols = (1..=n)
        .min_by_key(|&c| {
            let rows = n.div_ceil(c);
            let (w, h) = (full.width() / c as f32, full.height() / rows as f32);
            ((w / h.max(1.0) - 16.0 / 9.0).abs() * 1000.0) as i64
        })
        .unwrap_or(1);
    let rows = n.div_ceil(cols);
    let cell = egui::vec2(full.width() / cols as f32, full.height() / rows as f32);
    let mut pick: Option<u64> = None;
    for i in 0..st.comp.outputs.len() {
        let (id, (ow, oh)) = (st.comp.outputs[i].id, st.comp.outputs[i].size(canvas));
        let tex = st.output_tex(id);
        let selected = st.mapping_output == id;
        let open = st.output_window_open(id);
        let screen = st.comp.outputs[i].screen.clone();
        let o = &mut st.comp.outputs[i];
        let rect = egui::Rect::from_min_size(full.min + egui::vec2((i % cols) as f32 * cell.x, (i / cols) as f32 * cell.y), cell).shrink(6.0);
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
        child.horizontal(|ui| {
            ui.label(RichText::new(format!("{}", i + 1)).strong().color(accent()));
            crate::ui::heading(ui, &o.name);
            hint(ui, format!("{ow}×{oh}"));
            super::output::grid_toggle(ui, &mut o.grid);
            if o.mapping.active() {
                ui.label(RichText::new(format!("{} mapped", icon::PROJECTOR_SCREEN)).small().color(accent()));
            }
            let goes = if open { "window open".to_string() } else if screen.is_empty() { "not shown".to_string() } else { format!("to {screen}") };
            hint(ui, goes);
        });
        // The frame is the output's own shape, under its name (the whole cell painted black read as a letterboxed
        // picture in a tall panel).
        let room = child.available_rect_before_wrap();
        let fitted = fit_rect(room, ow as f32 / oh.max(1) as f32);
        let pic = fitted.translate(egui::vec2(0.0, room.top() - fitted.top()));
        let p = child.painter();
        p.rect_filled(pic, 4.0, Color32::BLACK);
        match tex.filter(|_| o.enabled) {
            Some(t) => paint_texture(p, t, pic.shrink(2.0)),
            None => {
                p.text(pic.center(), egui::Align2::CENTER_CENTER, "Off", egui::FontId::proportional(14.0), Color32::GRAY);
            }
        }
        let r = child.interact(pic, child.id().with(("stage_pick", id)), Sense::click());
        let border = if selected { accent() } else if r.hovered() { crate::theme::faint() } else { crate::theme::line() };
        child.painter().rect_stroke(pic, 4.0, Stroke::new(if selected { 2.0 } else { 1.0 }, border), StrokeKind::Inside);
        if r.on_hover_text("Click: pick this output in the Mapping Studio").clicked() {
            pick = Some(id);
        }
    }
    if let Some(id) = pick {
        st.mapping_output = id;
    }
    ui.allocate_rect(full, Sense::hover());
}

#[cfg(test)]
mod tests {
    #[test]
    fn edges_snap_to_the_nearest_within_reach() {
        // A 4-wide output dropped at 6.03 lands against an edge at 6.0; one at 1.97 lands its right end on 6.0.
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(near(super::snap(6.03, 4.0, [6.0, 10.0].into_iter(), 0.1), 6.0));
        assert!(near(super::snap(1.97, 4.0, [6.0].into_iter(), 0.1), 2.0));
        assert_eq!(super::snap(3.0, 4.0, [6.0].into_iter(), 0.1), 3.0, "nothing near: stays");
    }
}
