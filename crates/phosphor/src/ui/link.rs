//! Link from the source (VDMX's drag from a data source): carry a shared modulator, a Control Surface widget, a dial,
//! a controller input (a row of the MIDI / OSC monitor) or any slider (Alt-drag) onto a slider, and the two are linked.
//! - a modulator: the slider follows it (its modulation links to the modulator);
//! - a widget or dial (a driver, `phosphor_engine::is_driver`): it drives the slider (one more target in its list);
//! - a slider dropped on a dial: the dial drives it;
//! - any other control: the slider follows it (`Composition::follows`);
//! - a controller input: mapped to the slider (as Learn would).

use super::*;

/// What's carried to make a link.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LinkSource {
    Modulator { id: u64, name: String },
    Control { target: ControlTarget, name: String },
    Input(phosphor_engine::ControlSource),
}

impl LinkSource {
    pub(crate) fn name(&self) -> String {
        match self {
            LinkSource::Modulator { name, .. } | LinkSource::Control { name, .. } => name.clone(),
            LinkSource::Input(s) => s.label(),
        }
    }

    /// Whether it can land on a control with this target (and, for a modulator, a modulation of its own).
    fn lands_on(&self, target: Option<&ControlTarget>, has_modulation: bool) -> bool {
        match self {
            LinkSource::Modulator { .. } => has_modulation,
            LinkSource::Control { target: from, .. } => target.is_some_and(|t| t != from),
            LinkSource::Input(_) => target.is_some(),
        }
    }
}

/// A link dropped on a control, for the app to make: (the control it landed on, the control carried).
pub const LINK_REQUEST: &str = "phosphor_link_request";

/// A small handle to drag `source` from onto any slider.
pub(crate) fn link_handle(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, source: LinkSource, tip: &str) -> egui::Response {
    let id = ui.id().with(("link_handle", salt));
    let r = ui
        .dnd_drag_source(id, source, |ui| {
            let (rect, r) = ui.allocate_exact_size(egui::vec2(CONTROL_H, CONTROL_H), Sense::hover());
            let color = if r.hovered() { accent() } else { ui.visuals().weak_text_color() };
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon::LINK, egui::FontId::proportional(14.0), color);
        })
        .response;
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    r.on_hover_text(tip)
}

/// A row's words (a monitor row's input) that can be carried onto a slider.
pub(crate) fn link_drag_text(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, source: LinkSource, text: RichText, tip: &str) -> egui::Response {
    let id = ui.id().with(("link_text", salt));
    let r = ui.dnd_drag_source(id, source, |ui| ui.label(text)).response;
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    r.on_hover_text(tip)
}

/// Alt-drag on a slider: it carries itself (to land on another slider or a dial). Call before the slider reads its
/// drag; true while a link is being carried, so no slider moves under it.
pub(crate) fn carry_from_slider(ui: &egui::Ui, r: &egui::Response, target: Option<&ControlTarget>, name: &str) -> bool {
    if r.drag_started_by(egui::PointerButton::Primary)
        && ui.input(|i| i.modifiers.alt)
        && let Some(t) = target
    {
        egui::DragAndDrop::set_payload(ui.ctx(), LinkSource::Control { target: t.clone(), name: name.to_string() });
    }
    egui::DragAndDrop::has_payload_of_type::<LinkSource>(ui.ctx())
}

/// The drop side of a slider: lit while a link it can take is carried over it; dropped, the link is made. A
/// modulator lands in `modulation` straight away (the slider's own; without one a modulator can't land); anything
/// else goes to the app (`LINK_REQUEST`, a mapping for an input). Returns true when `modulation` changed.
pub(crate) fn link_drop(ui: &egui::Ui, r: &egui::Response, target: Option<&ControlTarget>, modulation: Option<&mut Option<Modulation>>) -> bool {
    let Some(carried) = r.dnd_hover_payload::<LinkSource>() else { return false };
    if !carried.lands_on(target, modulation.is_some()) {
        return false;
    }
    // Lit: it takes this.
    let p = ui.painter();
    p.rect_filled(r.rect, 0.0, accent().gamma_multiply(0.18));
    p.rect_stroke(r.rect, 0.0, Stroke::new(2.0, accent()), StrokeKind::Inside);
    let Some(src) = drop_payload::<LinkSource>(r) else { return false };
    match (&*src, target) {
        (LinkSource::Modulator { id, .. }, _) => {
            let Some(m) = modulation else { return false };
            // Already moving: it keeps its amount / range and follows the modulator instead. New: the modulator
            // sweeps the whole range, the way a drop links in VDMX.
            *m = Some(match *m {
                Some(old) => Modulation { link: *id, ..old },
                None => Modulation { link: *id, range: true, min: 0.0, max: 1.0, ..Default::default() },
            });
            return true;
        }
        (LinkSource::Input(source), Some(t)) => {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(MAP_REQUEST), (t.clone(), source.clone())));
        }
        (LinkSource::Control { target: from, .. }, Some(t)) => {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(LINK_REQUEST), (t.clone(), from.clone())));
        }
        _ => {}
    }
    false
}
