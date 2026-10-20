//! The collection header in the Media panel: rename, its watched folders, "Play on" a layer, delete.

use super::*;
use super::media::MediaAct;

/// Shown above the files while a collection is picked.
pub(crate) fn collection_header(ui: &mut egui::Ui, st: &mut State, name: &str, acts: &mut Vec<MediaAct>) {
    let count = st.comp.collection_items(name).len();
    let folders = st.comp.collection_folders_of(name);
    let layers: Vec<(u64, String)> = st.comp.layers.iter().rev().map(|l| (l.id, l.name.clone())).collect();
    let playing: Vec<String> = st.comp.layers.iter().filter(|l| l.collection.as_deref() == Some(name)).map(|l| l.name.clone()).collect();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{} {name}", icon::BOOKMARKS_SIMPLE)).strong());
            hint(ui, format!("{count} files"));
            ui.menu_button(label(icon::PENCIL_SIMPLE, "Rename"), |ui| {
                if let Some(new) = rename_menu(ui, ("rename_coll", name), name) {
                    acts.push(MediaAct::RenameCollection(name.to_string(), new));
                }
            });
            ui.menu_button(label(icon::PLAY, "Play on"), |ui| {
                hint(ui, "The layer plays this collection by itself: next / previous / random (Inspector)");
                for (id, lname) in &layers {
                    if ui.button(lname).clicked() {
                        acts.push(MediaAct::PlayCollection(name.to_string(), Some(*id)));
                        ui.close();
                    }
                }
                if ui.button(label(icon::PLUS, "A new layer")).clicked() {
                    acts.push(MediaAct::PlayCollection(name.to_string(), None));
                    ui.close();
                }
            })
            .response
            .on_hover_text("Put this collection on a layer: it steps through the files on its own");
            #[cfg(not(target_arch = "wasm32"))]
            if ui.button(label(icon::FOLDER_PLUS, "Add folder…")).on_hover_text("Every clip in that folder (and its subfolders) joins this collection, and new files as they arrive").clicked() {
                st.dialogs.pick_folder(crate::dialogs::Purpose::CollectionFolder(name.to_string()));
            }
            if ui.button(label(icon::TRASH, "Delete")).on_hover_text("The files stay in the media bin").clicked() {
                acts.push(MediaAct::DeleteCollection(name.to_string()));
            }
        });
        if !playing.is_empty() {
            hint(ui, format!("Playing on: {}", playing.join(", ")));
        }
        for f in folders {
            ui.horizontal(|ui| {
                let short = std::path::Path::new(&f).file_name().map_or(f.clone(), |n| n.to_string_lossy().into_owned());
                ui.label(RichText::new(format!("{} {short}", icon::FOLDER)).small()).on_hover_text(&f);
                if ui.small_button(icon::X).on_hover_text("Stop including this folder").clicked() {
                    acts.push(MediaAct::RemoveCollectionFolder(name.to_string(), f.clone()));
                }
            });
        }
        if count == 0 {
            hint(ui, "Empty: right-click files › Collections, or Add folder…");
        }
    });
}
