//! File › Projects… (and at start): a start screen. On the left, New / Templates / Open a file and where projects
//! are (Recent, this Mac and its drives, your account); on the right, the projects as picture cards (the picture is
//! the output when it was last saved), with search and sort. One click opens one.

use super::*;
use crate::ui::{RightMenu, label};
use egui::{Color32, RichText, Sense, Stroke};
use egui_phosphor::regular as icon;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Source {
    #[default]
    Recent,
    ThisMac,
    Account,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Sort {
    #[default]
    Newest,
    Name,
}

#[derive(Default)]
pub(crate) struct ProjectsUi {
    pub open: bool,
    /// Show the window when Phosphor starts.
    pub at_start: bool,
    pub query: String,
    pub source: Option<Source>,
    pub sort: Sort,
    #[cfg(not(target_arch = "wasm32"))]
    pub files: crate::project_files::ProjectFiles,
    /// The account's list was asked for (once per opening).
    pub cloud_asked: bool,
    /// Projects that have a picture (looked at when the window opens and when the search finishes, not each frame).
    pub pictures: std::collections::HashSet<String>,
    pub pictures_checked: bool,
}

/// What a click in the window asked for.
enum Pick {
    File(String),
    Cloud(String),
    Reveal(String),
    Forget(String),
    SignIn,
    New,
    Templates,
    OpenFile,
}

/// One card's worth: what it shows and what opening it does.
struct Card {
    name: String,
    detail: String,
    /// Seconds since 1970 (sorting).
    when: i64,
    picture: Option<String>,
    current: bool,
    there: bool,
    open: Pick,
    /// A file (right-click: Show in Finder…).
    path: Option<String>,
}

/// A project file's name without ".phosphor".
fn project_name(path: &str) -> String {
    std::path::Path::new(path).file_stem().map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// The folder it's in, home shortened to "~".
fn folder_of(path: &str) -> String {
    let folder = std::path::Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && folder.starts_with(&home) => format!("~{}", &folder[home.len()..]),
        _ => folder,
    }
}

/// Does `query` (words, any order) match this name / folder?
fn matches(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    query.split_whitespace().all(|w| text.contains(&w.to_lowercase()))
}

const CARD_W: f32 = 196.0;
const PICTURE_H: f32 = CARD_W * 9.0 / 16.0;

/// A project card: its picture (or a placeholder), the name, where / when. The current project is outlined in the
/// accent colour; one whose file isn't there (a drive unplugged) is dimmed and says so.
fn project_card(ui: &mut egui::Ui, card: &Card, picture: Option<egui::TextureId>) -> egui::Response {
    let size = egui::vec2(CARD_W, PICTURE_H + 44.0);
    let (rect, r) = ui.allocate_exact_size(size, if card.there { Sense::click() } else { Sense::hover() });
    if !ui.is_rect_visible(rect) {
        return r;
    }
    let v = ui.visuals();
    let p = ui.painter();
    let hovered = r.hovered() && card.there;
    p.rect_filled(rect, 6.0, if hovered { v.widgets.hovered.weak_bg_fill } else { v.faint_bg_color });
    let pic = egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 6.0), egui::vec2(CARD_W - 12.0, PICTURE_H - 12.0));
    let dim = if card.there { Color32::WHITE } else { Color32::from_gray(90) };
    match picture {
        Some(tex) => {
            p.image(tex, pic, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), dim);
        }
        None => {
            p.rect_filled(pic, 4.0, Color32::from_gray(18));
            let glyph = if card.path.is_none() { icon::CLOUD } else { icon::FILM_STRIP };
            p.text(pic.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(30.0), Color32::from_gray(70));
        }
    }
    if !card.there {
        p.text(pic.center(), egui::Align2::CENTER_CENTER, "Not found: drive unplugged?", egui::FontId::proportional(12.0), crate::theme::WARN);
    }
    let text_w = CARD_W - 14.0;
    let name_color = if card.current { crate::theme::accent() } else if card.there { v.strong_text_color() } else { v.weak_text_color() };
    let line = |s: &str, size: f32, color: Color32| {
        let mut job = egui::text::LayoutJob::single_section(s.to_string(), egui::TextFormat::simple(egui::FontId::proportional(size), color));
        job.wrap = egui::text::TextWrapping::truncate_at_width(text_w);
        ui.painter().layout_job(job)
    };
    let name = line(&card.name, 14.0, name_color);
    let detail = line(&card.detail, 11.0, v.weak_text_color());
    p.galley(egui::pos2(rect.left() + 7.0, pic.bottom() + 8.0), name, name_color);
    p.galley(egui::pos2(rect.left() + 7.0, pic.bottom() + 27.0), detail, v.weak_text_color());
    let stroke = if card.current { Stroke::new(2.0, crate::theme::accent()) } else if hovered { Stroke::new(1.5, v.widgets.hovered.fg_stroke.color) } else { Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color) };
    p.rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);
    let tip = card.path.clone().unwrap_or_else(|| "In your account".into());
    r.on_hover_text(tip)
}

impl PhosphorApp {
    pub(crate) fn projects_window(&mut self, ctx: &egui::Context) {
        if !self.state.projects.open {
            self.state.projects.cloud_asked = false;
            self.state.projects.pictures_checked = false;
            return;
        }
        let st = &mut self.state;
        #[cfg(not(target_arch = "wasm32"))]
        if st.projects.files.poll() {
            st.projects.pictures_checked = false;
        }
        // Which projects have pictures (and fresh copies of them: one may have been saved since).
        #[cfg(not(target_arch = "wasm32"))]
        if !st.projects.pictures_checked {
            st.projects.pictures_checked = true;
            st.projects.pictures.clear();
            let all: Vec<String> = st.recent.iter().cloned().chain(st.projects.files.found.iter().map(|f| f.path.clone())).collect();
            for project in all {
                if let Some(p) = crate::project_files::picture_path(&project) {
                    let p = p.to_string_lossy().into_owned();
                    st.thumbs.forget_sized(&p, 480);
                    if std::path::Path::new(&p).exists() {
                        st.projects.pictures.insert(project);
                    }
                }
            }
        }
        if st.account.signed_in() && !st.projects.cloud_asked {
            st.projects.cloud_asked = true;
            st.account.refresh(ctx);
        }
        // The cards of each place.
        let current = st.project_path.clone();
        let cloud_current = st.cloud_project.as_ref().map(|(id, _)| id.clone());
        #[cfg(not(target_arch = "wasm32"))]
        let (found, busy) = (st.projects.files.found.clone(), st.projects.files.busy());
        // The browser has no files of its own to look through.
        #[cfg(target_arch = "wasm32")]
        struct Found {
            path: String,
            modified: i64,
        }
        #[cfg(target_arch = "wasm32")]
        let (found, busy): (Vec<Found>, bool) = (Vec::new(), false);
        let file_card = |path: &str, when: i64, there: bool| {
            #[cfg(not(target_arch = "wasm32"))]
            let picture = st.projects.pictures.contains(path).then(|| crate::project_files::picture_path(path)).flatten().map(|p| p.to_string_lossy().into_owned());
            #[cfg(target_arch = "wasm32")]
            let picture = None;
            let ago = if when > 0 { format!(" · {}", crate::ui::ago_secs(when)) } else { String::new() };
            Card { name: project_name(path), detail: format!("{}{ago}", folder_of(path)), when, picture, current: current.as_deref() == Some(path), there, open: Pick::File(path.to_string()), path: Some(path.to_string()) }
        };
        let when_of = |path: &str| found.iter().find(|f| f.path == path).map_or(0, |f| f.modified);
        let recent: Vec<Card> = st
            .recent
            .iter()
            .enumerate()
            .map(|(k, p)| {
                #[cfg(not(target_arch = "wasm32"))]
                let there = crate::media::present::file_present(p).unwrap_or(true);
                #[cfg(target_arch = "wasm32")]
                let there = true;
                // (Recent keeps its own order for Newest: last opened first.)
                let mut c = file_card(p, when_of(p), there);
                c.when = i64::MAX - k as i64;
                if when_of(p) == 0 {
                    c.detail = folder_of(p);
                }
                c
            })
            .collect();
        let this_mac: Vec<Card> = found.iter().map(|f| file_card(&f.path, f.modified, true)).collect();
        let cloud: Vec<Card> = st
            .account
            .projects
            .iter()
            .map(|p| Card { name: p.name.clone(), detail: format!("{} · {:.0} KB", crate::ui::ago_secs(p.updated), p.size as f64 / 1024.0), when: p.updated, picture: None, current: cloud_current.as_deref() == Some(p.id.as_str()), there: true, open: Pick::Cloud(p.id.clone()), path: None })
            .collect();
        let (signed_in, cloud_busy) = (st.account.signed_in(), st.account.busy > 0);
        let ui_state = &mut st.projects;
        let source = *ui_state.source.get_or_insert(if recent.is_empty() { Source::ThisMac } else { Source::Recent });

        let mut pick: Option<Pick> = None;
        let mut search_again = false;
        let mut open = true;
        let counts = [recent.len(), found.len(), cloud.len()];
        let mut shown_cards: Vec<Card> = match source {
            Source::Recent => recent,
            Source::ThisMac => this_mac,
            Source::Account => cloud,
        };
        let q = ui_state.query.trim().to_string();
        shown_cards.retain(|c| matches(&q, &format!("{} {}", c.name, c.path.as_deref().unwrap_or(""))));
        match ui_state.sort {
            Sort::Newest => shown_cards.sort_by_key(|c| std::cmp::Reverse(c.when)),
            Sort::Name => shown_cards.sort_by_key(|c| c.name.to_lowercase()),
        }
        let ui_state = &mut st.projects;
        let thumbs = &mut st.thumbs;
        egui::Window::new("Projects")
            .collapsible(false)
            .resizable(true)
            .default_size([880.0, 560.0])
            .min_size([560.0, 360.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Panel::left("projects_sources").resizable(false).exact_size(190.0).show(ui, |ui| {
                    ui.add_space(4.0);
                    if crate::ui::action_button_sized(ui, crate::ui::Act::Go, format!("{} New project", icon::PLUS), egui::vec2(178.0, crate::ui::CONTROL_H + 6.0)).clicked() {
                        pick = Some(Pick::New);
                    }
                    ui.add_space(2.0);
                    if crate::ui::wide_button(ui, label(icon::SQUARES_FOUR, "From a template…"), 178.0).clicked() {
                        pick = Some(Pick::Templates);
                    }
                    if crate::ui::wide_button(ui, label(icon::FOLDER_OPEN, "Open a file…"), 178.0).clicked() {
                        pick = Some(Pick::OpenFile);
                    }
                    ui.add_space(10.0);
                    crate::ui::hint(ui, "PROJECTS");
                    let places = [(Source::Recent, format!("{} Recent", icon::CLOCK_COUNTER_CLOCKWISE), counts[0]), (Source::ThisMac, format!("{} This Mac & drives", icon::HARD_DRIVES), counts[1]), (Source::Account, format!("{} Your account", icon::CLOUD), counts[2])];
                    for (s, name, n) in places {
                        if crate::ui::list_row(ui, source == s, &name, &n.to_string(), false).clicked() {
                            ui_state.source = Some(s);
                        }
                    }
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                        ui.add_space(4.0);
                        crate::ui::toggle(ui, &mut ui_state.at_start, "Show at start").on_hover_text("Open this window when Phosphor starts");
                    });
                });
                // The cards.
                ui.add_space(2.0);
                egui::Frame::NONE.inner_margin(egui::Margin { left: 12, right: 4, top: 0, bottom: 0 }).show(ui, |ui| {
                crate::ui::toolbar(ui, |ui| {
                    let title = match source {
                        Source::Recent => "Recent",
                        Source::ThisMac => "On this Mac and its drives",
                        Source::Account => "In your account",
                    };
                    ui.label(RichText::new(title).size(16.0).strong());
                    if (source == Source::ThisMac && busy) || (source == Source::Account && cloud_busy) {
                        ui.spinner();
                    }
                });
                crate::ui::toolbar(ui, |ui| {
                    crate::ui::text_field(ui, &mut ui_state.query, &format!("{} Search", icon::MAGNIFYING_GLASS), Some(220.0));
                    crate::ui::segmented(ui, &mut ui_state.sort, &[(Sort::Newest, "Newest".into()), (Sort::Name, "Name".into())], |_, _, _| {});
                    if source == Source::ThisMac && !busy && crate::ui::small_command(ui, label(icon::ARROWS_CLOCKWISE, "Look again")).on_hover_text("Search this Mac and its drives again").clicked() {
                        search_again = true;
                    }
                });
                ui.add_space(6.0);
                if source == Source::Account && !signed_in {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        crate::ui::help(ui, "Sign in to see the projects saved in your account.");
                        if crate::ui::command(ui, "Sign in…").clicked() {
                            pick = Some(Pick::SignIn);
                        }
                    });
                    return;
                }
                if shown_cards.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        let empty = match (source, q.is_empty()) {
                            (_, false) => "No project matches.",
                            (Source::Recent, _) => "Nothing opened yet. Projects you open show here.",
                            (Source::ThisMac, _) if busy => "Looking…",
                            (Source::ThisMac, _) => "No projects found on this Mac or its drives.",
                            (Source::Account, _) => "Nothing saved in your account yet (File › Save to Account).",
                        };
                        crate::ui::hint(ui, empty);
                    });
                    return;
                }
                crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
                        for card in &shown_cards {
                            let picture = card.picture.as_deref().and_then(|p| thumbs.get_sized(p, 480));
                            let r = project_card(ui, card, picture);
                            if r.clicked() {
                                pick = Some(clone_pick(&card.open));
                            }
                            if let Some(path) = &card.path {
                                r.right_menu(|ui| {
                                    if crate::ui::menu_item(ui, true, "Show in Finder", "").clicked() {
                                        pick = Some(Pick::Reveal(path.clone()));
                                        ui.close();
                                    }
                                    if source == Source::Recent && crate::ui::menu_item(ui, true, "Remove from Recent", "").clicked() {
                                        pick = Some(Pick::Forget(path.clone()));
                                        ui.close();
                                    }
                                });
                            }
                        }
                    });
                });
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
        let st = &mut self.state;
        st.projects.open = open;
        #[cfg(not(target_arch = "wasm32"))]
        if search_again {
            st.projects.files.search();
        }
        match pick {
            Some(Pick::File(path)) => {
                st.open_project_file(&path);
                st.projects.open = false;
            }
            Some(Pick::Cloud(id)) => {
                st.account.open(ctx, &id);
                st.projects.open = false;
            }
            Some(Pick::Reveal(path)) => {
                #[cfg(not(target_arch = "wasm32"))]
                crate::os::reveal(&path);
                let _ = path;
            }
            Some(Pick::Forget(path)) => st.recent.retain(|p| *p != path),
            Some(Pick::SignIn) => st.account_window = true,
            Some(Pick::New) => {
                self.run_action(ctx, crate::menu::Action::NewProject, ctx.input(|i| i.time));
                self.state.projects.open = false;
            }
            Some(Pick::Templates) => {
                self.state.templates_open = true;
                self.state.projects.open = false;
            }
            Some(Pick::OpenFile) => {
                self.run_action(ctx, crate::menu::Action::Open, ctx.input(|i| i.time));
                self.state.projects.open = false;
            }
            None => {}
        }
    }
}

fn clone_pick(p: &Pick) -> Pick {
    match p {
        Pick::File(s) => Pick::File(s.clone()),
        Pick::Cloud(s) => Pick::Cloud(s.clone()),
        Pick::Reveal(s) => Pick::Reveal(s.clone()),
        Pick::Forget(s) => Pick::Forget(s.clone()),
        Pick::SignIn => Pick::SignIn,
        Pick::New => Pick::New,
        Pick::Templates => Pick::Templates,
        Pick::OpenFile => Pick::OpenFile,
    }
}

impl State {
    /// A small picture of the output, kept for the Projects window's card (on a save you asked for: not autosave).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn save_project_picture(&mut self) {
        // Read back over the next frames without waiting (`render`), saved small on a thread of its own.
        self.project_picture = self.project_path.clone();
        self.projects.pictures_checked = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_folders_and_search() {
        assert_eq!(project_name("/Volumes/WD/shows/Friday Night.phosphor"), "Friday Night");
        assert_eq!(folder_of("/Volumes/WD/shows/a.phosphor"), "/Volumes/WD/shows");
        assert!(matches("night wd", "/Volumes/WD/shows/Friday Night.phosphor"), "words in any order, any case");
        assert!(!matches("saturday", "/Volumes/WD/shows/Friday Night.phosphor"));
        assert!(matches("", "anything"));
    }
}
