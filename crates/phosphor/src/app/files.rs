//! File dialogs, dropped files and watched folders.

use super::*;

impl PhosphorApp {
    /// Grabs the output now; the save dialog opens as a sheet (the show keeps running).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn save_snapshot(&mut self) {
        let Some(engine) = &self.state.engine else { return };
        self.pending_snapshot = engine.read_output();
        self.state.dialogs.save_file(crate::dialogs::Purpose::SaveSnapshot, ("PNG", &["png"]), "Phosphor snapshot.png");
    }

    /// Answers from file dialogs, and files found in watched folders.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn handle_dialogs_and_folders(&mut self, now: f64) {
        use crate::dialogs::Purpose;
        let name_of = |p: &std::path::Path| p.file_name().map_or_else(|| p.to_string_lossy().to_string(), |n| n.to_string_lossy().to_string());
        let current_layout = to_text(&SavedLayout { name: "Show".into(), dock: self.dock.clone(), detached: self.detached.clone() });
        for (purpose, paths) in self.state.dialogs.poll() {
            let st = &mut self.state;
            match purpose {
                Purpose::LayerSource(id) => {
                    if let Some(p) = paths.first() {
                        st.launch_source(id, crate::media::spec_for_path(&p.to_string_lossy()), Vec::new(), name_of(p), now);
                    }
                }
                Purpose::ImportFiles => {
                    for p in &paths {
                        st.comp.add_media(&name_of(p), &p.to_string_lossy());
                    }
                }
                Purpose::WatchFolder => {
                    if let Some(p) = paths.first() {
                        st.folders.add(p.to_string_lossy().to_string());
                    }
                }
                Purpose::OpenProject => {
                    if let Some(p) = paths.first() {
                        match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|j| Composition::from_json(&j).map_err(|e| e.to_string())) {
                            Ok(comp) => {
                                st.load_composition(comp);
                                st.set_project_path(&p.to_string_lossy());
                            }
                            Err(e) => st.notice = Some(format!("Open failed: {e}")),
                        }
                    }
                }
                Purpose::SaveProject => {
                    if let Some(p) = paths.first() {
                        st.set_project_path(&p.to_string_lossy());
                        st.save_project();
                    }
                }
                Purpose::ExportShow => {
                    if let Some(dest) = paths.first() {
                        let screens = {
                            let map = st.output.lock().unwrap();
                            let displays = crate::screens::displays();
                            map.iter()
                                .filter_map(|(id, o)| {
                                    let d = displays.iter().find(|d| Some(d.id) == o.display)?;
                                    Some((*id, d.name.clone()))
                                })
                                .collect()
                        };
                        let req = crate::export::Request {
                            comp: st.comp.clone(),
                            name: st.export.name.clone(),
                            dest: dest.clone(),
                            zip: st.export.zip,
                            autostart: st.export.autostart,
                            screens,
                            layout: current_layout.clone(),
                        };
                        st.export.progress = Some(crate::export::start(req));
                    }
                }
                Purpose::SaveSnapshot => {
                    if let (Some(p), Some((w, h, px))) = (paths.first().cloned(), self.pending_snapshot.take()) {
                        std::thread::spawn(move || {
                            let _ = image::save_buffer(&p, &px, w, h, image::ColorType::Rgba8);
                        });
                    }
                }
                Purpose::StreamFile => {
                    if let Some(p) = paths.first() {
                        st.stream.settings.file_path = p.to_string_lossy().to_string();
                    }
                }
            }
        }
        if let Some(found) = self.state.folders.poll(now) {
            // Files that vanished from a watched folder leave the bin; new ones join it.
            let folders = self.state.folders.folders.clone();
            let paths: std::collections::HashSet<&str> = found.iter().map(|(_, p, _)| p.as_str()).collect();
            self.state.comp.media_bin.retain(|m| m.folder.is_empty() || !folders.contains(&m.folder) || paths.contains(m.path.as_str()));
            for (name, path, folder) in &found {
                self.state.comp.add_folder_media(name, path, folder);
            }
        }
    }

    pub(crate) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for (n, file) in dropped.into_iter().enumerate() {
            // Native: absolute path. Browser: just the file name (bytes arrive asynchronously).
            let path = file.path().to_string_lossy().to_string();
            let name = file.path().file_name().map_or(path.clone(), |n| n.to_string_lossy().to_string());
            if !media::is_supported(&name) {
                continue;
            }
            // First file replaces the selected layer's source; any others get new layers.
            let target = match self.state.selected {
                Some(id) if n == 0 && self.state.comp.layers.iter().any(|l| l.id == id) => id,
                _ => self.state.add_layer(),
            };
            let path = (!cfg!(target_arch = "wasm32")).then_some(path);
            let now = ctx.input(|i| i.time);
            self.state.launch_source(target, media::spec_for(&name, path), Vec::new(), name.clone(), now);
        }
    }
}
