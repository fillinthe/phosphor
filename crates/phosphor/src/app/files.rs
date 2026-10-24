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

    /// The same for one output, after its mapping (as the projector gets it).
    #[cfg(not(target_arch = "wasm32"))]
    fn save_output_snapshot(&mut self, id: u64) {
        let Some(engine) = &self.state.engine else { return };
        self.pending_snapshot = engine.read_output_of(id);
        let name = self.state.comp.outputs.iter().find(|o| o.id == id).map_or_else(|| "Output".to_string(), |o| o.name.clone());
        self.state.dialogs.save_file(crate::dialogs::Purpose::SaveSnapshot, ("PNG", &["png"]), &format!("{name} snapshot.png"));
    }

    /// The layer's thumbnail now, saved next to the settings (clip thumbnails/<clip id>.png) as the clip's picture.
    #[cfg(not(target_arch = "wasm32"))]
    fn save_clip_thumbnail(&mut self, layer: u64, slot: usize) -> Result<(), String> {
        let (w, h, px) = self.state.engine.as_ref().and_then(|e| e.read_layer_thumbnail(layer)).ok_or("the layer has no picture (is it on?)")?;
        let dir = crate::show::storage_dir().ok_or("no settings folder")?.join("clip thumbnails");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let clip = self.state.comp.layer_mut(layer).and_then(|l| l.clips.get_mut(slot)).ok_or("that clip is gone")?;
        let path = dir.join(format!("{}.png", clip.id));
        image::save_buffer(&path, &px, w, h, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
        clip.thumb = path.to_string_lossy().to_string();
        // The same file gets a new picture: drop the old one from the cache.
        self.state.thumbs.forget(&clip.thumb);
        Ok(())
    }

    /// Answers from file dialogs, and files found in watched folders.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn handle_dialogs_and_folders(&mut self, now: f64) {
        use crate::dialogs::Purpose;
        if let Some(id) = self.state.snapshot_output.take() {
            self.save_output_snapshot(id);
        }
        // A reference picture's colours are ready: match the layer's (its picture now) to them.
        if let Some((layer, found)) = self.state.color_ref.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.state.color_ref = None;
            let now_px = self.state.engine.as_ref().and_then(|e| e.read_layer_thumbnail(layer));
            self.state.notice = Some(match (found, now_px) {
                (Ok(to), Some((_, _, px))) => {
                    let from = phosphor_engine::color_stats(&px);
                    match self.state.comp.add_color_match(layer, from, to) {
                        Some(_) => "Colour Match added: the layer's colours follow the picture (tweak it in its effects)".into(),
                        None => "That layer is gone".into(),
                    }
                }
                (Err(e), _) => format!("Couldn't read that picture: {e}"),
                (_, None) => "The layer has no picture to match (is it on?)".into(),
            });
        }
        if let Some((layer, slot)) = self.state.clip_thumb_request.take() {
            self.state.notice = Some(match self.save_clip_thumbnail(layer, slot) {
                Ok(()) => "The clip shows this picture in the grid now".into(),
                Err(e) => format!("Couldn't keep the picture: {e}"),
            });
        }
        let name_of = |p: &std::path::Path| p.file_name().map_or_else(|| p.to_string_lossy().to_string(), |n| n.to_string_lossy().to_string());
        // Watched folders first: this has to run every frame (it rescans every 10 s), not only when a dialog closes.
        // A collection's folders are watched too (an opened project may bring new ones).
        // Following's folder is watched while any account is followed (each account = a subfolder in Media).
        #[cfg(not(target_arch = "wasm32"))]
        if !self.state.media_view.following.accounts.is_empty()
            && let Some(f) = crate::online::following::folder()
        {
            let f = f.to_string_lossy().into_owned();
            if !self.state.folders.folders.contains(&f) {
                let _ = std::fs::create_dir_all(&f);
                self.state.folders.add(f);
            }
        }
        for f in &self.state.comp.collection_folders {
            if !self.state.folders.folders.contains(&f.folder) {
                self.state.folders.add(f.folder.clone());
            }
        }
        if let Some(found) = self.state.folders.poll(now) {
            // Files that vanished from a watched folder leave the bin; new ones join it.
            // (A folder that's missing, its drive unplugged or the network down, keeps its files: they come back.)
            let folders: Vec<String> = self.state.folders.folders.iter().filter(|f| !self.state.folders.missing(f)).cloned().collect();
            let paths: std::collections::HashSet<&str> = found.iter().map(|(_, p, _)| p.as_str()).collect();
            self.state.comp.media_bin.retain(|m| m.folder.is_empty() || !folders.contains(&m.folder) || paths.contains(m.path.as_str()));
            for (name, path, folder) in &found {
                if self.state.comp.add_folder_media(name, path, folder) {
                    // Downloaded files (Online, Following) keep who made them.
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(m) = self.state.comp.media_bin.last_mut() {
                        m.credit = crate::online::saved_credit(std::path::Path::new(path));
                    }
                }
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
                    // (The Media panel's sources show it under This Mac instead: no collection.)
                    if let Some(p) = paths.first()
                        && !st.media_view.classic
                    {
                        let folder = p.to_string_lossy().to_string();
                        st.folders.add(folder.clone());
                        st.media_view.show(crate::panels::MediaSource::Folder(folder.clone()), Some(folder));
                        st.notice = Some(format!("Watching \"{}\": new clips show up on their own", name_of(p)));
                    } else if let Some(p) = paths.first() {
                        let folder = p.to_string_lossy().to_string();
                        let name = st.comp.create_collection(&name_of(p));
                        st.comp.add_collection_folder(&name, &folder);
                        st.folders.add(folder);
                        st.media_view.show_collection(name.clone());
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
                Purpose::SliceFile(sid) => {
                    if let Some(p) = paths.first() {
                        let path = p.to_string_lossy().to_string();
                        let name = p.file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().to_string());
                        st.comp.show_file_on_slice(sid, &name, crate::media::spec_for_path(&path));
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
                Purpose::ImportFixtures => {
                    if let Some(p) = paths.first() {
                        let text = std::fs::read_to_string(p).map_err(|e| e.to_string());
                        // Read into a scratch list, then each gets a project id.
                        let mut read = phosphor_engine::DmxSettings::default();
                        let result = text.and_then(|t| read.import_csv(&t, || 0));
                        for mut f in read.fixtures {
                            f.id = st.comp.new_id();
                            st.comp.dmx.fixtures.push(f);
                        }
                        st.notice = Some(match result {
                            Ok(n) => format!("{n} fixtures from {}", name_of(p)),
                            Err(e) => format!("Couldn't read that patch list: {e}"),
                        });
                    }
                }
                Purpose::OutputBackground(id) => {
                    if let (Some(p), Some(o)) = (paths.first(), st.comp.outputs.iter_mut().find(|o| o.id == id)) {
                        o.background = p.to_string_lossy().into_owned();
                    }
                }
                Purpose::MergeProject => {
                    if let Some(p) = paths.first() {
                        let read = std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|j| Composition::from_json(&j).map_err(|e| e.to_string()));
                        st.notice = Some(match read {
                            Ok(other) => {
                                let m = st.comp.merge(other);
                                format!("Merged {}: {} layers, {} media files, {} cues, {} mappings (Undo takes it back out)", name_of(p), m.layers, m.media, m.cues, m.mappings)
                            }
                            Err(e) => format!("Couldn't merge that project: {e}"),
                        });
                    }
                }
                Purpose::CueCalendar => {
                    if let Some(p) = paths.first() {
                        st.comp.cues.calendar = p.to_string_lossy().into_owned();
                        let n = std::fs::read_to_string(p).map(|t| crate::calendar::parse(&t, 0).len()).unwrap_or(0);
                        st.notice = Some(format!("Calendar: {n} timed events; each starts the cue with its name"));
                    }
                }
                Purpose::TextFile(id) => {
                    if let Some(p) = paths.first() {
                        match std::fs::read_to_string(p) {
                            Ok(text) => {
                                if let Some(phosphor_engine::SourceSpec::Text(style)) = st.comp.layer_mut(id).map(|l| &mut l.source) {
                                    let by = style.chunk_by;
                                    style.set_parts(&text, by);
                                    st.notice = Some(format!("{} parts from {}", style.parts.len(), name_of(p)));
                                }
                            }
                            Err(e) => st.notice = Some(format!("Couldn't read that file: {e}")),
                        }
                    }
                }
                Purpose::MatchColour(id) => {
                    if let Some(p) = paths.first().cloned() {
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.state.color_ref = Some(rx);
                        let path = p.to_string_lossy().to_string();
                        std::thread::spawn(move || {
                            let stats = crate::media::decode_still(&path).map(|img| phosphor_engine::color_stats(img.thumbnail(320, 320).to_rgba8().as_raw()));
                            let _ = tx.send((id, stats));
                        });
                    }
                }
                Purpose::SaveDmxTake => {
                    if let (Some(p), Some(take)) = (paths.first(), st.dmx_rec.take.as_ref()) {
                        let p = p.with_extension("phdmx");
                        st.notice = Some(match std::fs::write(&p, take.to_bytes()) {
                            Ok(()) => format!("DMX take saved: {}", p.display()),
                            Err(e) => format!("Couldn't save the take: {e}"),
                        });
                    }
                }
                Purpose::OpenDmxTake => {
                    if let Some(p) = paths.first() {
                        match std::fs::read(p).map_err(|e| e.to_string()).and_then(|b| crate::dmx_record::Take::from_bytes(&b)) {
                            Ok(t) => {
                                st.notice = Some(format!("DMX take: {:.1} s from {} (press Play)", t.length(), name_of(p)));
                                st.dmx_rec.take = Some(t);
                            }
                            Err(e) => st.notice = Some(format!("Couldn't open that take: {e}")),
                        }
                    }
                }
                Purpose::ExportBank(id) => {
                    if let (Some(p), Some(json)) = (paths.first(), st.comp.export_bank(id)) {
                        let p = p.with_extension("json");
                        st.notice = Some(match std::fs::write(&p, json) {
                            Ok(()) => format!("Clips saved: {}", p.display()),
                            Err(e) => format!("Couldn't save the clips: {e}"),
                        });
                    }
                }
                Purpose::ImportBank(id) => {
                    if let Some(p) = paths.first() {
                        let read = std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|t| st.comp.import_bank(id, &t));
                        st.notice = Some(match read {
                            Ok(n) => format!("{n} clips from {}", name_of(p)),
                            Err(e) => format!("Couldn't read those clips: {e}"),
                        });
                    }
                }
                Purpose::ExportMappingSvg(id) => {
                    if let (Some(p), Some(o)) = (paths.first(), st.comp.outputs.iter().find(|o| o.id == id)) {
                        let svg = crate::mapping_svg::export(&o.mapping, o.width, o.height);
                        // .png = the drawing as a picture at the output's size; .pdf = a vector page; else SVG.
                        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                        let p = if ext == "png" || ext == "pdf" { p.clone() } else { p.with_extension("svg") };
                        let written = match ext.as_str() {
                            "png" => crate::media::render_svg_sized(svg.as_bytes(), o.width.max(o.height) as f32).and_then(|img| img.save(&p).map_err(|e| e.to_string())),
                            "pdf" => std::fs::write(&p, crate::mapping_svg::export_pdf(&o.mapping, o.width, o.height)).map_err(|e| e.to_string()),
                            _ => std::fs::write(&p, svg).map_err(|e| e.to_string()),
                        };
                        st.notice = Some(match written {
                            Ok(()) => format!("Mapping saved: {}", p.display()),
                            Err(e) => format!("Couldn't save the drawing: {e}"),
                        });
                    }
                }
                Purpose::ImportMasksSvg(id) => {
                    if let Some(p) = paths.first() {
                        match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|t| crate::mapping_svg::import(&t)) {
                            Ok(drawing) => {
                                let ids: Vec<u64> = drawing.shapes.iter().chain(&drawing.lines).map(|_| st.comp.new_id()).collect();
                                let mut ids = ids.into_iter();
                                let mut lines = 0;
                                if let Some(o) = st.comp.outputs.iter_mut().find(|o| o.id == id) {
                                    o.mapping.enabled = true;
                                    for (k, shape) in drawing.shapes.iter().enumerate() {
                                        let m = o.mapping.add_mask(ids.next().unwrap_or_default());
                                        m.points.clone_from(shape);
                                        m.name = format!("{} {}", name_of(p), k + 1);
                                    }
                                    for line in &drawing.lines {
                                        lines += o.mapping.add_line_stroke(ids.next().unwrap_or_default(), line).is_some() as usize;
                                    }
                                }
                                st.notice = Some(format!("{} masks and {lines} line slices from {}", drawing.shapes.len(), name_of(p)));
                            }
                            Err(e) => st.notice = Some(format!("Couldn't read that drawing: {e}")),
                        }
                    }
                }
                Purpose::ExportCues => {
                    if let Some(p) = paths.first() {
                        let p = p.with_extension("csv");
                        st.notice = Some(match std::fs::write(&p, st.comp.cues.to_csv()) {
                            Ok(()) => format!("Cue list saved: {}", p.display()),
                            Err(e) => format!("Couldn't save the cue list: {e}"),
                        });
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                Purpose::LibraryFolder => {
                    if let Some(p) = paths.first() {
                        st.media_view.following.library = Some(p.to_string_lossy().into_owned());
                        st.media_view.following.save();
                        st.notice = Some(format!("Kept clips go to {}", p.display()));
                    }
                }
                Purpose::ImportCues => {
                    if let Some(p) = paths.first() {
                        let read = std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|t| st.comp.cues.apply_csv(&t));
                        st.notice = Some(match read {
                            Ok(n) => format!("Names and timings read into {n} cue(s) (the looks stay as recorded)"),
                            Err(e) => format!("Couldn't read that cue list: {e}"),
                        });
                    }
                }
                Purpose::ImportTouchOsc => {
                    if let Some(p) = paths.first() {
                        let result = std::fs::read(p).map_err(|e| e.to_string()).and_then(|b| crate::tosc::import(&mut st.comp, &b));
                        st.notice = Some(match result {
                            Ok(i) => format!("TouchOSC layout in: {} page(s), {} controls, {} OSC addresses mapped (Control Surface)", i.pages, i.widgets, i.mappings),
                            Err(e) => format!("Couldn't read that layout: {e}"),
                        });
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
                Purpose::RenderFile => {
                    if let Some(p) = paths.first() {
                        st.offline_setup.path = p.to_string_lossy().to_string();
                    }
                }
                Purpose::TimelineSound => {
                    if let Some(p) = paths.first() {
                        let path = p.to_string_lossy().to_string();
                        let sound = st.comp.timeline.sound.get_or_insert_with(Default::default);
                        sound.path = path;
                    }
                }
                Purpose::RecorderFile(i) => {
                    if let (Some(p), Some(r)) = (paths.first(), st.recorders.get_mut(i)) {
                        r.settings.file_path = p.to_string_lossy().to_string();
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
        // Onto the Mapping Studio's canvas: a slice there for each file, shaped like its picture.
        let studio = ctx.data(|d| d.get_temp::<(u64, egui::Rect, egui::Rect, u64)>(egui::Id::new(crate::panels::STUDIO_DROP)));
        if let (Some(p), Some((pass, area, frame, oid))) = (drop_point(ctx), studio)
            && ctx.cumulative_pass_nr() <= pass + 2
            && area.contains(p)
        {
            let mut made = 0;
            for (k, (name, path)) in files.iter().enumerate() {
                let at = [((p.x - frame.left()) / frame.width()) + k as f32 * 0.04, ((p.y - frame.top()) / frame.height()) + k as f32 * 0.04];
                // A picture's own shape; movies are taken as 16:9 (their size is known once they open).
                #[cfg(not(target_arch = "wasm32"))]
                let aspect = path.as_deref().filter(|_| media::is_picture(name)).and_then(|p| image::image_dimensions(p).ok()).map_or(16.0 / 9.0, |(w, h)| w as f32 / h.max(1) as f32);
                #[cfg(target_arch = "wasm32")]
                let aspect = 16.0 / 9.0;
                if let Some(sid) = st.comp.add_slice_with_file(oid, at, aspect, &media::clip_name(name), media::spec_for(name, path.clone())) {
                    st.selected_surface = Some(sid);
                    made += 1;
                }
            }
            st.notice = Some(format!("{made} slice{} added where you dropped", if made == 1 { "" } else { "s" }));
            return;
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
