//! Stage: every output on one screen, live and as big as fits (the whole show as the projectors see it), each
//! with its name, size, where it goes, its GRID toggle. Click one to pick it in the Mapping Studio.

use super::*;
use eframe::egui::UiBuilder;

pub(crate) fn stage(ui: &mut egui::Ui, st: &mut State) {
    st.comp.ensure_outputs();
    let canvas = (st.comp.width, st.comp.height);
    let n = st.comp.outputs.len().max(1);
    let full = ui.available_rect_before_wrap();
    // As many columns as keeps the tiles closest to square-ish for the room there is.
    let cols = (1..=n).min_by_key(|&c| {
        let rows = n.div_ceil(c);
        let (w, h) = (full.width() / c as f32, full.height() / rows as f32);
        ((w / h.max(1.0) - 16.0 / 9.0).abs() * 1000.0) as i64
    }).unwrap_or(1);
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
        let pic = child.available_rect_before_wrap();
        let p = child.painter();
        p.rect_filled(pic, 4.0, Color32::BLACK);
        match tex.filter(|_| o.enabled) {
            Some(t) => paint_texture(p, t, fit_rect(pic.shrink(2.0), ow as f32 / oh.max(1) as f32)),
            None => {
                p.text(pic.center(), egui::Align2::CENTER_CENTER, "Off", egui::FontId::proportional(14.0), Color32::GRAY);
            }
        }
        let r = child.interact(pic, child.id().with(("stage_pick", id)), Sense::click());
        let border = if selected { accent() } else if r.hovered() { Color32::from_gray(140) } else { Color32::from_gray(50) };
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
