//! Scopes panel: waveform, RGB parade, vectorscope or histogram of the mix or one layer (`crate::scopes`).

use phosphor_engine::MASTER_ID;

use super::*;

pub(crate) fn scopes(ui: &mut egui::Ui, st: &mut State) {
    st.scopes.shown_at = ui.input(|i| i.time);
    ui.horizontal_wrapped(|ui| {
        let s = &mut st.scopes;
        choice_buttons(ui, &mut s.kind, |_, _| {});
        let current = if s.source == MASTER_ID { "Mix".to_string() } else { format!("Layer: {}", crate::ui::layer_name(&st.comp.layers, s.source)) };
        ComboBox::from_id_salt("scopes_src").width(150.0).selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut s.source, MASTER_ID, "Mix");
            crate::ui::layer_items(ui, &st.comp.layers, &mut s.source, |id| id);
        });
    });
    let hint_text = match st.scopes.kind {
        crate::scopes::ScopeKind::Waveform => "Brightness across the picture: top = white, bottom = black",
        crate::scopes::ScopeKind::Parade => "Red, green and blue side by side: level them to balance colour",
        crate::scopes::ScopeKind::Vectorscope => "Colour: the centre is grey, further out = more saturated",
        crate::scopes::ScopeKind::Histogram => "How much of the picture sits at each level, per colour (left = dark)",
    };
    hint(ui, hint_text);
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 4.0, Color32::from_gray(4));
    match st.scopes.texture(ui.ctx()) {
        Some(t) => paint_texture(ui.painter(), t.id(), fit_rect(rect.shrink(4.0), 256.0 / 160.0)),
        None => {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "measuring…", egui::FontId::proportional(13.0), Color32::GRAY);
        }
    }
    ui.allocate_rect(rect, Sense::hover());
}
