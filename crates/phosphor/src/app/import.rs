//! Importing other apps' projects (Resolume compositions: pick decks, open as a new project) and finding
//! media that moved (search a folder by file name and relink).

use std::sync::mpsc::Receiver;

use super::*;
use crate::import::resolume::{AvcFile, read, to_composition};

/// A Resolume composition read and waiting for the deck choice.
pub struct AvcImport {
    pub file: AvcFile,
    pub chosen: Vec<bool>,
    /// From VDMX (pages) rather than Resolume (decks).
    pub vdmx: bool,
    /// Resolume's Advanced Output in use last (screens and slices), and whether to bring it too.
    pub screens: Option<crate::import::resolume_output::ScreenSetup>,
    pub with_screens: bool,
}

/// Paths found by a relink search: (old path, new path), and how many were looked for.
pub type Relinked = (Vec<(String, String)>, usize);

impl State {
    /// Reads a Resolume composition (.avc) or VDMX project (.vdmx5) and opens the deck / page chooser, or a
    /// MadMapper project (.mad) and opens its summary.
    pub fn begin_import(&mut self, path: &str) {
        if path.to_lowercase().ends_with(".mad") {
            match crate::import::madmapper::read(path) {
                Ok(file) => self.mad_import = Some(file),
                Err(e) => self.notice = Some(format!("Couldn't import {path}: {e}")),
            }
            return;
        }
        let vdmx = path.to_lowercase().ends_with(".vdmx5");
        let result = if vdmx { crate::import::vdmx::read(path) } else { read(path) };
        match result {
            Ok(file) => {
                let chosen = file.decks.iter().map(|d| d.files() > 0 || d.clips.iter().any(|c| c.generator.is_some())).collect();
                let screens = if vdmx { None } else { crate::import::resolume_output::current().and_then(|p| crate::import::resolume_output::read(&p).ok()) };
                let with_screens = screens.is_some();
                self.avc_import = Some(AvcImport { file, chosen, vdmx, screens, with_screens });
            }
            Err(e) => self.notice = Some(format!("Couldn't import {path}: {e}")),
        }
    }

    /// Opens the Resolume / VDMX project waiting in `avc_import` (its chosen decks / pages) as a new project. Says
    /// what came across in the notice.
    pub fn finish_avc_import(&mut self) {
        let Some(imp) = self.avc_import.take() else { return };
        let decks: Vec<usize> = imp.chosen.iter().enumerate().filter(|(_, c)| **c).map(|(i, _)| i).collect();
        let (mut comp, skipped) = to_composition(&imp.file, &decks);
        let screens = match imp.screens.as_ref().filter(|_| imp.with_screens) {
            Some(setup) => {
                let ids: Vec<u64> = comp.layers.iter().map(|l| l.id).collect();
                Some(crate::import::resolume_output::add_to(&mut comp, setup, &ids))
            }
            None => None,
        };
        self.load_composition(comp);
        self.project_path = None;
        self.clip_page = 0;
        let missing = self.missing_media().len();
        let mut msg = format!("Imported {} from {}.", imp.file.name, if imp.vdmx { "VDMX" } else { "Resolume" });
        if let Some((outs, slices, masks)) = screens {
            msg.push_str(&format!(" Advanced Output: {outs} outputs, {slices} slices, {masks} masks."));
        }
        if skipped > 0 {
            msg.push_str(&format!(" {skipped} clips without a media file (generators, audio, Resolume sources) were left out."));
        }
        let (stand_ins, left) = crate::import::looks::report(&imp.file.looks, &imp.file.master);
        if !stand_ins.is_empty() {
            msg.push_str(&format!(" Effects drawn by the nearest built-in one: {}.", stand_ins.join(", ")));
        }
        if !left.is_empty() {
            msg.push_str(&format!(" Effects left out: {}.", left.join(", ")));
        }
        if missing > 0 {
            msg.push_str(&format!(" {missing} media files aren't on this Mac — plug in the drive, or Media › Find missing media…"));
        }
        self.notice = Some(msg);
    }

    /// Brings the MadMapper project waiting in `mad_import` in: as a new project, or added to this one. Says what
    /// came across in the notice.
    pub fn finish_mad_import(&mut self, new: bool) {
        use crate::import::madmapper::{add_to, file_aspect, to_composition};
        let Some(file) = self.mad_import.take() else { return };
        let done = if new {
            let (comp, done) = to_composition(&file, &file_aspect);
            self.load_composition(comp);
            self.project_path = None;
            done
        } else {
            add_to(&mut self.comp, &file, &file_aspect)
        };
        let mut msg = format!("Imported {} from MadMapper: {} outputs, {} slices, {} layers.", file.name, done.outputs, done.slices, done.layers);
        if !done.stand_ins.is_empty() {
            msg.push_str(&format!(" Drawn by a built-in generator instead: {}.", done.stand_ins.join(", ")));
        }
        if done.faded > 0 {
            msg.push_str(&format!(" {} slices were faded out when it was saved (MadMapper's cues bring them up): raise their opacity in Mapping.", done.faded));
        }
        let missing = self.missing_media().len();
        if missing > 0 {
            msg.push_str(&format!(" {missing} media files aren't on this Mac — plug in the drive, or Media › Find missing media…"));
        }
        self.notice = Some(msg);
    }

    /// Media files the project uses that aren't where it says.
    pub fn missing_media(&self) -> Vec<String> {
        self.comp.media_paths().into_iter().filter(|p| !crate::media::sequence::exists(p)).collect()
    }

    /// Looks through `folder` (and everything inside) for the missing files by name, in the background.
    pub fn start_relink(&mut self, folder: std::path::PathBuf) {
        let missing = self.missing_media();
        if missing.is_empty() {
            self.notice = Some("No media is missing.".into());
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.relink = Some(rx);
        self.notice = Some(format!("Looking for {} missing files in {}…", missing.len(), folder.display()));
        std::thread::spawn(move || {
            let wanted: HashMap<String, Vec<String>> = missing.iter().fold(HashMap::new(), |mut m, p| {
                if let Some(name) = std::path::Path::new(p).file_name() {
                    m.entry(name.to_string_lossy().to_lowercase()).or_default().push(p.clone());
                }
                m
            });
            let mut found = Vec::new();
            let mut stack = vec![folder];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                for e in entries.flatten() {
                    let path = e.path();
                    if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                        continue;
                    }
                    if path.is_dir() {
                        stack.push(path);
                    } else if let Some(olds) = path.file_name().and_then(|n| wanted.get(&n.to_string_lossy().to_lowercase())) {
                        for old in olds {
                            found.push((old.clone(), path.to_string_lossy().to_string()));
                        }
                    }
                }
            }
            let _ = tx.send((found, missing.len()));
        });
    }

    /// Applies a finished relink search. Call once per frame.
    pub(crate) fn poll_relink(&mut self) {
        let Some(Ok((found, total))) = self.relink.as_ref().map(Receiver::try_recv) else { return };
        self.relink = None;
        let map: HashMap<String, String> = found.into_iter().collect();
        self.comp.map_paths(|p| map.get(p).cloned().unwrap_or_else(|| p.to_string()));
        // Files that changed path need reloading on their layers.
        self.media = Default::default();
        self.notice = Some(format!("Found {} of {} missing files.", map.len(), total));
    }
}

impl PhosphorApp {
    /// The "Import from Resolume / VDMX" deck chooser, and the MadMapper summary.
    pub(crate) fn import_windows(&mut self, ctx: &egui::Context) {
        self.mad_window(ctx);
        let Some(imp) = &mut self.state.avc_import else { return };
        let mut open = true;
        let mut go = false;
        let app = if imp.vdmx { "VDMX" } else { "Resolume" };
        crate::ui::dialog(ctx, format!("{} Import from {app}", icon::DOWNLOAD_SIMPLE)).open(&mut open).resizable(true).default_width(460.0).show(ctx, |ui| {
            ui.label(egui::RichText::new(&imp.file.name).strong());
            crate::ui::hint(ui, format!("{} × {} · {} layers{}", imp.file.width, imp.file.height, imp.file.layers.len(), imp.file.bpm.map_or(String::new(), |b| format!(" · {b:.1} BPM"))));
            ui.add_space(4.0);
            ui.label(format!("{} to bring in (they go side by side in the clip columns; each becomes a media collection):", if imp.vdmx { "Pages" } else { "Decks" }));
            crate::ui::column_scroll(ui).max_height(260.0).show(ui, |ui| {
                for (i, d) in imp.file.decks.iter().enumerate() {
                    let text = format!("{} — {} clips with media, {} columns", d.name, d.files(), d.columns);
                    crate::ui::toggle(ui, &mut imp.chosen[i], text);
                }
            });
            crate::ui::toolbar(ui, |ui| {
                if crate::ui::small_command(ui, "All").clicked() {
                    imp.chosen.iter_mut().for_each(|c| *c = true);
                }
                if crate::ui::small_command(ui, "None").clicked() {
                    imp.chosen.iter_mut().for_each(|c| *c = false);
                }
            });
            crate::ui::hint(
                ui,
                if imp.vdmx {
                    "The media bin's pages and clips come across, and the layers of the first preset: names, opacity, blend, fit and effects (ISF effects as the same shader; others as the nearest built-in one). VDMX's data sources and controller mappings don't."
                } else {
                    "Layers (opacity, blend, effects), clips (video and image files; generators as the nearest built-in one, text as text clips), the composition's effects, tempo and canvas size come across. Effects are drawn by the nearest built-in one."
                },
            );
            if let Some(setup) = &imp.screens {
                let slices: usize = setup.screens.iter().map(|s| s.slices.len()).sum();
                let text = format!("Advanced Output too: {} ({} screens, {slices} slices)", setup.name, setup.screens.len());
                crate::ui::toggle(ui, &mut imp.with_screens, text).on_hover_text("The screen setup Resolume used last: each screen becomes an output with its slices and masks");
            }
            ui.add_space(6.0);
            go = crate::ui::command_if(ui, imp.chosen.iter().any(|c| *c), crate::ui::label(icon::CHECK, "Open as a new project")).clicked();
        });
        if go {
            self.state.finish_avc_import();
        } else if !open {
            self.state.avc_import = None;
        }
    }

    /// A MadMapper project read: what comes across, then a new project or added to this one.
    fn mad_window(&mut self, ctx: &egui::Context) {
        let Some(file) = &self.state.mad_import else { return };
        let mut open = true;
        let (mut new, mut add) = (false, false);
        crate::ui::dialog(ctx, format!("{} Import from MadMapper", icon::DOWNLOAD_SIMPLE)).open(&mut open).resizable(true).default_width(460.0).show(ctx, |ui| {
            ui.label(egui::RichText::new(&file.name).strong());
            let [slices, groups, masks, lines, fixtures, _] = file.counts();
            crate::ui::counts(
                ui,
                &[
                    (file.outputs.len(), "{0} output", "{0} outputs"),
                    (slices, "{0} slice", "{0} slices"),
                    (groups, "{0} group", "{0} groups"),
                    (masks, "{0} mask", "{0} masks"),
                    (lines, "{0} line shape", "{0} line shapes"),
                ],
            );
            ui.add_space(4.0);
            ui.label("Pictures the slices show (each becomes a layer):");
            crate::ui::column_scroll(ui).max_height(200.0).show(ui, |ui| {
                for v in file.used_visuals() {
                    let missing = matches!(v.kind.as_str(), "Movie" | "Image") && !std::path::Path::new(v.url.trim()).exists();
                    let kind = match v.kind.as_str() {
                        "Movie" => "Movie",
                        "Image" => "Picture",
                        "Syphon" => "Syphon",
                        "Live" => "Camera",
                        _ => "Generator",
                    };
                    let text = format!("{kind} · {}{}", v.name, if missing { " (not on this Mac)" } else { "" });
                    crate::ui::hint(ui, text);
                }
            });
            let mut left = Vec::new();
            if file.cues > 0 {
                left.push(if file.cues == 1 { "1 cue".to_string() } else { format!("{} cues", file.cues) });
            }
            if file.mappings > 0 {
                left.push(if file.mappings == 1 { "1 MIDI / OSC mapping".to_string() } else { format!("{} MIDI / OSC mappings", file.mappings) });
            }
            if fixtures > 0 {
                left.push(if fixtures == 1 { "1 LED fixture".to_string() } else { format!("{fixtures} LED fixtures") });
            }
            crate::ui::help(ui, "Slices keep their corners, shapes, groups and the part of the picture they show. MadMapper's own generators are drawn by the nearest built-in one; its effects don't come across.");
            if !left.is_empty() {
                crate::ui::hint(ui, format!("Not brought in: {}.", left.join(", ")));
            }
            ui.add_space(6.0);
            crate::ui::toolbar(ui, |ui| {
                new = crate::ui::command(ui, crate::ui::label(icon::CHECK, "Open as a new project")).on_hover_text("A new project: the outputs, their slices and a layer for each picture").clicked();
                add = crate::ui::command(ui, crate::ui::label(icon::PLUS, "Add to this project")).on_hover_text("Adds the outputs (with their slices) and the layers to the project that's open").clicked();
            });
        });
        if new || add {
            self.state.finish_mad_import(new);
        } else if !open {
            self.state.mad_import = None;
        }
    }
}
