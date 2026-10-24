//! The Media panel's sources: a list down the left (a drop-down when the panel is narrow) and one grid for the source
//! picked. Favourites, Recently added, All; This Mac (files added one by one, each watched folder); Network (watched
//! folders on a network drive, marked when missing); Online (library search, followed X / Instagram accounts);
//! Collections. In a folder its subfolders are tiles to go into, with a path to step back out.

use super::*;

/// What the grid shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum MediaSource {
    #[default]
    All,
    Favorites,
    Recent,
    /// Files added one by one (not from a watched folder).
    Added,
    /// A watched folder (its root; `MediaView::dir` = the subfolder open in it).
    Folder(String),
    /// Library search (Online).
    Online,
    Collection(String),
}

/// How many files "Recently added" shows.
pub const RECENT: usize = 40;

/// One row of the source list.
pub(crate) struct Entry {
    pub heading: Option<&'static str>,
    pub label: String,
    /// Count, or a badge ("3 new", "missing").
    pub right: String,
    pub badge: bool,
    pub pick: Pick,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pick {
    Source(MediaSource, Option<String>),
    WatchFolder,
    Follow,
    NewCollection,
}

/// Is `path` inside `dir` (any depth)?
pub(crate) fn inside(path: &str, dir: &str) -> bool {
    std::path::Path::new(path).starts_with(dir)
}

/// The subfolders of `dir` that hold media (at any depth), with how many files each holds and the first file (its
/// tile's picture), by name; and the files right in `dir`.
pub(crate) fn folder_view<'a>(
    items: impl Iterator<Item = (usize, &'a str)>,
    dir: &str,
) -> (Vec<(String, usize, usize)>, Vec<usize>) {
    let mut subs: Vec<(String, usize, usize)> = Vec::new();
    let mut files = Vec::new();
    for (i, path) in items {
        let Ok(rel) = std::path::Path::new(path).strip_prefix(dir) else {
            continue;
        };
        let mut parts = rel.components();
        let (Some(first), Some(_)) = (parts.next(), parts.next()) else {
            files.push(i);
            continue;
        };
        let sub = std::path::Path::new(dir)
            .join(first)
            .to_string_lossy()
            .into_owned();
        match subs.iter_mut().find(|(s, _, _)| *s == sub) {
            Some(s) => s.1 += 1,
            None => subs.push((sub, 1, i)),
        }
    }
    subs.sort_by_key(|(s, _, _)| s.to_lowercase());
    (subs, files)
}

/// The path from a watched folder's root to `dir`: (name, path) per step.
pub(crate) fn crumbs(root: &str, dir: &str) -> Vec<(String, String)> {
    let name = |p: &std::path::Path| {
        p.file_name().map_or_else(
            || p.to_string_lossy().into_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    };
    let root_p = std::path::Path::new(root);
    let mut out = vec![(name(root_p), root.to_string())];
    if let Ok(rel) = std::path::Path::new(dir).strip_prefix(root) {
        let mut at = root_p.to_path_buf();
        for c in rel.components() {
            at.push(c);
            out.push((
                c.as_os_str().to_string_lossy().into_owned(),
                at.to_string_lossy().into_owned(),
            ));
        }
    }
    out
}

fn folder_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// The Following folder (followed accounts' clips), if there is one.
pub(crate) fn following_root() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    return crate::online::following::folder().map(|f| f.to_string_lossy().into_owned());
    #[cfg(target_arch = "wasm32")]
    None
}

/// Every row of the source list, from the bin, the watched folders and the followed accounts. Built once per frame.
pub(crate) fn entries(st: &State) -> Vec<Entry> {
    let bin = &st.comp.media_bin;
    let count =
        |f: &dyn Fn(&phosphor_engine::MediaItem) -> bool| bin.iter().filter(|m| f(m)).count();
    let mut out = Vec::new();
    let mut add = |heading, label: String, right: String, badge, pick| {
        out.push(Entry {
            heading,
            label,
            right,
            badge,
            pick,
        })
    };
    add(
        None,
        format!("{} Favourites", icon::STAR),
        count(&|m| m.favorite).to_string(),
        false,
        Pick::Source(MediaSource::Favorites, None),
    );
    add(
        None,
        format!("{} Recently added", icon::CLOCK),
        bin.len().min(RECENT).to_string(),
        false,
        Pick::Source(MediaSource::Recent, None),
    );
    add(
        None,
        format!("{} All media", icon::SQUARES_FOUR),
        bin.len().to_string(),
        false,
        Pick::Source(MediaSource::All, None),
    );
    let following = following_root();
    let (mut local, mut network): (Vec<&String>, Vec<&String>) = (Vec::new(), Vec::new());
    for f in &st.folders.folders {
        if Some(f) == following.as_ref() {
            continue;
        }
        if st.folders.state.get(f).is_some_and(|s| s.network) {
            network.push(f)
        } else {
            local.push(f)
        }
    }
    let folder_row = |f: &String, glyph: &str| {
        let (right, badge) = if st.folders.missing(f) {
            ("missing".to_string(), true)
        } else {
            (count(&|m| m.folder == *f).to_string(), false)
        };
        (
            format!("{glyph} {}", folder_name(f)),
            right,
            badge,
            Pick::Source(MediaSource::Folder(f.clone()), Some(f.clone())),
        )
    };
    add(
        Some("THIS MAC"),
        format!("{} Added files", icon::FILES),
        count(&|m| m.folder.is_empty()).to_string(),
        false,
        Pick::Source(MediaSource::Added, None),
    );
    for f in local {
        let (l, r, b, p) = folder_row(f, icon::FOLDER);
        add(None, l, r, b, p);
    }
    add(
        None,
        format!("{} Watch a folder…", icon::EYE),
        String::new(),
        false,
        Pick::WatchFolder,
    );
    if !network.is_empty() {
        let mut first = true;
        for f in network {
            let (l, r, b, p) = folder_row(f, icon::HARD_DRIVES);
            add(
                if std::mem::take(&mut first) {
                    Some("NETWORK")
                } else {
                    None
                },
                l,
                r,
                b,
                p,
            );
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        add(
            Some("ONLINE"),
            format!("{} Search libraries", icon::GLOBE),
            String::new(),
            false,
            Pick::Source(MediaSource::Online, None),
        );
        let progress = st
            .media_view
            .refresh
            .as_ref()
            .and_then(|p| p.lock().ok().map(|p| p.lines.clone()))
            .unwrap_or_default();
        if let Some(root) = &following {
            for a in &st.media_view.following.accounts {
                let Some(dir) = a.dir().map(|d| d.to_string_lossy().into_owned()) else {
                    continue;
                };
                let label = a.label();
                let news = progress
                    .iter()
                    .find(|(w, _)| *w == label)
                    .map(|(_, what)| what.clone())
                    .filter(|w| w.ends_with(" new") || w == "looking…");
                let (right, badge) = match news {
                    Some(n) => (n, true),
                    None => (count(&|m| inside(&m.path, &dir)).to_string(), false),
                };
                let logo = match a.site {
                    crate::online::following::Site::X => icon::X_LOGO,
                    crate::online::following::Site::Instagram => icon::INSTAGRAM_LOGO,
                };
                add(
                    None,
                    format!("{logo} {}", a.name),
                    right,
                    badge,
                    Pick::Source(MediaSource::Folder(root.clone()), Some(dir)),
                );
            }
        }
        add(
            None,
            format!("{} Follow…", icon::USER_PLUS),
            String::new(),
            false,
            Pick::Follow,
        );
    }
    let mut first = true;
    for c in &st.comp.media_collections {
        let n = count(&|m| st.comp.in_collection(m, c));
        add(
            if std::mem::take(&mut first) {
                Some("COLLECTIONS")
            } else {
                None
            },
            format!("{} {c}", icon::BOOKMARKS_SIMPLE),
            n.to_string(),
            false,
            Pick::Source(MediaSource::Collection(c.clone()), None),
        );
    }
    add(
        if first { Some("COLLECTIONS") } else { None },
        format!("{} New collection", icon::PLUS),
        String::new(),
        false,
        Pick::NewCollection,
    );
    out
}

impl Entry {
    fn picked(&self, view: &MediaView) -> bool {
        match &self.pick {
            Pick::Source(MediaSource::Folder(root), Some(dir))
                if Some(root) == following_root().as_ref() =>
            {
                matches!(&view.source, MediaSource::Folder(r) if r == root)
                    && view.dir.as_deref().is_some_and(|d| inside(d, dir))
            }
            Pick::Source(s, _) => *s == view.source,
            Pick::Follow => view.follow_open,
            _ => false,
        }
    }
}

/// What a source-list row asked for.
pub(crate) enum SourceAct {
    Pick(MediaSource, Option<String>),
    Watch,
    Follow,
    NewCollection,
    Unwatch(String),
    Unfollow(String),
    DeleteCollection(String),
}

/// The source list down the left of the panel.
pub(crate) fn source_list(ui: &mut egui::Ui, st: &State, entries: &[Entry]) -> Option<SourceAct> {
    let mut act = None;
    crate::ui::column_scroll(ui)
        .id_salt("media_sources")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            for e in entries {
                if let Some(h) = e.heading {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(h)
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                let r =
                    crate::ui::list_row(ui, e.picked(&st.media_view), &e.label, &e.right, e.badge);
                if r.clicked() {
                    act = Some(entry_act(&e.pick));
                }
                if let Some(a) = entry_menu(&r, e, st) {
                    act = Some(a);
                }
            }
        });
    act
}

/// The source list as a drop-down (a narrow panel).
pub(crate) fn source_dropdown(
    ui: &mut egui::Ui,
    st: &State,
    entries: &[Entry],
) -> Option<SourceAct> {
    let mut act = None;
    let current = entries
        .iter()
        .find(|e| e.picked(&st.media_view))
        .map_or_else(|| "All media".to_string(), |e| e.label.clone());
    crate::ui::dropdown(ui, "media_source_pick", ui.available_width().min(260.0))
        .selected_text(current)
        .height(600.0)
        .show_ui(ui, |ui| {
            for e in entries {
                if let Some(h) = e.heading {
                    ui.label(
                        RichText::new(h)
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                let text = if e.right.is_empty() {
                    e.label.clone()
                } else {
                    format!("{}  ({})", e.label, e.right)
                };
                if crate::ui::pick_item(ui, e.picked(&st.media_view), text).clicked() {
                    act = Some(entry_act(&e.pick));
                }
            }
        });
    act
}

fn entry_act(p: &Pick) -> SourceAct {
    match p {
        Pick::Source(s, d) => SourceAct::Pick(s.clone(), d.clone()),
        Pick::WatchFolder => SourceAct::Watch,
        Pick::Follow => SourceAct::Follow,
        Pick::NewCollection => SourceAct::NewCollection,
    }
}

/// A row's right-click menu: stop watching / unfollow / delete, and show the folder.
fn entry_menu(r: &egui::Response, e: &Entry, st: &State) -> Option<SourceAct> {
    let mut act = None;
    let following = following_root();
    match &e.pick {
        Pick::Source(MediaSource::Folder(root), Some(dir)) => {
            let account = Some(root) == following.as_ref();
            r.right_menu(|ui| {
                if crate::ui::command(ui, label(icon::FOLDER_OPEN, "Show the folder")).clicked() {
                    crate::os::open(dir);
                    ui.close();
                }
                if account {
                    if danger_button(ui, label(icon::EYE_SLASH, "Unfollow"))
                        .on_hover_text(
                            "Its clips stay in the folder (Show the folder to delete them)",
                        )
                        .clicked()
                    {
                        act = Some(SourceAct::Unfollow(dir.clone()));
                        ui.close();
                    }
                } else if crate::ui::command(ui, label(icon::EYE_SLASH, "Stop watching"))
                    .on_hover_text("Its files leave the bin; the folder stays on disk")
                    .clicked()
                {
                    act = Some(SourceAct::Unwatch(root.clone()));
                    ui.close();
                }
            });
        }
        Pick::Source(MediaSource::Collection(c), _) => {
            r.right_menu(|ui| {
                if danger_button(ui, label(icon::TRASH, "Delete collection"))
                    .on_hover_text("The media stays in the bin")
                    .clicked()
                {
                    act = Some(SourceAct::DeleteCollection(c.clone()));
                    ui.close();
                }
            });
        }
        _ => {}
    }
    let _ = st;
    act
}

/// A subfolder in the grid: its first file's picture behind a folder mark, its name and count. Click to go in.
pub(crate) fn folder_tile(
    ui: &mut egui::Ui,
    name: &str,
    count: usize,
    w: f32,
    picture: Option<&str>,
    thumbs: &mut crate::thumbs::Thumbs,
) -> egui::Response {
    let h = w * 9.0 / 16.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h + 18.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let pic = egui::Rect::from_min_size(rect.min, egui::vec2(w, h));
    let p = ui.painter();
    p.rect_filled(pic, 4.0, Color32::from_gray(24));
    if let Some(tex) = picture.and_then(|path| thumbs.get(path)) {
        crate::ui::paint_texture(p, tex, pic.shrink(1.0));
        p.rect_filled(pic, 4.0, Color32::from_black_alpha(150));
    }
    p.text(
        pic.center(),
        egui::Align2::CENTER_CENTER,
        icon::FOLDER,
        egui::FontId::proportional((h * 0.4).clamp(18.0, 44.0)),
        Color32::from_gray(210),
    );
    let stroke = if resp.hovered() {
        Stroke::new(1.5, accent())
    } else {
        Stroke::new(1.0, Color32::from_gray(50))
    };
    p.rect_stroke(pic, 4.0, stroke, StrokeKind::Inside);
    let max_chars = ((w / 6.5) as usize).max(8);
    let mut text = format!("{name} · {count}");
    if text.chars().count() > max_chars {
        text = format!("{}…", text.chars().take(max_chars - 1).collect::<String>());
    }
    p.text(
        egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0),
        egui::Align2::LEFT_BOTTOM,
        text,
        egui::FontId::proportional(11.0),
        Color32::LIGHT_GRAY,
    );
    resp.on_hover_text(format!("{name}: {count} files · click to open"))
}

/// The "Follow an account" box: the account (name or link), its site, Follow.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn follow_box(
    ui: &mut egui::Ui,
    view: &mut MediaView,
) -> Option<(crate::online::following::Site, String)> {
    use crate::online::following::{Site, handle};
    let mut out = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        crate::ui::heading(ui, label(icon::PLUS, "Follow an account"));
        hint(ui, "Its newest clips come into Media and stay on this Mac (X: animated GIFs; Instagram: videos). Refresh brings in new posts.");
        let r = crate::ui::text_field(ui, &mut view.follow_text, "@name or a link to the account", Some(ui.available_width().min(320.0)));
        let typed = handle(&view.follow_text);
        if let Some((Some(site), _)) = &typed {
            view.follow_site = *site;
        }
        crate::ui::toolbar(ui, |ui| {
            let opts: Vec<(Site, String)> = Site::ALL.iter().map(|s| (*s, s.name().to_string())).collect();
            segmented(ui, &mut view.follow_site, &opts, |_, _, _| {});
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (crate::ui::action_button(ui, crate::ui::Act::Go, "Follow").clicked() || enter)
                && let Some((_, name)) = &typed
            {
                out = Some((view.follow_site, name.clone()));
            }
            if crate::ui::command(ui, "Cancel").clicked() {
                view.follow_open = false;
            }
        });
        if !view.follow_text.trim().is_empty() && typed.is_none() {
            ui.colored_label(WARN, "That isn't an account name");
        }
        browser_pick(ui, view);
    });
    out
}

/// "Signed in with": the browser whose sign-in lists followed accounts' posts.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn browser_pick(ui: &mut egui::Ui, view: &mut MediaView) {
    crate::ui::toolbar(ui, |ui| {
        ui.label("Signed in with");
        let current = view
            .following
            .browser
            .clone()
            .unwrap_or_else(|| "pick a browser".into());
        let mut changed = false;
        crate::ui::dropdown(ui, "following_browser", 110.0)
            .selected_text(current)
            .show_ui(ui, |ui| {
                for b in crate::web_media::BROWSERS {
                    if crate::ui::pick_item(ui, view.following.browser.as_deref() == Some(b), b).clicked() {
                        view.following.browser = Some(b.to_string());
                        changed = true;
                    }
                }
            })
            .response
            .on_hover_text("X and Instagram only list an account's posts to people signed in: gallery-dl uses that browser's sign-in (only to list the posts)");
        if changed {
            view.following.save();
        }
    });
    if !*view
        .has_gallery_dl
        .get_or_insert_with(|| crate::web_media::find_gallery_dl().is_some())
    {
        ui.colored_label(WARN, crate::online::following::NO_TOOL);
    }
}

/// A followed account's header: Refresh (all accounts) and what the last refresh found.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn account_header(ui: &mut egui::Ui, view: &mut MediaView, account_dir: &str) -> bool {
    let mut refresh = false;
    let running = view
        .refresh
        .as_ref()
        .and_then(|p| p.lock().ok().map(|p| p.running))
        .unwrap_or(false);
    crate::ui::toolbar(ui, |ui| {
        if crate::ui::command_if(ui, !running, label(icon::ARROW_CLOCKWISE, "Refresh"))
            .on_hover_text("Look for new posts on every followed account")
            .clicked()
        {
            refresh = true;
        }
        if running {
            ui.spinner();
        }
        let label = view
            .following
            .accounts
            .iter()
            .find(|a| a.dir().is_some_and(|d| d.to_string_lossy() == account_dir))
            .map(|a| a.label());
        let line = view.refresh.as_ref().and_then(|p| {
            p.lock().ok().and_then(|p| {
                p.lines
                    .iter()
                    .find(|(w, _)| Some(w) == label.as_ref())
                    .map(|(_, l)| l.clone())
            })
        });
        if let Some(l) = line {
            hint(ui, l);
        }
    });
    browser_pick(ui, view);
    refresh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_shows_its_files_and_subfolders_as_tiles() {
        let paths = [
            "/show/a.mov",
            "/show/Intro/b.mov",
            "/show/Intro/c.mov",
            "/show/Drops/deep/d.mov",
            "/other/e.mov",
        ];
        let (subs, files) = folder_view(paths.iter().copied().enumerate(), "/show");
        assert_eq!(files, vec![0], "only the file right in the folder");
        assert_eq!(
            subs,
            vec![
                ("/show/Drops".to_string(), 1, 3),
                ("/show/Intro".to_string(), 2, 1)
            ],
            "subfolders by name, counted at any depth"
        );
        let (subs, files) = folder_view(paths.iter().copied().enumerate(), "/show/Intro");
        assert!(subs.is_empty());
        assert_eq!(files, vec![1, 2]);
    }

    #[test]
    fn the_path_steps_back_to_the_watched_folder() {
        let c = crumbs("/Users/me/Show Loops", "/Users/me/Show Loops/Intro/Slow");
        let names: Vec<&str> = c.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Show Loops", "Intro", "Slow"]);
        assert_eq!(c[1].1, "/Users/me/Show Loops/Intro");
        assert_eq!(crumbs("/a", "/a").len(), 1);
        assert!(inside("/a/b/c.mov", "/a/b") && !inside("/a/bc.mov", "/a/b"));
    }
}
