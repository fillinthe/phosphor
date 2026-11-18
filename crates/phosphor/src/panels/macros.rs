//! Dials (Resolume's dashboard) at the top of a layer, group, clip or MASTER in the Inspector: up to eight named
//! dials saved with it, each driving any settings anywhere (each with its own part of the dial, curve and range).
//! Link one by dragging (Alt-drag a slider onto a dial, or a dial onto a slider) or from its menu's list.

use phosphor_engine::{MAX_DIALS, MacroDial};

use super::*;

const HELP: &str = "Named dials saved with this, each moving any settings you link to it: Alt-drag a slider onto a dial (or a dial onto a slider), or pick them in the dial's menu. Map a knob to a dial and it plays them all.";

/// The Dials section for `owner` (a layer, group or clip id, or `MASTER_ID`).
pub(crate) fn dials_section(ui: &mut egui::Ui, owner: u64, dials: &mut Vec<MacroDial>) {
    let title = if dials.is_empty() { format!("{} Dials", icon::FADERS_HORIZONTAL) } else { format!("{} Dials · {}", icon::FADERS_HORIZONTAL, dials.len()) };
    let open = !dials.is_empty();
    crate::ui::section_help(ui, ("dials", owner), title, Some(HELP), open, |ui| {
        let mut remove = None;
        for (i, d) in dials.iter_mut().enumerate() {
            ui.push_id(("dial", i), |ui| {
                if dial_row(ui, owner, i, d) {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            dials.remove(i);
        }
        if dials.len() < MAX_DIALS && crate::ui::small_command(ui, label(icon::PLUS, "Dial")).on_hover_text(format!("Add a dial (up to {MAX_DIALS})")).clicked() {
            dials.push(MacroDial::numbered(dials.len() + 1));
        }
        if dials.is_empty() {
            hint(ui, "No dials yet.");
        }
    });
}

/// One dial: its bar (a control like any other: map it, drop on it) and its menu (name, what it drives, remove).
/// Returns true when it should be removed.
fn dial_row(ui: &mut egui::Ui, owner: u64, i: usize, d: &mut MacroDial) -> bool {
    let mut remove = false;
    ui.horizontal(|ui| {
        let menu_w = 44.0;
        let w = (ui.available_width() - menu_w - ui.spacing().item_spacing.x).max(60.0);
        Level::percent(&mut d.value, d.name.clone()).default(0.0).width(w).target(ControlTarget::Macro { owner, index: i as u8 }).tip("A dial: it moves everything linked to it").show(ui);
        let text = if d.targets.is_empty() { RichText::new(icon::LINK) } else { RichText::new(format!("{} {}", icon::LINK, d.targets.len())).color(accent()) };
        crate::ui::field_menu(ui, text, |ui| {
            ui.set_min_width(420.0);
            ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(WANT_TARGETS), true));
            let targets: std::sync::Arc<Vec<(ControlTarget, String)>> = ui.ctx().data(|data| data.get_temp(egui::Id::new(ALL_TARGETS))).unwrap_or_default();
            crate::ui::field(ui, "Name", |ui| {
                crate::ui::text_field(ui, &mut d.name, "Dial name", Some(200.0));
            });
            crate::ui::heading_help(ui, "Drives", "Each setting gets the dial's value through its own part of the dial, curve and from–to range.");
            binding_list(ui, ("dial_targets", owner, i), &targets, &mut d.targets);
            if crate::ui::small_danger(ui, label(icon::TRASH, "Remove dial")).clicked() {
                remove = true;
                ui.close();
            }
        })
        .response
        .on_hover_text(if d.targets.is_empty() { "Link settings to this dial".to_string() } else { format!("Drives {} setting{}", d.targets.len(), if d.targets.len() == 1 { "" } else { "s" }) });
    });
    remove
}
