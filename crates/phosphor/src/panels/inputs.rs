//! Inputs (in the Library): every live input on this Mac (cameras and capture cards, screens and windows,
//! Syphon, NDI) and a network stream address. Drag one onto a layer or a clip cell, or put it on the selected layer
//! or a new one. The list comes from `media::live::inputs`, the same one the layer's camera menu shows.

use super::*;

pub(crate) fn inputs_panel(ui: &mut egui::Ui, st: &mut State) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = st;
        hint(ui, "Live inputs (cameras, screens, Syphon, NDI) work in the Mac and Windows apps.");
    }
    #[cfg(not(target_arch = "wasm32"))]
    live_inputs(ui, st);
}

#[cfg(not(target_arch = "wasm32"))]
fn live_inputs(ui: &mut egui::Ui, st: &mut State) {
    use crate::media::live::{InputKind, inputs};
    let now = ui.ctx().input(|i| i.time);
    let mut rescan = false;
    let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        rescan = ui.small_button(icon::ARROWS_CLOCKWISE).on_hover_text("Look for cameras and capture cards again").clicked();
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| hint(ui, "Drag onto a layer or a clip cell, or use the buttons."));
    });
    let cams = crate::media::live::cameras_cached(ui.ctx(), rescan);
    let (list, notes) = inputs(&cams);
    let selected = st.selected.filter(|id| st.comp.layers.iter().any(|l| l.id == *id));
    let mut put: Option<(Option<u64>, String, SourceSpec)> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for kind in InputKind::ALL {
            let mine: Vec<_> = list.iter().filter(|i| i.kind == kind).collect();
            let note = notes.iter().find(|(k, _)| *k == kind).map(|(_, n)| n.clone());
            let title = format!("{} {} ({})", super::input_icon(kind), kind.title(), mine.len());
            egui::CollapsingHeader::new(title).id_salt(("inputs", kind as u8)).default_open(kind != InputKind::Window).show(ui, |ui| {
                if let Some(n) = &note {
                    hint(ui, n.as_str());
                }
                for i in mine {
                    if let Some(to) = input_row(ui, &i.label, &i.source, super::input_icon(kind), selected) {
                        put = Some((to, i.label.clone(), i.source.clone()));
                    }
                }
            });
        }
        egui::CollapsingHeader::new(format!("{} Network stream", icon::GLOBE)).id_salt("inputs_stream").default_open(true).show(ui, |ui| {
            let id = egui::Id::new("inputs_stream_url");
            let mut url: String = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
            crate::ui::text_field(ui, &mut url, "rtsp://… srt://… https://….m3u8", None);
            if url.contains("://") {
                let source = crate::media::live::stream_source(url.trim());
                if let Some(to) = input_row(ui, url.trim(), &source, icon::GLOBE, selected) {
                    put = Some((to, url.trim().to_string(), source));
                }
            }
            ui.ctx().data_mut(|d| d.insert_temp(id, url));
        });
    });
    if let Some((to, name, source)) = put {
        let layer = match to {
            Some(id) => id,
            None => {
                let l = st.comp.add_layer();
                l.name = name.chars().take(32).collect();
                l.id
            }
        };
        st.selected = Some(layer);
        st.launch_source(layer, source, Vec::new(), name, now);
    }
}

/// One input: a draggable name, then "on the selected layer" and "new layer" buttons. Returns where to put it
/// (`Some(Some(layer))` / `Some(None)` = a new layer).
#[cfg(not(target_arch = "wasm32"))]
fn input_row(ui: &mut egui::Ui, name: &str, source: &SourceSpec, glyph: &str, selected: Option<u64>) -> Option<Option<u64>> {
    let mut out = None;
    // Buttons on the right first, then the name fills what's left (cut short, full name on hover).
    let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.small_button(icon::PLUS).on_hover_text("On a new layer").clicked() {
            out = Some(None);
        }
        if crate::ui::small_command_if(ui, selected.is_some(), icon::ARROW_RIGHT).on_hover_text("On the selected layer").on_disabled_hover_text("Select a layer first").clicked() {
            out = Some(selected);
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let r = ui.add(egui::Label::new(format!("{glyph} {name}")).truncate().sense(Sense::click_and_drag()));
            r.dnd_set_drag_payload(super::SourceDrag { name: name.to_string(), source: source.clone() });
            if r.double_clicked() {
                out = Some(selected);
            }
            r.on_hover_text(format!("{name}\nDrag onto a layer or a clip cell · double-click: the selected layer"));
        });
    });
    out
}
