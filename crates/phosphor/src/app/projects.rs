//! File › Projects…: every saved project in one window (recent ones, the ones on this Mac and its drives, the ones in
//! your account), opened with one click. Also shown at start unless turned off.

use super::*;
use crate::ui::{RightMenu, label};
use egui::{Color32, RichText};
use egui_phosphor::regular as icon;

#[derive(Default)]
pub(crate) struct ProjectsUi {
    pub open: bool,
    pub detached: bool,
    /// Show the window when Phosphor starts.
    pub at_start: bool,
    pub query: String,
    #[cfg(not(target_arch = "wasm32"))]
    pub files: crate::project_files::ProjectFiles,
    /// The account's list was asked for (once per opening).
    pub cloud_asked: bool,
}

/// What a click in the window asked for.
enum Pick {
    File(String),
    Cloud(String),
    Reveal(String),
    Forget(String),
    SignIn,
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

/// One project in the list: Open, name (the current one in the accent colour), where / when. Greyed out when the
/// file isn't there (a drive not plugged in). Returns (open clicked, row response for its right-click menu).
fn project_row(ui: &mut egui::Ui, name: &str, detail: &str, current: bool, there: bool) -> (bool, egui::Response) {
    let mut open = false;
    let mut name_r = None;
    crate::ui::toolbar(ui, |ui| {
        if crate::ui::command_if(ui, there, label(icon::FOLDER_OPEN, "Open")).on_disabled_hover_text("Not found: is its drive plugged in?").clicked() {
            open = true;
        }
        let text = if current { RichText::new(name).color(crate::theme::accent()).strong() } else if there { RichText::new(name).strong() } else { RichText::new(name).color(Color32::GRAY) };
        name_r = Some(ui.label(text));
        crate::ui::hint(ui, detail);
    });
    (open, name_r.expect("drawn"))
}

impl PhosphorApp {
    pub(crate) fn projects_window(&mut self, ctx: &egui::Context) {
        if !self.state.projects.open {
            self.state.projects.cloud_asked = false;
            return;
        }
        let st = &mut self.state;
        #[cfg(not(target_arch = "wasm32"))]
        st.projects.files.poll();
        if st.account.signed_in() && !st.projects.cloud_asked {
            st.projects.cloud_asked = true;
            st.account.refresh(ctx);
        }
        let mut pick: Option<Pick> = None;
        let mut search_again = false;
        let current = st.project_path.clone();
        let cloud_current = st.cloud_project.as_ref().map(|(id, _)| id.clone());
        let recent = st.recent.clone();
        #[cfg(not(target_arch = "wasm32"))]
        let (found, busy) = (st.projects.files.found.clone(), st.projects.files.busy());
        let (cloud, signed_in, cloud_busy) = (st.account.projects.clone(), st.account.signed_in(), st.account.busy > 0);
        let ui_state = &mut st.projects;
        let (mut open, mut detached) = (ui_state.open, ui_state.detached);
        let (query, at_start) = (&mut ui_state.query, &mut ui_state.at_start);
        crate::ui::tool_window(ctx, "Projects", &mut open, &mut detached, 560.0, |ui| {
            crate::ui::toolbar(ui, |ui| {
                crate::ui::text_field(ui, query, "Search projects", Some(220.0));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if busy {
                        ui.spinner();
                    } else if crate::ui::small_command(ui, label(icon::ARROWS_CLOCKWISE, "Look again")).on_hover_text("Search this Mac and its drives again").clicked() {
                        search_again = true;
                    }
                }
                crate::ui::toggle(ui, at_start, "Show at start").on_hover_text("Open this window when Phosphor starts");
            });
            let q = query.trim().to_string();
            let row_menu = |r: &egui::Response, path: &str, in_recent: bool, pick: &mut Option<Pick>| {
                r.right_menu(|ui| {
                    if crate::ui::menu_item(ui, true, "Show in Finder", "").clicked() {
                        *pick = Some(Pick::Reveal(path.to_string()));
                        ui.close();
                    }
                    if in_recent && crate::ui::menu_item(ui, true, "Remove from Recent", "").clicked() {
                        *pick = Some(Pick::Forget(path.to_string()));
                        ui.close();
                    }
                });
            };

            crate::ui::heading(ui, format!("{} Recent", icon::CLOCK_COUNTER_CLOCKWISE));
            let recent_shown: Vec<&String> = recent.iter().filter(|p| matches(&q, p)).collect();
            if recent_shown.is_empty() {
                crate::ui::hint(ui, if q.is_empty() { "Nothing opened yet." } else { "No match." });
            }
            for p in recent_shown {
                #[cfg(not(target_arch = "wasm32"))]
                let there = crate::media::present::file_present(p).unwrap_or(true);
                #[cfg(target_arch = "wasm32")]
                let there = true;
                let detail = if there { folder_of(p) } else { format!("{} · not found", folder_of(p)) };
                let (open_it, r) = project_row(ui, &project_name(p), &detail, current.as_deref() == Some(p.as_str()), there);
                if open_it {
                    pick = Some(Pick::File(p.clone()));
                }
                row_menu(&r, p, true, &mut pick);
            }

            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.add_space(6.0);
                crate::ui::heading(ui, format!("{} On this Mac and its drives", icon::HARD_DRIVES));
                let shown: Vec<&crate::project_files::Found> = found.iter().filter(|f| !recent.contains(&f.path) && matches(&q, &f.path)).collect();
                if shown.is_empty() {
                    crate::ui::hint(ui, if busy { "Looking…" } else if q.is_empty() { "No other projects found." } else { "No match." });
                }
                for f in shown {
                    let detail = format!("{} · {} · {:.0} KB", folder_of(&f.path), crate::ui::ago_secs(f.modified), f.size as f64 / 1024.0);
                    let (open_it, r) = project_row(ui, &project_name(&f.path), &detail, current.as_deref() == Some(f.path.as_str()), true);
                    if open_it {
                        pick = Some(Pick::File(f.path.clone()));
                    }
                    row_menu(&r, &f.path, false, &mut pick);
                }
            }

            ui.add_space(6.0);
            crate::ui::toolbar(ui, |ui| {
                crate::ui::heading(ui, format!("{} In your account", icon::CLOUD));
                if cloud_busy {
                    ui.spinner();
                }
            });
            if !signed_in {
                crate::ui::toolbar(ui, |ui| {
                    crate::ui::hint(ui, "Sign in to see the projects saved in your account.");
                    if crate::ui::small_command(ui, "Sign in…").clicked() {
                        pick = Some(Pick::SignIn);
                    }
                });
            } else {
                let shown: Vec<_> = cloud.iter().filter(|p| matches(&q, &p.name)).collect();
                if shown.is_empty() {
                    crate::ui::hint(ui, if q.is_empty() { "Nothing saved in your account yet (File › Save to Account)." } else { "No match." });
                }
                for p in shown {
                    let detail = format!("{} · {:.0} KB", crate::ui::ago_secs(p.updated), p.size as f64 / 1024.0);
                    if project_row(ui, &p.name, &detail, cloud_current.as_deref() == Some(p.id.as_str()), true).0 {
                        pick = Some(Pick::Cloud(p.id.clone()));
                    }
                }
            }
            ui.add_space(4.0);
            crate::ui::hint(ui, "Opening one replaces the project you have; Undo (⌘Z) brings it back.");
        });
        let st = &mut self.state;
        (st.projects.open, st.projects.detached) = (open, detached);
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
            None => {}
        }
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
