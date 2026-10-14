//! Output windows (borderless, per screen), mapping sync and the phone remote sync.

use super::*;

impl PhosphorApp {
    /// Phone/tablet remote: apply what remotes sent, then publish the state (≈20 times a second).
    pub(crate) fn remote_sync(&mut self, now: f64) {
        if !self.state.net.running() {
            return;
        }
        let pw = self.state.require_password.then(|| self.state.remote_password.clone());
        self.state.net.set_password(pw);
        for cmd in self.state.net.take_remote_commands() {
            crate::remote::apply(&mut self.state, &cmd, now);
        }
        if now - self.remote_published > 0.05 {
            self.remote_published = now;
            let json = crate::remote::state_json(&self.state);
            self.state.net.set_remote_state(json);
        }
    }

    /// Pulls corner drags made in the output window back into the composition.
    pub(crate) fn sync_mapping_from_output(&mut self) {
        let mut map = self.state.output.lock().unwrap();
        for (id, out) in map.iter_mut() {
            if out.mapping_changed {
                if let Some(o) = self.state.comp.outputs.iter_mut().find(|o| o.id == *id) {
                    o.mapping = out.mapping.clone();
                }
                self.state.selected_surface = out.selected_surface;
                self.state.mapping_output = *id;
                out.mapping_changed = false;
            }
        }
    }

    pub(crate) fn output_windows(&self, ctx: &egui::Context) {
        let open: Vec<(u64, String)> =
            self.state.output.lock().unwrap().iter().filter(|(_, o)| o.open).map(|(id, o)| (*id, o.name.clone())).collect();
        for (oid, name) in open {
            self.output_window(ctx, oid, &name);
        }
    }

    pub(crate) fn output_window(&self, ctx: &egui::Context, oid: u64, name: &str) {
        let id = egui::ViewportId::from_hash_of(("phosphor_output", oid));
        let shared = self.state.output.clone();
        ctx.show_viewport_deferred(
            id,
            // Borderless: nothing but picture, so a projector sees no title bar or menu bar.
            egui::ViewportBuilder::default()
                .with_title(format!("Phosphor · {name}"))
                .with_decorations(false)
                .with_inner_size([960.0, 540.0]),
            move |ui, _class| {
                let ctx = ui.ctx().clone();
                let mut map = shared.lock().unwrap();
                let Some(out) = map.get_mut(&oid) else { return };
                let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                out.fullscreen = fullscreen;
                if ctx.input(|i| i.key_pressed(egui::Key::E)) {
                    out.edit_mapping = !out.edit_mapping;
                }
                // Screen placement: leave fullscreen, move to the display, then fullscreen there.
                if let Some((d, step, wait)) = out.place.as_mut() {
                    *wait += 1;
                    match *step {
                        0 => {
                            if fullscreen {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                                *step = 1;
                            } else {
                                *step = 2;
                            }
                            *wait = 0;
                        }
                        1 if *wait > 45 => {
                            *step = 2;
                            *wait = 0;
                        }
                        2 => {
                            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(d.x + 40.0, d.y + 40.0)));
                            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(d.w * 0.6, d.h * 0.6)));
                            *step = 3;
                            *wait = 0;
                        }
                        3 if *wait > 10 => {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                            *step = 4;
                        }
                        _ => {}
                    }
                    if *step == 4 {
                        out.place = None;
                    }
                }
                if ctx.input(|i| i.viewport().close_requested()) {
                    out.open = false;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::F)) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    if fullscreen {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                    } else {
                        out.open = false;
                    }
                }
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
                let frame = panels::fit_rect(rect, out.aspect);
                if let Some(tex) = out.texture {
                    panels::paint_texture(ui.painter(), tex, frame);
                }
                if fullscreen {
                    ctx.set_cursor_icon(egui::CursorIcon::None);
                }
                if !out.edit_mapping {
                    // No title bar, so dragging the picture moves the window (to the projector).
                    let drag = ui.interact(rect, egui::Id::new("output_drag"), egui::Sense::drag());
                    if drag.drag_started() && !fullscreen {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag.double_clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
                    }
                    if !fullscreen {
                        ui.painter().text(
                            rect.left_bottom() + egui::vec2(12.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            "Drag to move · double-click or F = fullscreen · E = mapping handles · Esc = close",
                            egui::FontId::proportional(12.0),
                            Color32::from_white_alpha(110),
                        );
                    }
                }
                if ctx.input(|i| i.time) < out.identify_until {
                    let c = rect.center();
                    ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(140));
                    ui.painter().text(c, egui::Align2::CENTER_CENTER, out.number.to_string(), egui::FontId::proportional(rect.height() * 0.45), theme::ACCENT);
                    ui.painter().text(c + egui::vec2(0.0, rect.height() * 0.28), egui::Align2::CENTER_CENTER, &out.name, egui::FontId::proportional(28.0), Color32::WHITE);
                }
                if out.edit_mapping {
                    let OutputShared { mapping, selected_surface, mapping_changed, .. } = &mut *out;
                    if panels::mapping_handles(ui, frame, mapping, selected_surface, true) {
                        *mapping_changed = true;
                    }
                    ui.painter().text(
                        rect.left_bottom() + egui::vec2(12.0, -12.0),
                        egui::Align2::LEFT_BOTTOM,
                        "Mapping edit: drag corners · E hides handles · F fullscreen",
                        egui::FontId::proportional(13.0),
                        Color32::from_white_alpha(160),
                    );
                }
            },
        );
        ctx.request_repaint_of(id);
    }
}
