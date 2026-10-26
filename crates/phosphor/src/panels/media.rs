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
    /// Longest first (movies' lengths are read in the background the first time).
    Length,
}

/// Where a click on a media tile plays it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PlaysOn {
    /// The selected layer (a new one if none is selected).
    #[default]
    Selected,
    /// Always this layer, whatever is selected.
    Layer(u64),
    /// A new layer each time.
    NewLayer,
}

/// The layer a media click plays on now (None = a new layer).
pub(crate) fn plays_on_layer(st: &State) -> Option<u64> {
    let exists = |id: u64| id != MASTER_ID && st.comp.layers.iter().any(|l| l.id == id);
    match st.media_view.plays_on {
        PlaysOn::Selected => st.selected.filter(|id| exists(*id)),
        PlaysOn::Layer(id) => Some(id).filter(|id| exists(*id)),
        PlaysOn::NewLayer => None,
    }
}

/// "Plays on": which layer a click on a tile plays on (the selected one, a fixed one, or a new one each time).
pub(crate) fn plays_on_picker(ui: &mut egui::Ui, st: &mut State) {
    let name = |id: u64| st.comp.layer(id).map_or_else(|| "?".to_string(), |l| l.name.clone());
    let current = match (st.media_view.plays_on, plays_on_layer(st)) {
        (PlaysOn::Selected, Some(id)) => format!("selected ({})", name(id)),
        (PlaysOn::Selected, None) => "selected (none: a new layer)".to_string(),
        (PlaysOn::Layer(_), Some(id)) => name(id),
        (_, _) => "a new layer".to_string(),
    };
    let layers: Vec<(u64, String)> = st.comp.layers.iter().rev().map(|l| (l.id, l.name.clone())).collect();
    let mut pick = st.media_view.plays_on;
    crate::ui::toolbar(ui, |ui| {
        ui.label(RichText::new(format!("{} Plays on", icon::PLAY)).small());
        crate::ui::dropdown(ui, "media_plays_on", 170.0).selected_text(current).show_ui(ui, |ui| {
            crate::ui::pick_value(ui, &mut pick, PlaysOn::Selected, "The selected layer");
            crate::ui::pick_value(ui, &mut pick, PlaysOn::NewLayer, "A new layer each time");
            ui.separator();
            for (id, n) in &layers {
                crate::ui::pick_value(ui, &mut pick, PlaysOn::Layer(*id), n);
            }
        })
        .response
        .on_hover_text("Where a click on a tile plays it (dragging onto a layer or a clip cell still goes where you drop it)");
    });
    st.media_view.plays_on = pick;
}

/// How the Media panel is filtered and laid out (per session).
pub struct MediaView {
    pub search: String,
    /// Favourites / a collection (the classic view's filter; kept in step with `source`), or Videos / Pictures.
    pub filter: MediaFilter,
    pub sort: MediaSort,
    pub by_folder: bool,
    pub tile: f32,
    /// What the grid shows (`media_sources`).
    pub source: super::media_sources::MediaSource,
    /// The folder open in a watched folder (`MediaSource::Folder`).
    pub dir: Option<String>,
    /// The old one-list view with folder groups (View menu).
    pub classic: bool,
    /// Videos / pictures only (`All`, `Videos`, `Images`).
    pub only: MediaFilter,
    /// Where a click on a tile plays.
    pub plays_on: PlaysOn,
    pub follow_open: bool,
    pub follow_text: String,
    #[cfg(not(target_arch = "wasm32"))]
    pub follow_site: crate::online::following::Site,
    #[cfg(not(target_arch = "wasm32"))]
    pub following: crate::online::following::Following,
    #[cfg(not(target_arch = "wasm32"))]
    pub refresh: Option<std::sync::Arc<std::sync::Mutex<crate::online::following::Progress>>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub has_gallery_dl: Option<bool>,
    /// Movies' lengths, read in the background (Sort by Length).
    #[cfg(not(target_arch = "wasm32"))]
    pub durations: crate::media::durations::Durations,
    /// What the last "Keep in my library" did (set by its thread).
    #[cfg(not(target_arch = "wasm32"))]
    pub kept: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl Default for MediaView {
    fn default() -> Self {
        Self {
            search: String::new(),
            filter: MediaFilter::All,
            sort: MediaSort::Name,
            by_folder: true,
            tile: 128.0,
            source: Default::default(),
            dir: None,
            classic: false,
            only: MediaFilter::All,
            plays_on: PlaysOn::Selected,
            follow_open: false,
            follow_text: String::new(),
            #[cfg(not(target_arch = "wasm32"))]
            follow_site: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            following: crate::online::following::Following::load(),
            #[cfg(not(target_arch = "wasm32"))]
            refresh: None,
            #[cfg(not(target_arch = "wasm32"))]
            has_gallery_dl: None,
            #[cfg(not(target_arch = "wasm32"))]
            kept: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            durations: Default::default(),
        }
    }
}

impl MediaView {
    /// Shows a source (and the folder open in it); the classic view's filter follows.
    pub fn show(&mut self, source: super::media_sources::MediaSource, dir: Option<String>) {
        use super::media_sources::MediaSource as S;
        self.filter = match &source {
            S::Favorites => MediaFilter::Favorites,
            S::Collection(c) => MediaFilter::Collection(c.clone()),
            _ => MediaFilter::All,
        };
        self.dir = dir;
        self.source = source;
        self.follow_open = false;
    }

    pub fn show_collection(&mut self, name: String) {
        self.show(super::media_sources::MediaSource::Collection(name), None);
    }
}

pub(crate) enum MediaAct {
    Assign(usize),
    NewLayer(usize),
    /// Play this item on that layer.
    PlayOn(usize, u64),
    /// Open the Convert window for this item.
    Convert(usize),
    /// Open the file again from disk (it was edited).
    Reload(usize),
    Favorite(usize),
    /// Drop the file's own effects.
    ForgetEffects(usize),
    Toggle(usize, String),
    Rename(usize, String),
    Remove(usize),
    FillFolder(Vec<usize>),
    /// Everything the bin shows now (search, filter, sort) → a new layer of clips.
    ShownToLayer(Vec<usize>),
    /// Play one of these at random on the selected layer.
    RandomOne(Vec<usize>),
    /// These as a new clip layer that plays them in random order by itself (named after the collection).
    ShuffleLayer(Vec<usize>, String),
    Unwatch(String),
    /// A new collection (empty name = "Collection N"), optionally with this file in it.
    NewCollection(String, Option<usize>),
    RenameCollection(String, String),
    RemoveCollectionFolder(String, String),
    /// Put this collection on that layer (None = a new layer): it plays through it by itself.
    PlayCollection(String, Option<u64>),
    /// Every layer playing this collection stops following it.
    StopCollection(String),
    DeleteCollection(String),
    /// Copy these downloaded files into the library folder (to hold onto them).
    Keep(Vec<usize>),
}

pub(crate) fn media(ui: &mut egui::Ui, st: &mut State) {
    if st.media_view.classic {
        return media_classic(ui, st);
    }
    super::media_browser::media_browser(ui, st);
}

/// The Media panel before sources (View › Classic view): one list, folders as groups.
fn media_classic(ui: &mut egui::Ui, st: &mut State) {
    // Row 1: search, the full width (it never shares a row, so nothing can cover it).
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(Color32::GRAY));
        ui.add(egui::TextEdit::singleline(&mut st.media_view.search).hint_text("Search media").desired_width(ui.available_width()));
    });
    // Row 2: import, watch, group, sort, size; wraps when the column is narrow.
    ui.horizontal_wrapped(|ui| {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if crate::ui::command(ui, label(icon::FOLDER_OPEN, "Import…")).on_hover_text("Add files").clicked() {
                let exts = crate::media::dialog_extensions();
                st.dialogs.pick_files(crate::dialogs::Purpose::ImportFiles, Some(("Media", &exts)));
            }
            if crate::ui::command(ui, label(icon::LINK, "From a link…")).on_hover_text("The video or GIF in a post on X, Instagram, TikTok, YouTube…").clicked() {
                st.link_import.open = true;
            }
            if crate::ui::command(ui, label(icon::EYE, "Watch folder…")).on_hover_text("A collection of every clip in a folder (and its subfolders); new files join as they arrive").clicked() {
                st.dialogs.pick_folder(crate::dialogs::Purpose::WatchFolder);
            }
            if st.folders.scanning() {
                ui.spinner();
            }
        }
        // Sorting, grouping and tile size in one menu, so this row fits any column.
        ui.menu_button(label(icon::SLIDERS_HORIZONTAL, "View"), |ui| {
            ui.set_min_width(200.0);
            crate::ui::heading(ui, "Sort by");
            crate::ui::toolbar(ui, |ui| {
                crate::ui::pick_value(ui, &mut st.media_view.sort, MediaSort::Name, "Name");
                crate::ui::pick_value(ui, &mut st.media_view.sort, MediaSort::Newest, "Newest");
                crate::ui::pick_value(ui, &mut st.media_view.sort, MediaSort::Type, "Type");
                crate::ui::pick_value(ui, &mut st.media_view.sort, MediaSort::Length, "Length");
            });
            crate::ui::toggle(ui, &mut st.media_view.by_folder, "Group by folder");
            Level::range(&mut st.media_view.tile, 72.0, 240.0, "Tile size").default(128.0).width(180.0).show(ui);
            crate::ui::toggle(ui, &mut st.media_view.classic, "Classic view").on_hover_text("One list with folders as groups (off: sources down the left)");
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
            let filters = [
                (MediaFilter::All, format!("All {}", comp.media_bin.len())),
                (MediaFilter::Favorites, format!("{} {}", icon::STAR, count(&|m| m.favorite))),
                (MediaFilter::Videos, format!("{} {}", icon::FILM_STRIP, count(&|m| crate::media::is_movie(&m.name)))),
                (MediaFilter::Images, format!("{} {}", icon::IMAGE, count(&|m| !crate::media::is_movie(&m.name)))),
            ];
            segmented(ui, &mut view.filter, &filters, |_, _, _| {});
            for c in comp.media_collections.clone() {
                let n = count(&|m| comp.in_collection(m, &c));
                let r = crate::ui::pick_value(ui, &mut view.filter, MediaFilter::Collection(c.clone()), format!("{} {c} {n}", icon::BOOKMARKS_SIMPLE));
                r.right_menu(|ui| {
                    if crate::ui::command(ui, label(icon::TRASH, "Delete collection")).on_hover_text("The media stays in the bin").clicked() {
                        acts.push(MediaAct::DeleteCollection(c.clone()));
                        ui.close();
                    }
                });
            }
            if crate::ui::command(ui, label(icon::PLUS, "New collection")).on_hover_text("A set of files for a show: add files or whole folders, then play it on a layer").clicked() {
                acts.push(MediaAct::NewCollection(String::new(), None));
            }
        });
    }
    if let MediaFilter::Collection(c) = st.media_view.filter.clone() {
        super::collections::collection_header(ui, st, &c, &mut acts);
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
            MediaFilter::Collection(c) => st.comp.in_collection(m, c),
        })
        .map(|(i, _)| i)
        .collect();
    let bin = &st.comp.media_bin;
    match view.sort {
        MediaSort::Name => shown.sort_by_key(|i| bin[*i].name.to_lowercase()),
        MediaSort::Newest => shown.reverse(),
        MediaSort::Type => shown.sort_by_key(|i| (!crate::media::is_movie(&bin[*i].name), bin[*i].name.to_lowercase())),
        MediaSort::Length => shown.sort_by_key(|i| length_key(&bin[*i])),
    }
    let tile_w = view.tile;
    let groups: Vec<String> = if view.by_folder {
        let mut g: Vec<String> = Vec::new();
        for i in &shown {
            let key = folder_of(&bin[*i]);
            if !g.contains(&key) {
                g.push(key);
            }
        }
        g.sort();
        g
    } else {
        vec![String::from("\u{0}all")]
    };
    let collections = st.comp.media_collections.clone();
    let mut acts = acts;
    // What to do with the files shown: a clip layer of them, one at random, or a layer shuffling them. In a
    // collection, its header's Play on does the playing (and follows new files), so only Random stays here.
    let in_collection = matches!(st.media_view.filter, MediaFilter::Collection(_));
    ui.horizontal_wrapped(|ui| {
        if !in_collection {
            let tip = "A new layer with every file shown here (after search, filter and sort) as its clips, the first one playing";
            if crate::ui::command_if(ui, !shown.is_empty(), label(icon::SQUARES_FOUR, &format!("Clip layer ({})", shown.len()))).on_hover_text(tip).clicked() {
                acts.push(MediaAct::ShownToLayer(shown.clone()));
            }
        }
        if crate::ui::command_if(ui, !shown.is_empty(), label(icon::DICE_FIVE, "Random")).on_hover_text("Play one of the files shown here, at random, on the selected layer (click again for another)").clicked() {
            acts.push(MediaAct::RandomOne(shown.clone()));
        }
        if !in_collection {
            let name = if st.media_view.filter == MediaFilter::Favorites { "Favourites" } else { "Shuffle" }.to_string();
            if crate::ui::command_if(ui, shown.len() > 1, label(icon::SHUFFLE, "Shuffle layer")).on_hover_text("A new layer with the files shown here as clips, playing them in random order by itself (autopilot)").clicked() {
                acts.push(MediaAct::ShuffleLayer(shown.clone(), name));
            }
        }
    });

    // Where a click plays (the selected layer, else a new one) and every layer for "Play on".
    let layers: Vec<(u64, String)> = st.comp.layers.iter().rev().map(|l| (l.id, l.name.clone())).collect();
    let click_target = plays_on_layer(st).and_then(|id| layers.iter().find(|(l, _)| *l == id)).map(|(_, n)| n.clone());
    let mut playing: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for l in st.comp.layers.iter().filter(|l| l.enabled) {
        if let SourceSpec::Movie { path: Some(p), .. } | SourceSpec::Image { path: Some(p), .. } = &l.source {
            playing.entry(p.clone()).or_default().push(l.name.clone());
        }
    }
    let uses = st.comp.media_uses();
    let targets = MediaTargets { layers: &layers, click: click_target.as_deref(), playing: &playing, uses: &uses };
    let State { comp, thumbs, .. } = st;
    crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
        if comp.media_bin.is_empty() {
            help(ui, "Empty. Import files, watch a folder, or drop files onto the window.");
        } else if shown.is_empty() {
            hint(ui, "Nothing matches this filter.");
        }
        for group in groups {
            let all = group == "\u{0}all";
            let items: Vec<usize> = shown.iter().copied().filter(|i| all || folder_of(&comp.media_bin[*i]) == group).collect();
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
            // A subfolder shows as "Watched/Sub" so show folders inside one watched folder stay apart.
            let root = items.first().map(|i| comp.media_bin[*i].folder.clone()).unwrap_or_default();
            let title = if group.is_empty() {
                "Files".to_string()
            } else {
                let base = std::path::Path::new(&root).parent().map_or(String::new(), |p| p.to_string_lossy().into_owned());
                group.strip_prefix(&base).unwrap_or(&group).trim_start_matches(['/', '\\']).to_string()
            };
            let header = crate::ui::section(ui, ("media_group", &group), format!("{} {title}  ({})", if group.is_empty() { icon::FILES } else { icon::FOLDER }, items.len()), true, |ui| grid(ui));
            header.header_response.right_menu(|ui| {
                if crate::ui::command(ui, label(icon::DICE_FIVE, "Random")).on_hover_text("Play one of this folder's files, at random, on the selected layer").clicked() {
                    acts.push(MediaAct::RandomOne(items.clone()));
                    ui.close();
                }
                if crate::ui::command_if(ui, items.len() > 1, label(icon::SHUFFLE, "Shuffle on a new layer")).on_hover_text("A new layer with this folder's files as clips, playing them in random order by itself").clicked() {
                    acts.push(MediaAct::ShuffleLayer(items.clone(), title.clone()));
                    ui.close();
                }
                if crate::ui::command(ui, label(icon::SQUARES_FOUR, "Fill clips of the selected layer")).on_hover_text("Puts this folder's media into the selected layer's clip slots, in order").clicked() {
                    acts.push(MediaAct::FillFolder(items.clone()));
                    ui.close();
                }
                if !group.is_empty() {
                    if root == group && crate::ui::command(ui, label(icon::EYE_SLASH, "Stop watching")).clicked() {
                        acts.push(MediaAct::Unwatch(root.clone()));
                        ui.close();
                    }
                    if crate::ui::command(ui, label(icon::FOLDER_OPEN, "Show the folder")).clicked() {
                        #[cfg(not(target_arch = "wasm32"))]
                        crate::os::open(&group);
                        ui.close();
                    }
                }
            });
        }
    });

    let now = ui.ctx().input(|i| i.time);
    apply_acts(st, acts, now);
}

/// Does what the Media panel's tiles and buttons asked.
pub(crate) fn apply_acts(st: &mut State, acts: Vec<MediaAct>, now: f64) {
    for act in acts {
        match act {
            MediaAct::Assign(i) => {
                let Some(item) = st.comp.media_bin.get(i).cloned() else { continue };
                let target = match plays_on_layer(st) {
                    Some(id) => id,
                    None => st.add_layer(),
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
            MediaAct::Reload(i) => {
                if let Some(item) = st.comp.media_bin.get(i).cloned() {
                    st.media.reload(&st.comp, &item.path);
                    st.thumbs.forget(&item.path);
                }
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
            MediaAct::ForgetEffects(i) => {
                if let Some(path) = st.comp.media_bin.get(i).map(|m| m.path.clone()) {
                    st.comp.forget_file_effects(&path);
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
                let items = clip_items(group.iter().filter_map(|i| st.comp.media_bin.get(*i)));
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
                let target = match plays_on_layer(st) {
                    Some(id) => id,
                    None => st.add_layer(),
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
                let name = st.comp.create_collection(&name);
                if let Some(i) = item {
                    st.comp.add_to_collection(i, &name);
                }
                st.media_view.show_collection(name);
            }
            MediaAct::RenameCollection(from, to) => {
                if st.comp.rename_collection(&from, &to) {
                    st.media_view.show_collection(to);
                } else {
                    st.notice = Some(format!("There's already a collection called \"{to}\""));
                }
            }
            MediaAct::RemoveCollectionFolder(name, folder) => st.comp.remove_collection_folder(&name, &folder),
            MediaAct::PlayCollection(name, layer) => {
                let id = match layer {
                    Some(id) => id,
                    None => st.add_layer(),
                };
                if let Some(l) = st.comp.layer_mut(id) {
                    l.collection = Some(name.clone());
                    if l.autopilot == phosphor_engine::Autopilot::Off {
                        l.autopilot = phosphor_engine::Autopilot::Next;
                    }
                    l.name = name.clone();
                }
                st.selected = Some(id);
                st.notice = Some(format!("\"{name}\" plays on that layer: next / previous / random is in the Inspector"));
            }
            MediaAct::StopCollection(name) => {
                for l in st.comp.layers.iter_mut().filter(|l| l.collection.as_deref() == Some(name.as_str())) {
                    l.collection = None;
                }
            }
            MediaAct::Keep(items) => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(lib) = st.media_view.following.library_dir() {
                    let files = items.iter().filter_map(|i| st.comp.media_bin.get(*i)).map(|m| std::path::PathBuf::from(&m.path)).collect();
                    // Watched, so kept clips show under This Mac.
                    let _ = std::fs::create_dir_all(&lib);
                    st.folders.add(lib.to_string_lossy().into_owned());
                    crate::online::following::keep(files, lib, None, st.media_view.kept.clone());
                }
                #[cfg(target_arch = "wasm32")]
                let _ = items;
            }
            MediaAct::DeleteCollection(name) => {
                st.comp.delete_collection(&name);
                if st.media_view.filter == MediaFilter::Collection(name) {
                    st.media_view.show(Default::default(), None);
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
    /// Files on screen now (path → the layers showing them), marked on their tiles.
    pub playing: &'a std::collections::HashMap<String, Vec<String>>,
    /// How many layers and clips use each file.
    pub uses: &'a std::collections::HashMap<String, usize>,
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
        Some(tex) => paint_texture(p, tex, fit_rect(thumb_rect.shrink(1.0), thumbs.aspect(&item.path).unwrap_or(16.0 / 9.0))),
        None => {
            let glyph = if crate::media::is_movie(&item.name) { icon::FILM_STRIP } else { icon::IMAGE };
            let text = if thumbs.is_pending(&item.path) { "…" } else { glyph };
            p.text(thumb_rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(22.0), Color32::GRAY);
        }
    }
    let on = targets.playing.get(&item.path);
    let stroke = if on.is_some() { Stroke::new(2.5, accent()) } else if resp.hovered() { Stroke::new(1.5, accent()) } else { Stroke::new(1.0, Color32::from_gray(50)) };
    p.rect_stroke(thumb_rect, 4.0, stroke, StrokeKind::Inside);
    if on.is_some() {
        // On screen now: a play badge in the corner (which layers: on hover).
        let badge = egui::Rect::from_min_size(thumb_rect.left_bottom() + egui::vec2(4.0, -20.0), egui::vec2(18.0, 16.0));
        p.rect_filled(badge, 3.0, accent());
        p.text(badge.center(), egui::Align2::CENTER_CENTER, icon::PLAY, egui::FontId::proportional(11.0), Color32::BLACK);
    }
    let max_chars = ((w / 6.5) as usize).max(8);
    let mut name = item.name.clone();
    if name.chars().count() > max_chars {
        name = format!("{}…", name.chars().take(max_chars - 1).collect::<String>());
    }
    p.text(egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, name, egui::FontId::proportional(11.0), Color32::LIGHT_GRAY);
    if !item.collections.is_empty() {
        p.text(thumb_rect.left_top() + egui::vec2(4.0, 3.0), egui::Align2::LEFT_TOP, icon::BOOKMARKS_SIMPLE, egui::FontId::proportional(11.0), Color32::from_white_alpha(180));
    }
    // What comes with it when it plays: its own effects, changed speed / direction, a trim.
    let comes_with = super::layers::badge_chip(p, thumb_rect, &media_extras(item));
    // Star (registered after the tile so it gets the click).
    let star_rect = egui::Rect::from_min_size(thumb_rect.right_top() + egui::vec2(-22.0, 2.0), egui::vec2(20.0, 20.0));
    let star = ui.interact(star_rect, ui.id().with(("star", &item.path)), Sense::click());
    // A favourite: a solid gold star. Not one yet: an outline only while the mouse is over the tile (so the picture
    // stays clear), brighter over the star itself.
    let hover = resp.hovered() || star.hovered();
    if item.favorite || hover {
        let font = if item.favorite { crate::theme::filled_icons(15.0) } else { egui::FontId::proportional(15.0) };
        let color = if item.favorite { Color32::from_rgb(255, 210, 90) } else if star.hovered() { Color32::WHITE } else { Color32::from_white_alpha(150) };
        let p = ui.painter();
        p.text(star_rect.center() + egui::vec2(0.5, 1.0), egui::Align2::CENTER_CENTER, icon::STAR, font.clone(), Color32::from_black_alpha(160));
        p.text(star_rect.center(), egui::Align2::CENTER_CENTER, icon::STAR, font, color);
    }
    let star = star.on_hover_text(if item.favorite { "A favourite: click to unstar" } else { "Click to make it a favourite" });
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
    let showing = on.map_or(String::new(), |ls| format!("\nOn screen now: {}", ls.join(", ")));
    let used = match targets.uses.get(&item.path).copied().unwrap_or(0) {
        0 => "\nNot used by any layer or clip".to_string(),
        1 => "\nUsed by 1 layer or clip".to_string(),
        n => format!("\nUsed by {n} layers and clips"),
    };
    let credit = item.credit.as_ref().map_or(String::new(), |c| format!("\nCredit: {}", c.line()));
    resp.on_hover_text(format!("{}{credit}{comes_with}{showing}{used}\n{click}", item.path)).right_menu(|ui| {
        ui.menu_button(label(icon::PLAY, "Play on"), |ui| {
            for (id, name) in targets.layers {
                if crate::ui::command(ui, name).clicked() {
                    acts.push(MediaAct::PlayOn(i, *id));
                    ui.close();
                }
            }
            if crate::ui::command(ui, label(icon::PLUS, "A new layer")).clicked() {
                acts.push(MediaAct::NewLayer(i));
                ui.close();
            }
        });
        ui.separator();
        let fav = if item.favorite { "Remove from favourites" } else { "Add to favourites" };
        if crate::ui::command(ui, label(icon::STAR, fav)).clicked() {
            acts.push(MediaAct::Favorite(i));
            ui.close();
        }
        crate::ui::field_menu(ui, label(icon::BOOKMARKS_SIMPLE, "Collections"), |ui| {
            for c in collections {
                let mut on = item.collections.contains(c);
                if crate::ui::toggle(ui, &mut on, c).clicked() {
                    acts.push(MediaAct::Toggle(i, c.clone()));
                }
            }
            ui.separator();
            if let Some(name) = new_name_menu(ui, ("new_coll", &item.path), "New collection", "Add") {
                acts.push(MediaAct::NewCollection(name, Some(i)));
            }
        });
        if !item.effects.is_empty() && crate::ui::command(ui, label(icon::X, "Forget its effects")).on_hover_text("Take this file's own effects off it (and off layers playing it now). Clips made from it keep theirs").clicked() {
            acts.push(MediaAct::ForgetEffects(i));
            ui.close();
        }
        crate::ui::field_menu(ui, label(icon::PENCIL_SIMPLE, "Rename"), |ui| {
            if let Some(name) = rename_menu(ui, ("rename", &item.path), &item.name) {
                acts.push(MediaAct::Rename(i, name));
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        if crate::media::is_movie(&item.name) && crate::ui::command(ui, label(icon::FILM_STRIP, "Convert…")).on_hover_text("Make a copy that plays better in a show (Hap, ProRes, or H.264 with every frame a keyframe); see the size first").clicked() {
            acts.push(MediaAct::Convert(i));
            ui.close();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if crate::web_media::downloads_dir().is_some_and(|d| std::path::Path::new(&item.path).starts_with(d))
            && crate::ui::command(ui, label(icon::DOWNLOAD_SIMPLE, "Keep in my library")).on_hover_text("Copy it (with its credit) into your library folder, to hold onto it").clicked()
        {
            acts.push(MediaAct::Keep(vec![i]));
            ui.close();
        }
        if crate::ui::command(ui, label(icon::ARROW_CLOCKWISE, "Reload")).on_hover_text("Open the file again from disk (after you've changed it in another app)").clicked() {
            acts.push(MediaAct::Reload(i));
            ui.close();
        }
        ui.separator();
        if crate::ui::command(ui, label(icon::PLUS, "Put on a new layer")).clicked() {
            acts.push(MediaAct::NewLayer(i));
            ui.close();
        }
        #[cfg(target_os = "macos")]
        if crate::ui::command(ui, label(icon::FOLDER_OPEN, "Reveal in Finder")).clicked() {
            crate::os::reveal(&item.path);
            ui.close();
        }
        // For reports: what the file is called and where it is.
        let file_name = std::path::Path::new(&item.path).file_name().map_or_else(|| item.name.clone(), |n| n.to_string_lossy().into_owned());
        if crate::ui::command(ui, label(icon::COPY, "Copy file name")).on_hover_text(file_name.clone()).clicked() {
            ui.ctx().copy_text(file_name);
            ui.close();
        }
        if crate::ui::command(ui, label(icon::COPY, "Copy full path")).clicked() {
            ui.ctx().copy_text(item.path.clone());
            ui.close();
        }
        if crate::ui::command(ui, label(icon::TRASH, "Remove from bin")).clicked() {
            acts.push(MediaAct::Remove(i));
            ui.close();
        }
    });
}

/// Longest first; lengths not read yet go last.
/// What a media file brings when it plays, as (icon, words): its own effects, its speed / direction, its trim.
pub(crate) fn media_extras(item: &phosphor_engine::MediaItem) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if !item.effects.is_empty() {
        let names: Vec<&str> = item.effects.iter().map(|e| shaders::effect(&e.effect).map_or(e.effect.as_str(), |d| d.name)).collect();
        out.push((icon::MAGIC_WAND.to_string(), format!("Its own effects: {}", names.join(", "))));
    }
    if let Some(s) = &item.settings {
        if s.changes_playback() {
            let sync = if s.sync_beats > 0.0 { format!(", synced to {} beats", s.sync_beats) } else { String::new() };
            out.push((icon::SLIDERS_HORIZONTAL.to_string(), format!("Plays {} at {:.2}x{sync}", s.direction.name().to_lowercase(), s.rate)));
        }
        if s.is_cut() {
            let to = if s.loop_beats > 0.0 { format!(", a {}-beat loop", s.loop_beats) } else { format!(" to {:.0}%", s.loop_out * 100.0) };
            out.push((icon::SCISSORS.to_string(), format!("Trimmed: from {:.0}%{to}", s.loop_in * 100.0)));
        }
    }
    out
}

pub(crate) fn length_key(m: &phosphor_engine::MediaItem) -> std::cmp::Reverse<i64> {
    std::cmp::Reverse(m.duration.map_or(-1, |d| (d * 1000.0) as i64))
}

/// Media bin items as clips (name without extension, source by file type).
pub(crate) fn clip_items<'a>(items: impl Iterator<Item = &'a phosphor_engine::MediaItem>) -> Vec<(String, SourceSpec)> {
    items
        .map(|m| (crate::media::clip_name(&m.name), crate::media::spec_for_path(&m.path)))
        .collect()
}

/// The folder a file shows under: its own folder for files from a watched folder (so subfolders, e.g. one per
/// show, are groups of their own), "" for files added one by one.
fn folder_of(m: &phosphor_engine::MediaItem) -> String {
    if m.folder.is_empty() {
        return String::new();
    }
    std::path::Path::new(&m.path).parent().map_or_else(|| m.folder.clone(), |p| p.to_string_lossy().into_owned())
}
