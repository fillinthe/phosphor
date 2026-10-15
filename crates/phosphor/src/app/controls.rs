//! Controller learning requests, the learning banner and keyboard shortcuts.

use super::*;

impl PhosphorApp {
    /// Mapped targets for the UI badges, Learn/Forget requests from right-click menus, and
    /// keyboard keys as controller input.
    pub(crate) fn control_requests(&mut self, ctx: &egui::Context) {
        let st = &mut self.state;
        let mapped: panels::MappedList = std::sync::Arc::new(st.comp.control_maps.iter().map(|m| (m.target.clone(), m.source.label())).collect());
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(panels::MAPPED_TARGETS), mapped));
        if let Some((target, source)) = ctx.data_mut(|d| {
            let id = egui::Id::new(panels::MAP_REQUEST);
            let t = d.get_temp::<(phosphor_engine::ControlTarget, phosphor_engine::ControlSource)>(id);
            d.remove::<(phosphor_engine::ControlTarget, phosphor_engine::ControlSource)>(id);
            t
        }) {
            st.comp.control_maps.retain(|m| m.target != target);
            st.comp.control_maps.push(phosphor_engine::ControlMap::new(source, target));
        }
        if let Some(t) = ctx.data_mut(|d| {
            let id = egui::Id::new(panels::LEARN_REQUEST);
            let t = d.get_temp::<phosphor_engine::ControlTarget>(id);
            d.remove::<phosphor_engine::ControlTarget>(id);
            t
        }) {
            st.control.learning = Some(t);
        }
        if let Some(t) = ctx.data_mut(|d| {
            let id = egui::Id::new(panels::UNLEARN_REQUEST);
            let t = d.get_temp::<phosphor_engine::ControlTarget>(id);
            d.remove::<phosphor_engine::ControlTarget>(id);
            t
        }) {
            st.comp.control_maps.retain(|m| m.target != t);
        }
        // Keys act as controller buttons (ignored while typing in a text field).
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let keys: Vec<(String, bool)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed, repeat: false, modifiers, .. } if !modifiers.command && *key != egui::Key::Escape => {
                        Some((key.name().to_string(), *pressed))
                    }
                    _ => None,
                })
                .collect()
        });
        for (key, pressed) in keys {
            let source = phosphor_engine::ControlSource::Key { key };
            let mapped = st.comp.control_maps.iter().any(|m| m.source == source);
            // Unmapped key releases don't need to reach the hub; presses do (for Learn).
            if pressed || mapped {
                st.control.inject(&mut st.comp, crate::control::Incoming { source, value: if pressed { 1.0 } else { 0.0 } });
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            // Esc cancels the control being learned, then leaves Map mode.
            if st.control.learning.take().is_none() {
                st.map_mode = false;
            }
        }
    }

    pub(crate) fn learning_banner(&mut self, ctx: &egui::Context) {
        if self.state.map_mode && self.state.control.learning.is_none() {
            egui::Area::new(egui::Id::new("map_banner")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 44.0)).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(egui::Color32::from_rgb(14, 34, 40)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{} MAP MODE", icon::PLUG)).strong().color(theme::ACCENT));
                        ui.label("Click any highlighted control, then move a knob, send OSC, or press a key.");
                        if ui.small_button("Done (Esc)").clicked() {
                            self.state.map_mode = false;
                        }
                    });
                });
            });
            return;
        }
        let Some(t) = self.state.control.learning.clone() else { return };
        let name = crate::control::target_label(&self.state.comp, &t);
        egui::Area::new(egui::Id::new("learn_banner")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 44.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(egui::Color32::from_rgb(40, 34, 14)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{} Learning: {name}", icon::BROADCAST)).color(egui::Color32::from_rgb(255, 205, 90)));
                    crate::ui::hint(ui, "move a knob, send OSC, or press a key");
                    if ui.small_button("Cancel (Esc)").clicked() {
                        self.state.control.learning = None;
                    }
                });
            });
        });
    }

    /// Keyboard shortcuts that act on the selected layer (ignored while typing in a field).
    pub(crate) fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let Some(sel) = self.state.selected else { return };
        if sel == phosphor_engine::MASTER_ID {
            return;
        }
        let digit = ctx.input(|i| {
            [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7, egui::Key::Num8, egui::Key::Num9]
                .iter()
                .position(|k| i.key_pressed(*k))
        });
        if let Some(col) = digit {
            let now = ctx.input(|i| i.time);
            let col = col + self.state.clip_page * panels::CLIP_PAGE_SIZE;
            self.state.trigger(Trigger::Column(col), now);
        }
        // ⌘D (duplicate) lives in the Edit menu.
        let (delete, toggle) = ctx.input(|i| (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace), i.key_pressed(egui::Key::Space)));
        if delete {
            panels::remove_layer(&mut self.state, sel);
        } else if toggle && let Some(l) = self.state.comp.layer_mut(sel) {
            l.enabled = !l.enabled;
        }
    }
}
