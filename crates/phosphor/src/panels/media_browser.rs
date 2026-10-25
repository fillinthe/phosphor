//! The Media panel: sources down the left (`media_sources`; a drop-down when the panel is narrow) and one grid for
//! the source picked, folders as tiles. The old one-list view is View › Classic view.

use super::media_sources::{self as src, MediaSource, SourceAct};
use super::*;

pub(crate) fn media_browser(ui: &mut egui::Ui, st: &mut State) {
    #[cfg(not(target_arch = "wasm32"))]
    lengths(ui, st);
    let now = ui.ctx().input(|i| i.time);
    forget_gone_sources(st);
    #[cfg(not(target_arch = "wasm32"))]
    following_poll(st);
    let entries = src::entries(st);
    let mut act = None;
    // The sources go down the left from the Library's usual width (about 400 points); narrower, a drop-down.
    if ui.available_width() >= 340.0 {
        egui::Panel::left("media_sources")
            .resizable(true)
            .default_size(150.0)
            .size_range(110.0..=320.0)
            .show(ui, |ui| {
                act = src::source_list(ui, st, &entries);
            });
    } else {
        crate::ui::toolbar(ui, |ui| act = src::source_dropdown(ui, st, &entries));
    }
    let mut acts: Vec<MediaAct> = Vec::new();
    if let Some(a) = act {
        source_act(st, a, &mut acts, ui.ctx());
    }
    #[cfg(not(target_arch = "wasm32"))]
    if st.media_view.follow_open {
        if let Some(account) = src::follow_box(ui, &mut st.media_view) {
            follow(st, account, ui.ctx());
        }
        ui.add_space(4.0);
    }
    if st.media_view.source == MediaSource::Online {
        #[cfg(not(target_arch = "wasm32"))]
        online_panel(ui, st);
        return;
    }
    tools(ui, st);
    browse(ui, st, &mut acts);
    apply_acts(st, acts, now);
}

/// Sort by Length: asks for the lengths not known yet (read in the background) and keeps the ones that came in.
#[cfg(not(target_arch = "wasm32"))]
fn lengths(ui: &egui::Ui, st: &mut State) {
    let view = &mut st.media_view;
    for (path, secs) in view.durations.take() {
        if let Some(m) = st.comp.media_bin.iter_mut().find(|m| m.path == path) {
            m.duration = Some(secs);
        }
    }
    if view.sort != MediaSort::Length {
        return;
    }
    let mut missing = false;
    for m in st.comp.media_bin.iter().filter(|m| m.duration.is_none()) {
        view.durations.want(&m.path);
        missing = true;
    }
    if missing {
        // (Lengths come in on their own thread: look again shortly.)
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
    }
}

/// A source whose folder stopped being watched, or whose collection was deleted, goes back to All media.
fn forget_gone_sources(st: &mut State) {
    let gone = match &st.media_view.source {
        #[cfg(not(target_arch = "wasm32"))]
        MediaSource::Folder(root) => !st.folders.folders.contains(root),
        #[cfg(target_arch = "wasm32")]
        MediaSource::Folder(_) => true,
        MediaSource::Collection(c) => !st.comp.media_collections.contains(c),
        _ => false,
    };
    if gone {
        st.media_view.show(MediaSource::All, None);
    }
}

fn source_act(st: &mut State, act: SourceAct, acts: &mut Vec<MediaAct>, ctx: &egui::Context) {
    let _ = ctx;
    match act {
        SourceAct::Pick(s, dir) => st.media_view.show(s, dir),
        SourceAct::Watch => {
            #[cfg(not(target_arch = "wasm32"))]
            st.dialogs.pick_folder(crate::dialogs::Purpose::WatchFolder);
        }
        SourceAct::Follow => st.media_view.follow_open = !st.media_view.follow_open,
        SourceAct::NewCollection => acts.push(MediaAct::NewCollection(String::new(), None)),
        SourceAct::Unwatch(f) => acts.push(MediaAct::Unwatch(f)),
        SourceAct::DeleteCollection(c) => acts.push(MediaAct::DeleteCollection(c)),
        SourceAct::Unfollow(dir) => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let f = &mut st.media_view.following;
                f.accounts
                    .retain(|a| a.dir().is_none_or(|d| d.to_string_lossy() != dir));
                f.save();
                st.media_view.show(MediaSource::All, None);
            }
            #[cfg(target_arch = "wasm32")]
            let _ = dir;
        }
    }
}

/// Follows an account (or one of your lists), opens it and brings in its newest clips.
#[cfg(not(target_arch = "wasm32"))]
fn follow(st: &mut State, account: crate::online::following::Account, ctx: &egui::Context) {
    use crate::online::following::{self as fl, Following};
    let view = &mut st.media_view;
    if !view.following.add(account.clone()) {
        st.notice = Some(format!("Already following {}", account.label()));
    }
    view.following.save();
    view.follow_text.clear();
    view.follow_open = false;
    if let (Some(root), Some(dir)) = (fl::folder(), account.dir()) {
        let _ = std::fs::create_dir_all(&dir);
        view.show(MediaSource::Folder(root.to_string_lossy().into_owned()), Some(dir.to_string_lossy().into_owned()));
    }
    let one = Following { accounts: vec![account], ..view.following.clone() };
    view.refresh = Some(fl::refresh(&one, Some(ctx.clone())));
}

/// When a refresh brought files in: rescan the Following folder now (not in 10 s).
#[cfg(not(target_arch = "wasm32"))]
fn following_poll(st: &mut State) {
    // Keep in my library finished: say so, and show the copies now.
    if let Some(msg) = st.media_view.kept.lock().ok().and_then(|mut k| k.take()) {
        st.notice = Some(msg);
        if let Some(lib) = st.media_view.following.library_dir() {
            st.folders.add(lib.to_string_lossy().into_owned());
        }
    }
    let Some(p) = &st.media_view.refresh else {
        return;
    };
    let arrived = p.lock().ok().is_some_and(|mut p| {
        let n = p.new_files;
        p.new_files = 0;
        n > 0
    });
    if arrived && let Some(root) = crate::online::following::folder() {
        st.folders.add(root.to_string_lossy().into_owned());
    }
}

/// Search, videos / pictures, View, Import, From a link.
fn tools(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(Color32::GRAY));
        ui.add(
            egui::TextEdit::singleline(&mut st.media_view.search)
                .hint_text("Search here")
                .desired_width(ui.available_width()),
        );
    });
    ui.horizontal_wrapped(|ui| {
        let kinds = [
            (MediaFilter::All, "All".to_string()),
            (MediaFilter::Videos, icon::FILM_STRIP.to_string()),
            (MediaFilter::Images, icon::IMAGE.to_string()),
        ];
        segmented(ui, &mut st.media_view.only, &kinds, |_, r, k| {
            r.clone().on_hover_text(match k {
                MediaFilter::Videos => "Videos only",
                MediaFilter::Images => "Pictures only",
                _ => "Videos and pictures",
            });
        });
        ui.menu_button(label(icon::SLIDERS_HORIZONTAL, "View"), |ui| {
            ui.set_min_width(200.0);
            crate::ui::heading(ui, "Sort by");
            crate::ui::toolbar(ui, |ui| {
                crate::ui::pick_value(ui, &mut st.media_view.sort, MediaSort::Name, "Name");
                crate::ui::pick_value(ui, &mut st.media_view.sort, MediaSort::Newest, "Newest");
                crate::ui::pick_value(ui, &mut st.media_view.sort, MediaSort::Type, "Type");
                crate::ui::pick_value(ui, &mut st.media_view.sort, MediaSort::Length, "Length");
            });
            Level::range(&mut st.media_view.tile, 72.0, 240.0, "Tile size")
                .default(128.0)
                .width(180.0)
                .show(ui);
            crate::ui::toggle(ui, &mut st.media_view.classic, "Classic view")
                .on_hover_text("One list with folders as groups (off: sources down the left)");
        })
        .response
        .on_hover_text("Sort, tile size");
        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.menu_button(label(icon::PLUS, "Add"), |ui| {
                if crate::ui::command(ui, label(icon::FOLDER_OPEN, "Files…"))
                    .on_hover_text("Add files")
                    .clicked()
                {
                    let exts = crate::media::dialog_extensions();
                    st.dialogs
                        .pick_files(crate::dialogs::Purpose::ImportFiles, Some(("Media", &exts)));
                    ui.close();
                }
                if crate::ui::command(ui, label(icon::EYE, "Watch a folder…"))
                    .on_hover_text(
                        "Every clip in a folder and its subfolders; new files show up on their own",
                    )
                    .clicked()
                {
                    st.dialogs.pick_folder(crate::dialogs::Purpose::WatchFolder);
                    ui.close();
                }
                if crate::ui::command(ui, label(icon::LINK, "From a link…"))
                    .on_hover_text("The video or GIF in a post on X, Instagram, TikTok, YouTube…")
                    .clicked()
                {
                    st.link_import.open = true;
                    ui.close();
                }
                if crate::ui::command(ui, label(icon::USER_PLUS, "Follow an account…"))
                    .on_hover_text("An X or Instagram account's newest clips, kept on this Mac")
                    .clicked()
                {
                    st.media_view.follow_open = true;
                    ui.close();
                }
            })
            .response
            .on_hover_text("Files, a watched folder, a link, an account to follow");
            if st.folders.scanning() {
                ui.spinner();
            }
        }
    });
}

/// The grid for the source picked (with its header: path, account, collection).
fn browse(ui: &mut egui::Ui, st: &mut State, acts: &mut Vec<MediaAct>) {
    let source = st.media_view.source.clone();
    // A folder: where in it, and the path back out.
    let mut dir: Option<String> = None;
    // A followed account's folder (Load more under its tiles).
    let mut account: Option<String> = None;
    if let MediaSource::Folder(root) = &source {
        let at = st
            .media_view
            .dir
            .clone()
            .filter(|d| src::inside(d, root))
            .unwrap_or_else(|| root.clone());
        let following = src::following_root().as_deref() == Some(root.as_str());
        // A followed account's folder is its root (the Following folder above it isn't shown).
        let base = if following {
            account_of(&at, root).unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };
        let mut go = None;
        // The path back out (once inside a subfolder: at the top, the source list already says where).
        let steps = src::crumbs(&base, &at);
        if steps.len() > 1 {
            crate::ui::toolbar(ui, |ui| {
                let last = steps.len() - 1;
                for (k, (name, path)) in steps.into_iter().enumerate() {
                    if k > 0 {
                        ui.label(RichText::new(icon::CARET_RIGHT).color(Color32::GRAY));
                    }
                    if k == last {
                        ui.label(RichText::new(name).strong());
                    } else if crate::ui::small_command(ui, name).clicked() {
                        go = Some(path);
                    }
                }
            });
        }
        if let Some(p) = go {
            st.media_view.dir = Some(p);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if st.folders.missing(root) {
            ui.colored_label(WARN, "This folder isn't there now (drive unplugged or network down): its files come back when it is.");
        }
        #[cfg(not(target_arch = "wasm32"))]
        if following {
            account = Some(base.clone());
            match src::account_header(ui, &mut st.media_view, &base) {
                Some(src::FollowAct::KeepAll) => {
                    let all = (0..st.comp.media_bin.len()).filter(|i| src::inside(&st.comp.media_bin[*i].path, &base)).collect();
                    acts.push(MediaAct::Keep(all));
                }
                Some(src::FollowAct::PickLibrary) => st.dialogs.pick_folder(crate::dialogs::Purpose::LibraryFolder),
                Some(act) => follow_fetch(st, act, &base, ui.ctx()),
                None => {}
            }
        }
        dir = Some(
            st.media_view
                .dir
                .clone()
                .filter(|d| src::inside(d, root))
                .unwrap_or(base),
        );
    }
    if let MediaSource::Collection(c) = &source {
        super::collections::collection_header(ui, st, c, acts);
    }

    // Which files, in what order.
    let view = &st.media_view;
    let bin = &st.comp.media_bin;
    let q = view.search.to_lowercase();
    let searching = !q.is_empty();
    let kind_ok = |m: &phosphor_engine::MediaItem| match view.only {
        MediaFilter::Videos => crate::media::is_movie(&m.name),
        MediaFilter::Images => !crate::media::is_movie(&m.name),
        _ => true,
    };
    let text_ok = |m: &phosphor_engine::MediaItem| {
        !searching || m.name.to_lowercase().contains(&q) || m.path.to_lowercase().contains(&q)
    };
    let in_source: Vec<usize> = match &source {
        MediaSource::Recent => (0..bin.len()).rev().take(src::RECENT).collect(),
        _ => (0..bin.len())
            .filter(|i| {
                let m = &bin[*i];
                match &source {
                    MediaSource::All | MediaSource::Recent | MediaSource::Online => true,
                    MediaSource::Favorites => m.favorite,
                    MediaSource::Added => m.folder.is_empty(),
                    MediaSource::Folder(root) => {
                        m.folder == *root && dir.as_deref().is_none_or(|d| src::inside(&m.path, d))
                    }
                    MediaSource::Collection(c) => st.comp.in_collection(m, c),
                }
            })
            .collect(),
    };
    let wanted = |i: &usize| kind_ok(&bin[*i]) && text_ok(&bin[*i]);
    // In a folder (not searching): its subfolders as tiles, then the files right in it.
    let (subs, mut shown) = match (&dir, searching) {
        (Some(d), false) => {
            let (subs, files) = src::folder_view(
                in_source
                    .iter()
                    .copied()
                    .filter(wanted)
                    .map(|i| (i, bin[i].path.as_str())),
                d,
            );
            (subs, files)
        }
        _ => (
            Vec::new(),
            in_source.into_iter().filter(wanted).collect::<Vec<usize>>(),
        ),
    };
    if source != MediaSource::Recent {
        match view.sort {
            MediaSort::Name => shown.sort_by_key(|i| bin[*i].name.to_lowercase()),
            MediaSort::Newest => shown.sort_by_key(|i| std::cmp::Reverse(*i)),
            MediaSort::Type => shown.sort_by_key(|i| {
                (
                    !crate::media::is_movie(&bin[*i].name),
                    bin[*i].name.to_lowercase(),
                )
            }),
            MediaSort::Length => shown.sort_by_key(|i| super::media::length_key(&bin[*i])),
        }
    }
    let tile_w = view.tile;

    // What to do with the files shown.
    let in_collection = matches!(source, MediaSource::Collection(_));
    let title = match &source {
        MediaSource::Favorites => "Favourites".to_string(),
        MediaSource::Folder(_) => dir.as_deref().map_or_else(
            || "Folder".into(),
            |d| {
                std::path::Path::new(d)
                    .file_name()
                    .map_or_else(|| d.to_string(), |n| n.to_string_lossy().into_owned())
            },
        ),
        _ => "Shuffle".to_string(),
    };
    ui.horizontal_wrapped(|ui| {
        if !in_collection {
            let tip = "A new layer with every file shown here (after search and sort) as its clips, the first one playing";
            if crate::ui::command_if(ui, !shown.is_empty(), label(icon::SQUARES_FOUR, &format!("Clip layer ({})", shown.len()))).on_hover_text(tip).clicked() {
                acts.push(MediaAct::ShownToLayer(shown.clone()));
            }
        }
        if crate::ui::command_if(ui, !shown.is_empty(), label(icon::DICE_FIVE, "Random")).on_hover_text("Play one of the files shown here, at random, on the selected layer (click again for another)").clicked() {
            acts.push(MediaAct::RandomOne(shown.clone()));
        }
        if !in_collection && crate::ui::command_if(ui, shown.len() > 1, label(icon::SHUFFLE, "Shuffle layer")).on_hover_text("A new layer with the files shown here as clips, playing them in random order by itself (autopilot)").clicked() {
            acts.push(MediaAct::ShuffleLayer(shown.clone(), title.clone()));
        }
        if st.selected.is_some() && crate::ui::command_if(ui, !shown.is_empty(), label(icon::ARROW_DOWN, "Fill selected layer")).on_hover_text("Puts the files shown here into the selected layer's clip slots, in order").clicked() {
            acts.push(MediaAct::FillFolder(shown.clone()));
        }
    });

    // Where a click plays (the selected layer, else a new one) and every layer for "Play on".
    let layers: Vec<(u64, String)> = st
        .comp
        .layers
        .iter()
        .rev()
        .map(|l| (l.id, l.name.clone()))
        .collect();
    let click_target = st
        .selected
        .and_then(|id| layers.iter().find(|(l, _)| *l == id))
        .map(|(_, n)| n.clone());
    let mut playing: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for l in st.comp.layers.iter().filter(|l| l.enabled) {
        if let SourceSpec::Movie { path: Some(p), .. } | SourceSpec::Image { path: Some(p), .. } =
            &l.source
        {
            playing.entry(p.clone()).or_default().push(l.name.clone());
        }
    }
    let uses = st.comp.media_uses();
    let targets = MediaTargets {
        layers: &layers,
        click: click_target.as_deref(),
        playing: &playing,
        uses: &uses,
    };
    let collections = st.comp.media_collections.clone();
    let mut open_dir = None;
    let mut more = false;
    let State { comp, thumbs, media_view, .. } = st;
    #[cfg(target_arch = "wasm32")]
    let _ = &media_view;
    crate::ui::column_scroll(ui)
        .id_salt("media_grid")
        .auto_shrink(false)
        .show(ui, |ui| {
            // As many columns as fit at the tile size, stretched to fill the width (no ragged gap on the right).
            let (gap, room) = (8.0, ui.available_width() - 2.0);
            // (At least two columns when two of 80 points fit: one huge tile wastes a narrow column.)
            let cols = ((room + gap) / (tile_w + gap))
                .floor()
                .max(if room >= 160.0 + gap { 2.0 } else { 1.0 });
            let tile_w = ((room - gap * (cols - 1.0)) / cols)
                .floor()
                .min(tile_w * 1.6);
            if shown.is_empty() && subs.is_empty() {
                hint(
                    ui,
                    empty_text(&source, comp.media_bin.is_empty(), searching),
                );
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                for (path, n, first) in &subs {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
                    let picture = comp.media_bin.get(*first).map(|m| m.path.as_str());
                    if src::folder_tile(ui, &name, *n, tile_w, picture, thumbs).clicked() {
                        open_dir = Some(path.clone());
                    }
                }
                for i in &shown {
                    media_tile(
                        ui,
                        &comp.media_bin[*i],
                        *i,
                        tile_w,
                        thumbs,
                        &collections,
                        &targets,
                        acts,
                    );
                }
            });
        #[cfg(not(target_arch = "wasm32"))]
        if account.is_some() && !searching {
            ui.add_space(6.0);
            more = src::load_more(ui, media_view);
        }
        });
    if let Some(d) = open_dir {
        st.media_view.dir = Some(d);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if more && let Some(a) = account {
        follow_fetch(st, src::FollowAct::More, &a, ui.ctx());
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (more, account);
}

/// Refresh every account, or Load more of the one in `account_dir` (how many clips it has now: its files here).
#[cfg(not(target_arch = "wasm32"))]
fn follow_fetch(st: &mut State, act: src::FollowAct, account_dir: &str, ctx: &egui::Context) {
    use crate::online::following as fl;
    let view = &mut st.media_view;
    view.refresh = Some(match act {
        src::FollowAct::Refresh => fl::refresh(&view.following, Some(ctx.clone())),
        src::FollowAct::KeepAll | src::FollowAct::PickLibrary => return,
        src::FollowAct::More => {
            let Some(a) = view.following.accounts.iter().find(|a| a.dir().is_some_and(|d| d.to_string_lossy() == account_dir)).cloned() else { return };
            fl::more(&view.following, &a, Some(ctx.clone()))
        }
    });
}

fn empty_text(source: &MediaSource, bin_empty: bool, searching: bool) -> &'static str {
    if searching {
        return "Nothing here matches the search.";
    }
    match source {
        _ if bin_empty && *source == MediaSource::All => {
            "Empty. Import files, watch a folder, or drop files onto the window."
        }
        MediaSource::Favorites => "No favourites yet: click the star on a tile.",
        MediaSource::Added => {
            "Files you add one by one show here: Import…, or drop files onto the window."
        }
        MediaSource::Folder(_) => {
            "No videos or pictures in this folder (new ones show up on their own)."
        }
        MediaSource::Collection(_) => {
            "Empty: add files from a tile's right-click menu, or a whole folder above."
        }
        _ => "Nothing here.",
    }
}

/// The account folder (`root/@name (Site)`) that `dir` is in.
fn account_of(dir: &str, root: &str) -> Option<String> {
    let rel = std::path::Path::new(dir).strip_prefix(root).ok()?;
    let first = rel.components().next()?;
    Some(
        std::path::Path::new(root)
            .join(first)
            .to_string_lossy()
            .into_owned(),
    )
}
