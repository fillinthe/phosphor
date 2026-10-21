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
        // Watched folders first: this has to run every frame (it rescans every 10 s), not only when a dialog closes.
        // A collection's folders are watched too (an opened project may bring new ones).
        for f in &self.state.comp.collection_folders {
            if !self.state.folders.folders.contains(&f.folder) {
                self.state.folders.add(f.folder.clone());
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
        let dialogs = self.state.dialogs.poll();
        if dialogs.is_empty() {
            return;
        }
        // The layout text is only needed by an export; dialogs rarely finish, so build it then, not every frame.
        let current_layout = to_text(&SavedLayout { name: "Show".into(), dock: self.dock.clone(), detached: self.detached.clone() });
        for (purpose, paths) in dialogs {
            let st = &mut self.state;
            match purpose {
                Purpose::LayerSource(id) => {
                    if let Some(p) = paths.first() {
                        st.launch_source(id, crate::media::spec_for_path(&p.to_string_lossy()), Vec::new(), name_of(p), now);
                    }
                }
                Purpose::ImportFiles => {
                    // Importing while a collection is showing puts the files in it (else you'd not see them).
                    let into = match &st.media_view.filter {
                        crate::panels::MediaFilter::Collection(c) => Some(c.clone()),
                        _ => None,
                    };
                    for p in &paths {
                        st.comp.add_media_to(&name_of(p), &p.to_string_lossy(), into.as_deref());
                    }
                    if let Some(c) = into {
                        st.notice = Some(format!("{} file(s) added to \"{c}\"", paths.len()));
                    }
                }
                Purpose::WatchFolder => {
                    // A watched folder is a collection (like a VDMX media bin page): named after the folder, every
                    // clip in it and new ones as they arrive, ready to Play on a layer.
                    if let Some(p) = paths.first() {
                        let folder = p.to_string_lossy().to_string();
                        let name = st.comp.create_collection(&name_of(p));
                        st.comp.add_collection_folder(&name, &folder);
                        st.folders.add(folder);
                        st.media_view.filter = crate::panels::MediaFilter::Collection(name.clone());
                        st.notice = Some(format!("\"{name}\": every clip in that folder, and new ones as they arrive (Play on, to put it on a layer)"));
                    }
                }
                Purpose::CollectionFolder(name) => {
                    if let Some(p) = paths.first() {
                        let folder = p.to_string_lossy().to_string();
                        st.comp.add_collection_folder(&name, &folder);
                        st.folders.add(folder);
                        st.notice = Some(format!("\"{name}\" now has every clip in that folder, and new ones as they arrive"));
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
                            lock: st.export.lock.then(|| st.export.lock_password.clone()),
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
                Purpose::ImportResolume => {
                    if let Some(p) = paths.first() {
                        st.begin_resolume_import(&p.to_string_lossy());
                    }
                }
                Purpose::StagePhoto(oid) => {
                    if let (Some(p), Some(o)) = (paths.first(), st.comp.outputs.iter_mut().find(|o| o.id == oid)) {
                        o.stage_photo = p.to_string_lossy().to_string();
                    }
                }
                Purpose::Model3d(sid) => {
                    if let Some(p) = paths.first() {
                        let path = p.to_string_lossy().to_string();
                        phosphor_engine::mapping::forget_obj(&path);
                        let loaded = phosphor_engine::mapping::obj_mesh(&path);
                        for o in &mut st.comp.outputs {
                            if let Some(m) = o.mapping.surfaces.iter_mut().find(|s| s.id == sid).and_then(|s| s.model.as_mut()) {
                                m.path = path.clone();
                            }
                        }
                        st.notice = Some(match loaded.as_ref() {
                            Ok(mesh) => format!("{}: {} triangles", name_of(p), mesh.tris.len()),
                            Err(e) => format!("Couldn't read {}: {e}", name_of(p)),
                        });
                    }
                }
                Purpose::RelinkFolder => {
                    if let Some(p) = paths.first() {
                        st.start_relink(p.clone());
                    }
                }
                Purpose::TouchOsc => {
                    if let Some(p) = paths.first() {
                        let p = p.with_extension("tosc");
                        st.notice = Some(match std::fs::write(&p, crate::tosc::export(&st.comp)) {
                            Ok(()) => format!("TouchOSC layout saved: {} (open it in TouchOSC, then add a UDP connection to this Mac)", p.display()),
                            Err(e) => format!("Couldn't save the TouchOSC layout: {e}"),
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
    }

    /// Files and folders dropped from the Finder:
    /// - onto a clip cell: they fill that row from there (the grid grows);
    /// - one file elsewhere: it plays on the selected layer (or a new one);
    /// - several files or a folder elsewhere: one new layer with them as its clips, the first playing.
    ///
    /// Everything also goes into the media bin.
    pub(crate) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        let now = ctx.input(|i| i.time);
        let files = dropped_media(&dropped);
        if files.is_empty() {
            self.state.notice = Some("Nothing Phosphor can play in what was dropped (movies and pictures)".into());
            return;
        }
        let st = &mut self.state;
        for (name, path) in &files {
            if let Some(p) = path {
                st.comp.add_media(name, p);
            }
        }
        let items: Vec<(String, phosphor_engine::SourceSpec)> = files.iter().map(|(name, path)| (media::clip_name(name), media::spec_for(name, path.clone()))).collect();
        // A track dropped in with no audio input open: listen to the clips, so the visuals follow it right away.
        if files.iter().any(|(name, _)| media::is_sound_file(name)) && !st.audio.is_open() && !st.audio.is_clips() && st.audio.current().is_none() {
            st.audio.use_clips();
            st.notice = Some("Audio source set to Clip sound: the visuals follow what you play (Audio panel to change)".into());
        }
        let cell = drop_point(ctx).and_then(|p| st.clip_cells.iter().find(|(r, ..)| r.contains(p)).map(|(_, l, s)| (*l, *s)));
        if let Some((layer, slot)) = cell {
            let n = st.comp.place_clips(layer, slot, items);
            #[cfg(not(target_arch = "wasm32"))]
            st.auto_match_new(layer, slot, n);
            st.selected = Some(layer);
            st.notice = Some(format!("{n} clip{} added to the row", if n == 1 { "" } else { "s" }));
        } else if files.len() == 1 {
            let target = match st.selected {
                Some(id) if st.comp.layers.iter().any(|l| l.id == id) => id,
                _ => st.add_layer(),
            };
            let (name, path) = files[0].clone();
            let spec = media::spec_for(&name, path);
            #[cfg(not(target_arch = "wasm32"))]
            st.auto_match_source(target, &spec);
            st.launch_source(target, spec, Vec::new(), name, now);
        } else {
            let layer = st.add_layer();
            let n = st.comp.place_clips(layer, 0, items);
            #[cfg(not(target_arch = "wasm32"))]
            st.auto_match_new(layer, 0, n);
            st.selected = Some(layer);
            st.clip_page = 0;
            st.trigger(Trigger::Clip { layer, slot: 0 }, now);
            st.notice = Some(format!("New layer with {n} clips (drop onto a clip cell to fill a row there instead)"));
        }
    }
}

/// The playable files in a drop, folders opened (two levels, sorted by name): (file name, path).
fn dropped_media(dropped: &[egui::DroppedFileHandle]) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for file in dropped {
        let path = file.path();
        #[cfg(not(target_arch = "wasm32"))]
        if path.is_dir() {
            folder_media(path, 2, &mut out);
            continue;
        }
        // Native: the full path. Browser: just the name (the bytes arrive separately).
        let name = path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
        if media::is_supported(&name) {
            out.push((name, (!cfg!(target_arch = "wasm32")).then(|| path.to_string_lossy().into_owned())));
        }
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn folder_media(dir: &std::path::Path, depth: usize, out: &mut Vec<(String, Option<String>)>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<std::path::PathBuf> = read.flatten().map(|e| e.path()).collect();
    entries.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    // Numbered pictures (frame_0001.png…) come in as one image-sequence clip each.
    for (name, path) in media::sequence::take_sequences(dir, &mut entries) {
        out.push((name, Some(path)));
    }
    for p in entries {
        let name = p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if depth > 1 {
                folder_media(&p, depth - 1, out);
            }
        } else if media::is_supported(&name) {
            out.push((name, Some(p.to_string_lossy().into_owned())));
        }
    }
}

/// Where in the window a drop landed (egui points). macOS sends no mouse moves during a Finder drag, so ask
/// the system for the mouse instead of trusting egui's last pointer position.
fn drop_point(ctx: &egui::Context) -> Option<egui::Pos2> {
    if let Some((x, y)) = crate::screens::mouse_location() {
        let inner = ctx.input(|i| i.viewport().inner_rect)?;
        let zoom = ctx.zoom_factor();
        return Some(egui::pos2(x / zoom, y / zoom) - inner.min.to_vec2());
    }
    ctx.input(|i| i.pointer.hover_pos())
}

/// Where files being dragged over the window are (None when nothing is being dragged in).
pub(crate) fn file_drag_point(ctx: &egui::Context) -> Option<egui::Pos2> {
    if ctx.input(|i| i.raw.hovered_files.is_empty()) {
        return None;
    }
    drop_point(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_open_to_their_media_sorted() {
        let dir = std::env::temp_dir().join(format!("phosphor_drop_{}", std::process::id()));
        let sub = dir.join("More");
        std::fs::create_dir_all(&sub).unwrap();
        for f in ["b.mov", "A.png", "notes.txt", ".hidden.mov"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        std::fs::write(sub.join("c.mp4"), b"").unwrap();
        let mut out = Vec::new();
        folder_media(&dir, 2, &mut out);
        let names: Vec<&str> = out.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["A.png", "b.mov", "c.mp4"]);
        assert!(out.iter().all(|(_, p)| p.as_deref().is_some_and(|p| p.starts_with(dir.to_str().unwrap()))));
        assert_eq!(media::clip_name("b.mov"), "b");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
