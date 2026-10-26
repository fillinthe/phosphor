//! Projector control (PJLink) in the UI: one output's projector (address, password, on / off / shutter / check) and
//! a row for all of them at once (mappable, for the start and end of a show).

use phosphor_engine::{AppControl, OutputConfig, Projector};

use crate::pjlink::{Ask, Projectors};

use super::*;

/// The Projector section of an output's properties.
pub(crate) fn projector_settings(ui: &mut egui::Ui, o: &mut OutputConfig, projectors: &mut Projectors) {
    crate::ui::section(ui, ("pjlink", o.id), label(icon::PROJECTOR_SCREEN_CHART, "Projector (PJLink)"), o.projector.is_some(), |ui| {
        let Some(p) = &mut o.projector else {
            help(ui, "Switch this output's projector on and off, and close its shutter, over the network (most Epson, Panasonic, NEC, Sony, Optoma, Christie and Barco projectors).");
            if crate::ui::command(ui, label(icon::PLUS, "Add its projector")).clicked() {
                o.projector = Some(Projector::default());
            }
            return;
        };
        ui.horizontal(|ui| {
            ui.label("Address");
            text_field(ui, &mut p.host, "192.168.1.50", Some(150.0)).on_hover_text("The projector's IP address or name (PJLink port 4352; add :port for another)");
        });
        ui.horizontal(|ui| {
            ui.label("Password");
            ui.add(egui::TextEdit::singleline(&mut p.password).password(true).hint_text("none").desired_width(150.0)).on_hover_text("The projector's PJLink password, if it has one (set in its network menu)");
        });
        let p = p.clone();
        ui.horizontal_wrapped(|ui| {
            if crate::ui::command(ui, label(icon::POWER, "Switch on")).clicked() {
                projectors.ask(o.id, &p, Ask::PowerOn);
            }
            if crate::ui::command(ui, "Switch off").clicked() {
                projectors.ask(o.id, &p, Ask::PowerOff);
            }
            if crate::ui::command(ui, label(icon::EYE_SLASH, "Shutter")).on_hover_text("Close the shutter (picture black, lamp stays on)").clicked() {
                projectors.ask(o.id, &p, Ask::Shutter(true));
            }
            if crate::ui::command(ui, label(icon::EYE, "Open")).on_hover_text("Open the shutter").clicked() {
                projectors.ask(o.id, &p, Ask::Shutter(false));
            }
            if crate::ui::command(ui, label(icon::ARROWS_CLOCKWISE, "Check")).on_hover_text("Ask the projector how it is").clicked() {
                projectors.ask(o.id, &p, Ask::Status);
            }
        });
        if let Some(s) = projectors.status.get(&o.id) {
            hint(ui, s.clone());
        }
        if crate::ui::small_command(ui, label(icon::X, "Forget this projector")).clicked() {
            o.projector = None;
        }
    });
}

/// "Projectors: all on / all off / shutter", when any output has one. Each is mappable (right-click › Learn).
pub(crate) fn all_projectors_row(ui: &mut egui::Ui, outputs: &[OutputConfig], projectors: &mut Projectors) {
    if !outputs.iter().any(|o| o.projector.as_ref().is_some_and(|p| !p.host.trim().is_empty())) {
        return;
    }
    crate::ui::toolbar(ui, |ui| {
        ui.label(label(icon::PROJECTOR_SCREEN_CHART, "Projectors"));
        let on = crate::ui::command(ui, "All on");
        if on.clicked() {
            projectors.ask_all(outputs, Ask::PowerOn);
        }
        map_menu(ui, &on, ControlTarget::App(AppControl::ProjectorsOn));
        let off = crate::ui::command(ui, "All off");
        if off.clicked() {
            projectors.ask_all(outputs, Ask::PowerOff);
        }
        map_menu(ui, &off, ControlTarget::App(AppControl::ProjectorsOff));
        let mut closed = projectors.shutter_closed;
        let r = toggle(ui, &mut closed, "Shutter");
        if r.changed() {
            projectors.ask_all(outputs, Ask::Shutter(closed));
        }
        map_menu(ui, &r, ControlTarget::App(AppControl::ProjectorsShutter));
        if crate::ui::small_command(ui, icon::ARROWS_CLOCKWISE).on_hover_text("Check them all").clicked() {
            projectors.ask_all(outputs, Ask::Status);
        }
    });
}
