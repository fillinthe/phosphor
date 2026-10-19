//! Media bin: favourites, collections, search, sort, thumbnails.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum MediaFilter {
    #[default]
    All,
    Favorites,
    Videos,
    Images,
    Collection(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaSort {
    #[default]
    Name,
    Newest,
    Type,
}

/// How the Media panel is filtered and laid out (per session).
pub struct MediaView {
    pub search: String,
    pub filter: MediaFilter,
    pub sort: MediaSort,
    pub by_folder: bool,
    pub tile: f32,
    new_collection: String,
}

impl Default for MediaView {
    fn default() -> Self {
        Self { search: String::new(), filter: MediaFilter::All, sort: MediaSort::Name, by_folder: true, tile: 128.0, new_collection: String::new() }
    }
}

pub(crate) enum MediaAct {
    Assign(usize),
    NewLayer(usize),
    /// Play this item on that layer.
    PlayOn(usize, u64),
    /// Open the Convert window for this item.
    Convert(usize),
    Favorite(usize),
    Toggle(usize, String),
    Rename(usize, String),
    Remove(usize),
    FillFolder(String),
    /// Everything the bin shows now (search, filter, sort) → a new layer of clips.
    ShownToLayer(Vec<usize>),
    /// Play one of these at random on the selected layer.
    RandomOne(Vec<usize>),
    /// These as a new clip layer that plays them in random order by itself (named after the collection).
    ShuffleLayer(Vec<usize>, String),
    Unwatch(String),
    NewCollection(String, Option<usize>),
    DeleteCollection(String),
}

pub(crate) fn media(ui: &mut egui::Ui, st: &mut State) {
    // Row 1: search, the full width (it never shares a row, so nothing can cover it).
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(Color32::GRAY));
        ui.add(egui::TextEdit::singleline(&mut st.media_view.search).hint_text("Search media").desired_width(ui.available_width()));
    });
    // Row 2: import, watch, group, sort, size; wraps when the column is narrow.
    ui.horizontal_wrapped(|ui| {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if ui.button(label(icon::FOLDER_OPEN, "Import…")).on_hover_text("Add files").clicked() {
                let exts = crate::media::dialog_extensions();
                st.dialogs.pick_files(crate::dialogs::Purpose::ImportFiles, Some(("Media", &exts)));
            }
            if ui.button(label(icon::EYE, "Watch folder…")).on_hover_text("Loads every clip in a folder (and its subfolders) and picks up new files automatically").clicked() {
                st.dialogs.pick_folder(crate::dialogs::Purpose::WatchFolder);
            }
            if st.folders.scanning() {
                ui.spinner();
            }
        }
        // Sorting, grouping and tile size in one menu, so this row fits any column.
        ui.menu_button(label(icon::SLIDERS_HORIZONTAL, "View"), |ui| {
            ui.set_min_width(200.0);
            ui.label(RichText::new("Sort by").strong());
            ui.horizontal(|ui| {
                ui.selectable_value(&mut st.media_view.sort, MediaSort::Name, "Name");
                ui.selectable_value(&mut st.media_view.sort, MediaSort::Newest, "Newest");
                ui.selectable_value(&mut st.media_view.sort, MediaSort::Type, "Type");
            });
            ui.checkbox(&mut st.media_view.by_folder, "Group by folder");
            ui.label(RichText::new("Tile size").strong());
            ui.spacing_mut().slider_width = 160.0;
            guarded!(ui, st.media_view.tile, Slider::new(&mut st.media_view.tile, 72.0..=240.0).show_value(false));
        })
        .response
        .on_hover_text("Sort, group by folder, tile size");
    });
    // Row 3: filters and collections.
    let mut acts: Vec<MediaAct> = Vec::new();
    {
        let State { comp, media_view: view, .. } = st;
        ui.horizontal_wrapped(|ui| {
            let count = |f: &dyn Fn(&phosphor_engine::MediaItem) -> bool| comp.media_bin.iter().filter(|m| f(m)).count();
            ui.selectable_value(&mut view.filter, MediaFilter::All, format!("All {}", comp.media_bin.len()));
            ui.selectable_value(&mut view.filter, MediaFilter::Favorites, format!("{} {}", icon::STAR, count(&|m| m.favorite)));
            ui.selectable_value(&mut view.filter, MediaFilter::Videos, format!("{} {}", icon::FILM_STRIP, count(&|m| crate::media::is_movie(&m.name))));
            ui.selectable_value(&mut view.filter, MediaFilter::Images, format!("{} {}", icon::IMAGE, count(&|m| !crate::media::is_movie(&m.name))));
            for c in comp.media_collections.clone() {
                let n = count(&|m| m.collections.contains(&c));
                let r = ui.selectable_value(&mut view.filter, MediaFilter::Collection(c.clone()), format!("{} {c} {n}", icon::BOOKMARKS_SIMPLE));
                r.right_menu(|ui| {
                    if ui.button(label(icon::TRASH, "Delete collection")).on_hover_text("The media stays in the bin").clicked() {
                        acts.push(MediaAct::DeleteCollection(c.clone()));
                        ui.close();
                    }
                });
            }
            ui.menu_button(label(icon::PLUS, "Collection"), |ui| {
                ui.add(egui::TextEdit::singleline(&mut view.new_collection).hint_text("e.g. Drops").desired_width(140.0));
                if ui.add_enabled(!view.new_collection.trim().is_empty(), egui::Button::new("Create")).clicked() {
                    acts.push(MediaAct::NewCollection(view.new_collection.trim().to_string(), None));
                    view.new_collection.clear();
                    ui.close();
                }
            });
        });
    }
    ui.add_space(2.0);

    // Which items show, in what order.
    let view = &st.media_view;
    let q = view.search.to_lowercase();
    let mut shown: Vec<usize> = st
        .comp
        .media_bin
        .iter()
        .enumerate()
        .filter(|(_, m)| q.is_empty() || m.name.to_lowercase().contains(&q) || m.path.to_lowercase().contains(&q))
        .filter(|(_, m)| match &view.filter {
            MediaFilter::All => true,
            MediaFilter::Favorites => m.favorite,
            MediaFilter::Videos => crate::media::is_movie(&m.name),
            MediaFilter::Images => !crate::media::is_movie(&m.name),
            MediaFilter::Collection(c) => m.collections.contains(c),
        })
        .map(|(i, _)| i)
        .collect();
    let bin = &st.comp.media_bin;
    match view.sort {
        MediaSort::Name => shown.sort_by_key(|i| bin[*i].name.to_lowercase()),
        MediaSort::Newest => shown.reverse(),
        MediaSort::Type => shown.sort_by_key(|i| (!crate::media::is_movie(&bin[*i].name), bin[*i].name.to_lowercase())),
    }
    let tile_w = view.tile;
    let groups: Vec<String> = if view.by_folder {
        let mut g: Vec<String> = Vec::new();
        for i in &shown {
            if !g.contains(&bin[*i].folder) {
                g.push(bin[*i].folder.clone());
            }
        }
        g.sort();
        g
    } else {
        vec![String::from("\u{0}all")]
    };
    let collections = st.comp.media_collections.clone();
    let mut acts = acts;
    ui.horizontal(|ui| {
        let tip = "A new layer with every file shown here (after search, filter and sort) as its clips, the first one playing";
        if ui.add_enabled(!shown.is_empty(), egui::Button::new(label(icon::SQUARES_FOUR, &format!("Make a clip layer ({})", shown.len())))).on_hover_text(tip).clicked() {
            acts.push(MediaAct::ShownToLayer(shown.clone()));
        }
    });
    // Show folders (collections): pick one above, then play from it at random.
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(!shown.is_empty(), egui::Button::new(label(icon::DICE_FIVE, "Random"))).on_hover_text("Play one of the files shown here, at random, on the selected layer (click again for another)").clicked() {
            acts.push(MediaAct::RandomOne(shown.clone()));
        }
        let name = match &st.media_view.filter {
            MediaFilter::Collection(c) => c.clone(),
            MediaFilter::Favorites => "Favourites".to_string(),
            _ => "Shuffle".to_string(),
        };
        if ui.add_enabled(shown.len() > 1, egui::Button::new(label(icon::SHUFFLE, "Shuffle on a new layer"))).on_hover_text("A new layer with the files shown here as clips, playing them in random order by itself (autopilot)").clicked() {
            acts.push(MediaAct::ShuffleLayer(shown.clone(), name));
        }
    });

    // Where a click plays (the selected layer, else a new one) and every layer for "Play on".
    let layers: Vec<(u64, String)> = st.comp.layers.iter().rev().map(|l| (l.id, l.name.clone())).collect();
    let click_target = st.selected.and_then(|id| layers.iter().find(|(l, _)| *l == id)).map(|(_, n)| n.clone());
    let targets = MediaTargets { layers: &layers, click: click_target.as_deref() };
    let State { comp, thumbs, .. } = st;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if comp.media_bin.is_empty() {
            hint(ui, "Empty. Import files, watch a folder, or drop files onto the window.");
        } else if shown.is_empty() {
            hint(ui, "Nothing matches this filter.");
        }
        for group in groups {
            let all = group == "\u{0}all";
            let items: Vec<usize> = shown.iter().copied().filter(|i| all || comp.media_bin[*i].folder == group).collect();
            let mut grid = |ui: &mut egui::Ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                    for i in &items {
                        media_tile(ui, &comp.media_bin[*i], *i, tile_w, thumbs, &collections, &targets, &mut acts);
                    }
                });
            };
            if all {
                grid(ui);
                continue;
            }
            let title = if group.is_empty() { "Files".to_string() } else { group.rsplit('/').next().unwrap_or(&group).to_string() };
            let header = egui::CollapsingHeader::new(RichText::new(format!("{} {title}  ({})", if group.is_empty() { icon::FILES } else { icon::FOLDER }, items.len())).strong())
                .id_salt(("media_group", &group))
                .default_open(true)
                .show(ui, |ui| grid(ui));
            header.header_response.right_menu(|ui| {
                if ui.button(label(icon::SQUARES_FOUR, "Fill clips of the selected layer")).on_hover_text("Puts this folder's media into the selected layer's clip slots, in order").clicked() {
                    acts.push(MediaAct::FillFolder(group.clone()));
                    ui.close();
                }
                if !group.is_empty() {
                    if ui.button(label(icon::EYE_SLASH, "Stop watching")).clicked() {
                        acts.push(MediaAct::Unwatch(group.clone()));
                        ui.close();
                    }
                    #[cfg(target_os = "macos")]
                    if ui.button(label(icon::FOLDER_OPEN, "Show in Finder")).clicked() {
                        crate::os::open(&group);
                        ui.close();
                    }
                }
            });
        }
    });

    let now = ui.ctx().input(|i| i.time);
    for act in acts {
        match act {
            MediaAct::Assign(i) => {
                let Some(item) = st.comp.media_bin.get(i).cloned() else { continue };
                let target = match st.selected {
                    Some(id) if id != MASTER_ID && st.comp.layers.iter().any(|l| l.id == id) => id,
                    _ => st.add_layer(),
                };
                st.launch_source(target, crate::media::spec_for_path(&item.path), Vec::new(), item.name, now);
            }
            MediaAct::Convert(i) => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(item) = st.comp.media_bin.get(i).cloned() {
                    st.convert.open_for(&item.path, &item.name);
                }
                #[cfg(target_arch = "wasm32")]
                let _ = i;
            }
            MediaAct::PlayOn(i, layer) => {
                let Some(item) = st.comp.media_bin.get(i).cloned() else { continue };
                st.selected = Some(layer);
                st.launch_source(layer, crate::media::spec_for_path(&item.path), Vec::new(), item.name, now);
            }
            MediaAct::NewLayer(i) => {
                let Some(item) = st.comp.media_bin.get(i).cloned() else { continue };
                let id = st.add_layer();
                st.launch_source(id, crate::media::spec_for_path(&item.path), Vec::new(), item.name, now);
            }
            MediaAct::Favorite(i) => {
                if let Some(m) = st.comp.media_bin.get_mut(i) {
                    m.favorite = !m.favorite;
                }
            }
            MediaAct::Toggle(i, c) => {
                if let Some(m) = st.comp.media_bin.get_mut(i) {
                    if let Some(k) = m.collections.iter().position(|x| *x == c) {
                        m.collections.remove(k);
                    } else {
                        m.collections.push(c);
                    }
                }
            }
            MediaAct::Rename(i, name) => {
                if let Some(m) = st.comp.media_bin.get_mut(i) {
                    m.name = name;
                }
            }
            MediaAct::Remove(i) => {
                if i < st.comp.media_bin.len() {
                    let item = st.comp.media_bin.remove(i);
                    st.thumbs.forget(&item.path);
                }
            }
            MediaAct::FillFolder(group) => {
                let Some(sel) = st.selected.filter(|id| st.comp.layers.iter().any(|l| l.id == *id)) else { continue };
                let items = clip_items(st.comp.media_bin.iter().filter(|m| m.folder == group));
                st.comp.place_clips(sel, 0, items);
            }
            MediaAct::ShownToLayer(shown) => {
                let items = clip_items(shown.iter().filter_map(|i| st.comp.media_bin.get(*i)));
                let layer = st.add_layer();
                let n = st.comp.place_clips(layer, 0, items);
                st.selected = Some(layer);
                st.clip_page = 0;
                st.trigger(Trigger::Clip { layer, slot: 0 }, now);
                st.notice = Some(format!("New layer with {n} clips"));
            }
            MediaAct::RandomOne(shown) => {
                let Some(i) = crate::app::dice().pick(&shown).copied() else { continue };
                let Some(item) = st.comp.media_bin.get(i).cloned() else { continue };
                let target = match st.selected {
                    Some(id) if id != MASTER_ID && st.comp.layers.iter().any(|l| l.id == id) => id,
                    _ => st.add_layer(),
                };
                st.selected = Some(target);
                st.launch_source(target, crate::media::spec_for_path(&item.path), Vec::new(), item.name, now);
            }
            MediaAct::ShuffleLayer(shown, name) => {
                let items = clip_items(shown.iter().filter_map(|i| st.comp.media_bin.get(*i)));
                let layer = st.add_layer();
                let n = st.comp.place_clips(layer, 0, items);
                if let Some(l) = st.comp.layer_mut(layer) {
                    l.name = name.clone();
                    l.autopilot = phosphor_engine::Autopilot::Random;
                    l.autopilot_beats = 0.0;
                }
                st.selected = Some(layer);
                let first = crate::app::dice().below(n.max(1));
                st.trigger(Trigger::Clip { layer, slot: first }, now);
                st.notice = Some(format!("\"{name}\": {n} clips playing in random order (the layer's name menu changes how)"));
            }
            MediaAct::Unwatch(folder) => {
                st.comp.media_bin.retain(|m| m.folder != folder);
                #[cfg(not(target_arch = "wasm32"))]
                st.folders.remove(&folder);
            }
            MediaAct::NewCollection(name, item) => {
                if !st.comp.media_collections.contains(&name) {
                    st.comp.media_collections.push(name.clone());
                }
                if let Some(m) = item.and_then(|i| st.comp.media_bin.get_mut(i))
                    && !m.collections.contains(&name)
                {
                    m.collections.push(name);
                }
            }
            MediaAct::DeleteCollection(name) => {
                st.comp.media_collections.retain(|c| *c != name);
                for m in &mut st.comp.media_bin {
                    m.collections.retain(|c| *c != name);
                }
                if st.media_view.filter == MediaFilter::Collection(name) {
                    st.media_view.filter = MediaFilter::All;
                }
            }
        }
    }
}

/// One media thumbnail: click to launch, star to favourite, drag onto layers or clip slots,
/// right-click to organise.
/// The layers a media file can be played on, and the one a click plays it on (None = a new layer).
pub(crate) struct MediaTargets<'a> {
    pub layers: &'a [(u64, String)],
    pub click: Option<&'a str>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn media_tile(
    ui: &mut egui::Ui,
    item: &phosphor_engine::MediaItem,
    i: usize,
    w: f32,
    thumbs: &mut crate::thumbs::Thumbs,
    collections: &[String],
    targets: &MediaTargets,
    acts: &mut Vec<MediaAct>,
) {
    let h = w * 9.0 / 16.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h + 18.0), Sense::click_and_drag());
    if !ui.is_rect_visible(rect) {
        return;
    }
    resp.dnd_set_drag_payload(MediaDrag(i));
    let thumb_rect = egui::Rect::from_min_size(rect.min, egui::vec2(w, h));
    let p = ui.painter();
    p.rect_filled(thumb_rect, 4.0, Color32::from_gray(14));
    match thumbs.get(&item.path) {
        Some(tex) => paint_texture(p, tex, thumb_rect),
        None => {
            let glyph = if crate::media::is_movie(&item.name) { icon::FILM_STRIP } else { icon::IMAGE };
            let text = if thumbs.is_pending(&item.path) { "…" } else { glyph };
            p.text(thumb_rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(22.0), Color32::GRAY);
        }
    }
    let stroke = if resp.hovered() { Stroke::new(1.5, accent()) } else { Stroke::new(1.0, Color32::from_gray(50)) };
    p.rect_stroke(thumb_rect, 4.0, stroke, StrokeKind::Inside);
    let max_chars = ((w / 6.5) as usize).max(8);
    let mut name = item.name.clone();
    if name.chars().count() > max_chars {
        name = format!("{}…", name.chars().take(max_chars - 1).collect::<String>());
    }
    p.text(egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, name, egui::FontId::proportional(11.0), Color32::LIGHT_GRAY);
    if !item.collections.is_empty() {
        p.text(thumb_rect.left_top() + egui::vec2(4.0, 3.0), egui::Align2::LEFT_TOP, icon::BOOKMARKS_SIMPLE, egui::FontId::proportional(11.0), Color32::from_white_alpha(180));
    }
    // Star (registered after the tile so it gets the click).
    let star_rect = egui::Rect::from_min_size(thumb_rect.right_top() + egui::vec2(-22.0, 2.0), egui::vec2(20.0, 20.0));
    let star = ui.interact(star_rect, ui.id().with(("star", &item.path)), Sense::click());
    if item.favorite || resp.hovered() || star.hovered() {
        let color = if item.favorite { Color32::from_rgb(255, 210, 90) } else { Color32::from_white_alpha(150) };
        let glyph = if item.favorite { icon::STAR } else { icon::STAR_HALF };
        ui.painter().text(star_rect.center(), egui::Align2::CENTER_CENTER, if item.favorite { glyph } else { icon::STAR }, egui::FontId::proportional(15.0), color);
    }
    if star.clicked() {
        acts.push(MediaAct::Favorite(i));
    }
    if resp.clicked() {
        acts.push(MediaAct::Assign(i));
    }
    let click = match targets.click {
        Some(name) => format!("Click: play on \"{name}\" · drag onto any layer or clip slot · right-click: Play on…"),
        None => "Click: play on a new layer · drag onto any layer or clip slot · right-click: Play on…".to_string(),
    };
    resp.on_hover_text(format!("{}\n{click}", item.path)).right_menu(|ui| {
        ui.menu_button(label(icon::PLAY, "Play on"), |ui| {
            for (id, name) in targets.layers {
                if ui.button(name).clicked() {
                    acts.push(MediaAct::PlayOn(i, *id));
                    ui.close();
                }
            }
            if ui.button(label(icon::PLUS, "A new layer")).clicked() {
                acts.push(MediaAct::NewLayer(i));
                ui.close();
            }
        });
        ui.separator();
        let fav = if item.favorite { "Remove from favourites" } else { "Add to favourites" };
        if ui.button(label(icon::STAR, fav)).clicked() {
            acts.push(MediaAct::Favorite(i));
            ui.close();
        }
        ui.menu_button(label(icon::BOOKMARKS_SIMPLE, "Collections"), |ui| {
            for c in collections {
                let mut on = item.collections.contains(c);
                if ui.checkbox(&mut on, c).clicked() {
                    acts.push(MediaAct::Toggle(i, c.clone()));
                }
            }
            ui.separator();
            let id = egui::Id::new(("new_coll", &item.path));
            let mut text: String = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut text).hint_text("New collection").desired_width(120.0));
                if ui.add_enabled(!text.trim().is_empty(), egui::Button::new("Add")).clicked() {
                    acts.push(MediaAct::NewCollection(text.trim().to_string(), Some(i)));
                    text.clear();
                    ui.close();
                }
            });
            ui.ctx().data_mut(|d| d.insert_temp(id, text));
        });
        ui.menu_button(label(icon::PENCIL_SIMPLE, "Rename"), |ui| {
            let id = egui::Id::new(("rename", &item.path));
            let mut text: String = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_else(|| item.name.clone());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut text).desired_width(160.0));
                if ui.button("OK").clicked() {
                    acts.push(MediaAct::Rename(i, text.trim().to_string()));
                    ui.close();
                }
            });
            ui.ctx().data_mut(|d| d.insert_temp(id, text));
        });
        #[cfg(not(target_arch = "wasm32"))]
        if crate::media::is_movie(&item.name) && ui.button(label(icon::FILM_STRIP, "Convert…")).on_hover_text("Make a copy that plays better in a show (Hap, ProRes, or H.264 with every frame a keyframe); see the size first").clicked() {
            acts.push(MediaAct::Convert(i));
            ui.close();
        }
        ui.separator();
        if ui.button(label(icon::PLUS, "Put on a new layer")).clicked() {
            acts.push(MediaAct::NewLayer(i));
            ui.close();
        }
        #[cfg(target_os = "macos")]
        if ui.button(label(icon::FOLDER_OPEN, "Reveal in Finder")).clicked() {
            crate::os::reveal(&item.path);
            ui.close();
        }
        if ui.button(label(icon::TRASH, "Remove from bin")).clicked() {
            acts.push(MediaAct::Remove(i));
            ui.close();
        }
    });
}

/// Media bin items as clips (name without extension, source by file type).
fn clip_items<'a>(items: impl Iterator<Item = &'a phosphor_engine::MediaItem>) -> Vec<(String, SourceSpec)> {
    items
        .map(|m| (crate::media::clip_name(&m.name), crate::media::spec_for_path(&m.path)))
        .collect()
}
