//! A label that follows the mouse while something is carried (an effect, a generator, a file, a source, a layer),
//! so you can see what you're about to drop.

use super::*;

/// Draws it (once per frame, over everything).
pub(crate) fn drag_ghost(ctx: &egui::Context, st: &State) {
    use egui::DragAndDrop as Dnd;
    let what = if let Some(p) = Dnd::payload::<crate::explorer::PaletteDrag>(ctx) {
        p.effect.and_then(shaders::effect).map(|d| label(icon::MAGIC_WAND, d.name)).or_else(|| p.generator.and_then(shaders::generator).map(|d| label(icon::SPARKLE, d.name)))
    } else if let Some(m) = Dnd::payload::<MediaDrag>(ctx) {
        st.comp.media_bin.get(m.0).map(|m| label(icon::FILM_STRIP, &m.name))
    } else if let Some(s) = Dnd::payload::<SourceDrag>(ctx) {
        Some(label(icon::VIDEO_CAMERA, &s.name))
    } else if let Some(id) = Dnd::payload::<u64>(ctx) {
        st.comp.layer(*id).map(|l| l.name.clone()).or_else(|| st.comp.group(*id).map(|g| g.name.clone())).map(|n| label(icon::STACK, &n))
    } else {
        None
    };
    let (Some(text), Some(at)) = (what, ctx.pointer_hover_pos()) else { return };
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag_ghost")));
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), Color32::BLACK);
    let rect = egui::Rect::from_min_size(at + egui::vec2(14.0, 12.0), galley.size() + egui::vec2(12.0, 6.0));
    painter.rect_filled(rect, 4.0, accent());
    painter.galley(rect.min + egui::vec2(6.0, 3.0), galley, Color32::BLACK);
}
