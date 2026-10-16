//! Exported shows: auto-start on launch, and the Export Show window.

use super::*;

impl PhosphorApp {
    /// Exported shows open their outputs by themselves, on the screens they were exported with.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn start_show(&mut self, _now: f64) {
        let Some(frame) = self.show_start.as_mut() else { return };
        *frame += 1;
        // Give the main window a moment to settle first.
        if *frame < 30 {
            return;
        }
        self.show_start = None;
        let Some(b) = crate::show::bundled() else { return };
        if !b.manifest.autostart {
            return;
        }
        let displays = crate::screens::displays();
        self.state.displays = displays.clone();
        let others: Vec<_> = displays.iter().filter(|d| !d.main).cloned().collect();
        let outputs: Vec<u64> = self.state.comp.outputs.iter().filter(|o| o.enabled).map(|o| o.id).collect();
        for (i, id) in outputs.iter().enumerate() {
            let saved = b.manifest.screens.iter().find(|(o, _)| o == id).and_then(|(_, name)| displays.iter().find(|d| &d.name == name));
            // Same screen as when exported; otherwise the extra screens in order; otherwise this one.
            let target = saved.cloned().or_else(|| others.get(i).cloned()).or_else(|| (i == 0).then(|| displays.first().cloned()).flatten());
            match target {
                Some(d) => self.state.send_output_id_to(*id, d),
                None => self.state.toggle_output_window(*id),
            }
        }
    }

    /// Export Show: name, options, then a folder; shows progress until done.
    pub(crate) fn export_window(&mut self, ctx: &egui::Context) {
        let st = &mut self.state;
        if !st.export.open {
            return;
        }
        let mut open = true;
        egui::Window::new(format!("{} Export Show", icon::PACKAGE)).open(&mut open).collapsible(false).resizable(false).default_width(420.0).show(ctx, |ui| {
            #[cfg(target_arch = "wasm32")]
            ui.label("Export Show is available in the Mac app.");
            #[cfg(not(target_arch = "wasm32"))]
            {
                let progress = st.export.progress.as_ref().map(|p| p.lock().unwrap().clone());
                match progress {
                    Some(p) if !p.done => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(&p.step);
                        });
                        crate::ui::hint(ui, "The show keeps running while this works.");
                    }
                    Some(p) => {
                        if let Some(app) = &p.output {
                            ui.label(egui::RichText::new(format!("{} Exported {}", icon::CHECK_CIRCLE, app.file_name().unwrap_or_default().to_string_lossy())).strong().color(theme::ACCENT));
                            ui.label(format!("{} media file(s), {:.1} MB of media", p.media_files, p.bytes as f64 / 1e6));
                            if let Some(z) = &p.zip {
                                ui.label(format!("Zip: {}", z.display()));
                            }
                            crate::ui::hint(ui, format!("Copy it to the other Mac and double-click. First time macOS says it can't verify it: System Settings {} Privacy & Security {} Open Anyway.", icon::ARROW_RIGHT, icon::ARROW_RIGHT));
                            ui.horizontal(|ui| {
                                if ui.button(label_text(icon::FOLDER_OPEN, "Show in Finder")).clicked() {
                                    let _ = std::process::Command::new("open").arg("-R").arg(app).spawn();
                                }
                                if ui.button("Export again").clicked() {
                                    st.export.progress = None;
                                    st.export.summary = None;
                                }
                            });
                        }
                        if let Some(e) = &p.error {
                            ui.colored_label(egui::Color32::from_rgb(255, 120, 90), e);
                            if p.output.is_none() && ui.button("Try again").clicked() {
                                st.export.progress = None;
                            }
                        }
                    }
                    None => {
                        ui.label("Packs this project, every media file, the shaders it uses, and all MIDI/OSC/key mappings and outputs into one app that runs on another Mac — no Phosphor install needed.");
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Show name");
                            ui.text_edit_singleline(&mut st.export.name);
                        });
                        ui.checkbox(&mut st.export.autostart, "Open the outputs full screen when the show starts");
                        ui.checkbox(&mut st.export.zip, "Also make a .zip (easier to send)");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut st.export.lock, "Open locked (play only)").on_hover_text("No menus or editing on the other Mac: just the preview, clips, keys, controllers and outputs");
                            ui.add_enabled(st.export.lock, egui::TextEdit::singleline(&mut st.export.lock_password).password(true).hint_text("unlock password (optional)").desired_width(170.0));
                        });
                        let (files, bytes, missing) = *st.export.summary.get_or_insert_with(|| crate::export::summary(&st.comp));
                        crate::ui::hint(ui, format!("{files} media file(s), {:.1} MB · {} output(s) · {} mapping(s)", bytes as f64 / 1e6, st.comp.outputs.len(), st.comp.control_maps.len()));
                        if missing > 0 {
                            ui.colored_label(egui::Color32::from_rgb(255, 196, 70), format!("{missing} file(s) can't be found and will be left out"));
                        }
                        ui.add_space(4.0);
                        if ui.add_enabled(!st.export.name.trim().is_empty(), egui::Button::new(label_text(icon::PACKAGE, "Choose folder and export…"))).clicked() {
                            st.dialogs.pick_folder(crate::dialogs::Purpose::ExportShow);
                        }
                    }
                }
            }
        });
        if !open {
            st.export.open = false;
            #[cfg(not(target_arch = "wasm32"))]
            if st.export.progress.as_ref().is_some_and(|p| p.lock().unwrap().done) {
                st.export.progress = None;
            }
        }
    }
}
