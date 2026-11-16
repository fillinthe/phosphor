//! Links: one list of what drives what, grouped by the thing doing the driving (VDMX's data-source mappings):
//! controller inputs, controls followed by other controls, Control Surface widgets and shared modulators. Follows
//! and widget targets are edited in place (part of the fader, value effects, range); "show" jumps to a control.

use super::*;

pub(crate) fn links(ui: &mut egui::Ui, st: &mut State) {
    let count = st.comp.follows.len();
    let title = if count == 0 { format!("{} Links: what drives what", icon::LINK) } else { format!("{} Links: what drives what ({count} following)", icon::LINK) };
    crate::ui::section_help(ui, "control_links", title, Some("Everything that moves something else, by source. To make one control follow another: right-click the follower › Follow another control. To link a knob: right-click › Listen."), false, |ui| {
        let targets = crate::control::all_targets(&st.comp);
        let name = |t: &ControlTarget| targets.iter().find(|(x, _)| x == t).map_or_else(|| "(removed)".to_string(), |(_, n)| n.clone());
        let mut show: Option<(Tab, Option<u64>, Option<u64>)> = None;
        let comp = &mut st.comp;

        // Controls followed by other controls: edited here.
        let mut sources: Vec<ControlTarget> = Vec::new();
        for f in &comp.follows {
            if !sources.contains(&f.source) {
                sources.push(f.source.clone());
            }
        }
        let places: Vec<_> = comp.follows.iter().map(|f| super::control::where_is(comp, &f.to.target)).collect();
        let mut remove = None;
        for source in &sources {
            crate::ui::heading(ui, &name(source));
            for (i, f) in comp.follows.iter_mut().enumerate().filter(|(_, f)| f.source == *source) {
                crate::ui::toolbar(ui, |ui| {
                    ui.label(icon::ARROW_RIGHT);
                    ui.label(crate::ui::short_name(&name(&f.to.target), 34));
                    let place = places.get(i).copied().flatten();
                    if crate::ui::small_command_if(ui, place.is_some(), icon::CROSSHAIR).on_hover_text("Show this control (selects its layer and opens its panel)").clicked() {
                        show = place;
                    }
                    binding_editor(ui, &mut f.to);
                    if crate::ui::small_danger(ui, icon::TRASH).on_hover_text("Stop following").clicked() {
                        remove = Some(i);
                    }
                });
            }
        }
        if let Some(i) = remove {
            comp.follows.remove(i);
        }

        // Control Surface widgets: each target's own part, effects and range.
        let mut any_widget = false;
        for (k, w) in comp.surface.widgets.iter_mut().enumerate().filter(|(_, w)| !w.bindings.is_empty() || !w.bindings_y.is_empty()) {
            any_widget = true;
            let label = if w.label.is_empty() { format!("{} (surface)", w.kind.name()) } else { format!("{} (surface)", w.label) };
            crate::ui::heading(ui, &label);
            binding_list(ui, ("links_w", k), &targets, &mut w.bindings);
            if !w.bindings_y.is_empty() {
                hint(ui, "Up / down:");
                binding_list(ui, ("links_wy", k), &targets, &mut w.bindings_y);
            }
        }

        // Controller inputs: one line each (edit them under Mappings below).
        let mut inputs: Vec<(String, Vec<String>)> = Vec::new();
        for m in comp.control_maps.iter().filter(|m| m.enabled) {
            let (source, target) = (m.source.label(), name(&m.target));
            match inputs.iter_mut().find(|(s, _)| *s == source) {
                Some((_, list)) => list.push(target),
                None => inputs.push((source, vec![target])),
            }
        }
        // Shared modulators and what follows them.
        for m in &comp.modulators {
            let followers = comp.modulator_followers(m.id);
            if !followers.is_empty() {
                inputs.push((format!("{} (modulator)", m.name), followers));
            }
        }
        for (source, list) in &inputs {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(source).strong());
                ui.label(icon::ARROW_RIGHT);
                ui.label(list.join(", "));
            });
        }
        if sources.is_empty() && inputs.is_empty() && !any_widget {
            crate::ui::empty_note(ui, "Nothing drives anything yet.");
        }
        if let Some((tab, item, slice)) = show {
            if item.is_some() {
                st.selected = item;
            }
            if slice.is_some() {
                st.selected_surface = slice;
            }
            st.open_tab = Some(tab);
        }
    });
}
