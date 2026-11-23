//! The Control Surface's preset bar: save the page's widget values by name, recall them with a fade, step through
//! them (mappable: a pick across the presets, and Next). Engine side: `phosphor_engine::SurfacePreset`.

use super::*;

/// The preset bar of the page showing. Shown while editing or once the page has presets.
pub(crate) fn preset_bar(ui: &mut egui::Ui, st: &mut State, edit: bool, now: f64) {
    let page = st.comp.surface.current_page();
    let list = st.comp.surface.page_presets(page);
    if list.is_empty() && !edit {
        return;
    }
    let mut recall = None;
    let mut remove = None;
    let mut update = None;
    ui.horizontal_wrapped(|ui| {
        let title = crate::ui::hint(ui, "Presets");
        map_menu(ui, &title, ControlTarget::App(phosphor_engine::AppControl::SurfacePreset));
        for &i in &list {
            let lit = st.comp.surface.last_preset == Some(i);
            let r = crate::ui::pick_item(ui, lit, &st.comp.surface.presets[i].name).on_hover_text("Click: this page's widgets go to the values saved here (faders glide over the fade time). Right-click: rename, save again, delete.");
            if r.clicked() {
                recall = Some(i);
            }
            r.context_menu(|ui| {
                crate::ui::rename_item(ui, ("surface_preset", i), &st.comp.surface.presets[i].name);
                if crate::ui::command(ui, label(icon::FLOPPY_DISK, "Save the values now")).on_hover_text("This preset takes the widgets' values as they are now").clicked() {
                    update = Some(i);
                    ui.close();
                }
                if crate::ui::danger_button(ui, label(icon::TRASH, "Delete")).clicked() {
                    remove = Some(i);
                    ui.close();
                }
            });
            if let Some(name) = crate::ui::renamed(ui.ctx(), ("surface_preset", i)) {
                st.comp.surface.presets[i].name = name;
            }
        }
        if !list.is_empty() {
            let next = crate::ui::small_command(ui, label(icon::SKIP_FORWARD, "Next")).on_hover_text("The next preset of this page (map it to a pad or key)");
            map_menu(ui, &next, ControlTarget::App(phosphor_engine::AppControl::SurfacePresetNext));
            if next.clicked() {
                recall = st.comp.surface.next_preset();
            }
        }
        if crate::ui::small_command(ui, label(icon::PLUS, "Save")).on_hover_text("Save this page's widget values as a new preset").clicked() {
            let name = format!("Preset {}", list.len() + 1);
            st.comp.surface.save_preset(name);
        }
        let sf = &mut st.comp.surface;
        guarded!(ui, sf.preset_fade, egui::DragValue::new(&mut sf.preset_fade).range(0.0..=30.0).speed(0.02).max_decimals(2).prefix("fade ").suffix(" s"))
            .on_hover_text("Faders, sliders and pads glide this long into a preset (0 = they jump); switches and choices change at once");
    });
    if let Some(i) = update {
        st.comp.surface.update_preset(i);
    }
    if let Some(i) = remove {
        st.comp.surface.remove_preset(i);
    }
    if let Some(i) = recall {
        st.recall_surface_preset(i, now);
    }
}
