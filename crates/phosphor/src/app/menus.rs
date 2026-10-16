//! The top bar and the File/Edit/View/Output/Help menus (in-window and macOS) and their actions.

use super::*;

impl PhosphorApp {
    /// Menu bar clicks (system and in-window) and their shortcuts.
    pub(crate) fn menu_actions(&mut self, ctx: &egui::Context, now: f64) {
        let mut actions = crate::menu::pressed(ctx);
        #[cfg(target_os = "macos")]
        if let Some((_, ids)) = &self.native_menu {
            actions.extend(crate::menu::native::poll(ids));
        }
        actions.extend(ctx.data_mut(|d| d.remove_temp::<Vec<crate::menu::Action>>(egui::Id::new(MENU_REQUEST))).unwrap_or_default());
        for a in actions {
            self.run_action(ctx, a, now);
        }
    }

    pub(crate) fn run_action(&mut self, ctx: &egui::Context, a: crate::menu::Action, now: f64) {
        use crate::menu::Action as A;
        let st = &mut self.state;
        let selected_layer = st.selected.filter(|id| *id != phosphor_engine::MASTER_ID && st.comp.layers.iter().any(|l| l.id == *id));
        match a {
            A::NewProject => {
                st.load_composition(Composition::starter());
                st.project_path = None;
                st.notice = Some("New project. Undo (⌘Z) brings the previous one back.".into());
            }
            A::Open => {
                #[cfg(not(target_arch = "wasm32"))]
                st.dialogs.pick_file(crate::dialogs::Purpose::OpenProject, Some(("Phosphor project", &["phosphor"])));
                #[cfg(target_arch = "wasm32")]
                crate::web_files::pick_text(".phosphor,.json");
            }
            A::Save => st.save_project(),
            A::SaveAs => st.save_project_as(),
            A::SaveToAccount => {
                let name = st.cloud_project.as_ref().map(|(_, n)| n.clone()).unwrap_or_else(|| st.account_ui.name.trim().to_string());
                match st.cloud_project.clone() {
                    Some((id, _)) if st.account.signed_in() => st.account.save(ctx, &id, &name, &st.comp.to_json()),
                    _ => self.state.account_window = true,
                }
            }
            A::OpenFromAccount => self.state.account_window = true,
            A::ImportResolume => {
                #[cfg(not(target_arch = "wasm32"))]
                st.dialogs.pick_file(crate::dialogs::Purpose::ImportResolume, Some(("Resolume composition or VDMX project", &["avc", "vdmx5"])));
            }
            A::FindMissing => {
                #[cfg(not(target_arch = "wasm32"))]
                st.dialogs.pick_folder(crate::dialogs::Purpose::RelinkFolder);
            }
            A::Import => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let exts = crate::media::dialog_extensions();
                    st.dialogs.pick_files(crate::dialogs::Purpose::ImportFiles, Some(("Media", &exts)));
                }
            }
            A::WatchFolder => {
                #[cfg(not(target_arch = "wasm32"))]
                st.dialogs.pick_folder(crate::dialogs::Purpose::WatchFolder);
            }
            A::Snapshot => {
                #[cfg(not(target_arch = "wasm32"))]
                self.save_snapshot();
            }
            A::ExportShow => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    st.export.summary = Some(crate::export::summary(&st.comp));
                }
                let e = &mut st.export;
                e.open = true;
                if e.name.is_empty() {
                    e.name = st
                        .project_path
                        .as_deref()
                        .and_then(|p| std::path::Path::new(p).file_stem())
                        .map_or("My Show".to_string(), |s| s.to_string_lossy().into_owned());
                    e.autostart = true;
                }
            }
            A::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            A::Undo => {
                if let Some(c) = st.history.undo(&st.comp) {
                    st.comp = c;
                    st.comp.ensure_outputs();
                }
            }
            A::Redo => {
                if let Some(c) = st.history.redo(&st.comp) {
                    st.comp = c;
                    st.comp.ensure_outputs();
                }
            }
            A::AddLayer => {
                st.add_layer();
            }
            A::DuplicateLayer => {
                if let Some(id) = selected_layer {
                    panels::duplicate_layer(st, id);
                }
            }
            A::DeleteLayer => {
                if let Some(id) = selected_layer {
                    panels::remove_layer(st, id);
                }
            }
            A::Group => {
                if let Some(id) = selected_layer {
                    panels::group_with_below(&mut st.comp, &mut st.selected, id);
                }
            }
            A::Ungroup => {
                if let Some(g) = panels::selected_group(&st.comp, st.selected) {
                    st.comp.ungroup(g);
                    if st.selected == Some(g) {
                        st.selected = st.comp.layers.last().map(|l| l.id);
                    }
                }
            }
            A::Palette => st.palette.toggle(),
            A::MapMode => {
                st.map_mode = !st.map_mode;
                st.control.learning = None;
            }
            A::ResetLayout => self.dock = default_layout(),
            A::AddMonitor => {
                let _ = self.add_monitor();
            }
            A::Gather => self.gather_windows = true,
            A::ZoomIn => ctx.set_zoom_factor((ctx.zoom_factor() * 1.1).min(3.0)),
            A::ZoomOut => ctx.set_zoom_factor((ctx.zoom_factor() / 1.1).max(0.5)),
            A::ZoomReset => ctx.set_zoom_factor(1.0),
            A::ToggleOutput => st.toggle_output(),
            A::Identify => st.identify_outputs(now),
            A::Syphon => st.comp.syphon = !st.comp.syphon,
            A::Ndi => st.comp.ndi.enabled = !st.comp.ndi.enabled,
            A::Notes => open_doc("NOTES.md"),
            A::Roadmap => open_doc("../ROADMAP.md"),
            A::Website => ctx.open_url(egui::OpenUrl::new_tab("https://phosphorfx.com")),
            A::Donate => ctx.open_url(egui::OpenUrl::new_tab("https://phosphorfx.com/donate/")),
            A::Panel(i) => {
                if let Some(tab) = Tab::ALL.get(i).copied() {
                    self.toggle_panel(tab);
                }
            }
            A::Layout(i) => self.apply_layout(i),
            A::SaveLayout => self.layout_draft = Some((String::new(), self.layouts.startup.is_none() && self.layouts.saved.is_empty())),
        }
    }

    /// Window title: project name (and the show name for exported shows).
    pub(crate) fn update_title(&mut self, ctx: &egui::Context) {
        let project = self.state.project_path.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map(|s| s.to_string_lossy().into_owned());
        #[cfg(not(target_arch = "wasm32"))]
        let show = crate::show::bundled().map(|b| b.manifest.name.clone());
        #[cfg(target_arch = "wasm32")]
        let show: Option<String> = None;
        let title = match (show, project) {
            (Some(s), _) => format!("{s} — Phosphor show"),
            (None, Some(p)) => format!("{p} — Phosphor"),
            (None, None) => "Phosphor".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    pub(crate) fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("PHOSPHOR").strong().size(13.5).color(theme::ACCENT));
                let now = ui.ctx().input(|i| i.time);
                let clicked = self.menu_bar(ui);
                let ctx = ui.ctx().clone();
                for a in clicked {
                    self.run_action(&ctx, a, now);
                }
                ui.separator();
                if ui.button(format!("{} Layer", icon::PLUS)).clicked() {
                    self.state.add_layer();
                }
                let output_label = if self.state.output_open() { format!("{} Output", icon::STOP) } else { format!("{} Output", icon::PLAY) };
                if ui.button(output_label).on_hover_text("Output window. F = fullscreen, Esc = exit").clicked() {
                    self.state.toggle_output();
                }
                ui.menu_button(format!("{} {}", icon::LIGHTNING, launch_summary(&self.state.comp)), |ui| {
                    ui.horizontal(|ui| panels::launch_controls(ui, &mut self.state.comp));
                })
                .response
                .on_hover_text("How clips and media land: Cut/Fade, fade style, Now/On beat/On bar");
                ui.separator();
                // Workspaces: the built-in layouts, one click each (each keeps your changes while you switch).
                for (i, (name, _)) in super::layout::LAYOUTS.iter().enumerate() {
                    let (glyph, text, tip) = match *name {
                        "Default" => (icon::SQUARES_FOUR, "Main", "Everything at once"),
                        "Performance" => (icon::PLAY_CIRCLE, "Perform", "Big preview for playing a show"),
                        "Mapping" => (icon::PROJECTOR_SCREEN, "Studio", "Projection mapping studio"),
                        "VDMX" => (icon::COLUMNS, "VDMX", "Every layer in its own window, media bin underneath"),
                        other => (icon::LAYOUT, other, ""),
                    };
                    if ui.selectable_label(self.workspace == Some(i), format!("{glyph} {text}")).on_hover_text(tip).clicked() {
                        self.switch_workspace(i);
                    }
                }
                ui.menu_button(icon::CARET_DOWN, |ui| {
                    ui.label(egui::RichText::new("Send output to").strong());
                    crate::ui::screen_menu(ui, &mut self.state);
                })
                .response
                .on_hover_text("Choose the screen / projector");
                ui.separator();
                if ui.button(format!("{} Search  ⌘K", icon::MAGNIFYING_GLASS)).on_hover_text("Command palette: effects, sources, panels, actions").clicked() {
                    self.state.palette.toggle();
                }
                let map_text = egui::RichText::new(format!("{} MAP", icon::PLUG)).strong();
                let map_text = if self.state.map_mode { map_text.color(egui::Color32::BLACK) } else { map_text };
                if ui
                    .add(egui::Button::new(map_text).selected(self.state.map_mode).fill(if self.state.map_mode { theme::ACCENT } else { egui::Color32::TRANSPARENT }))
                    .on_hover_text("Map mode (⇧⌘M): click any control, then move a MIDI knob, send OSC or press a key to link them")
                    .clicked()
                {
                    self.state.map_mode = !self.state.map_mode;
                    self.state.control.learning = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    account_button(ui, &mut self.state);
                    let s = self.state.stats.latest;
                    let wide = ui.available_width() > 460.0;
                    if wide {
                        ui.monospace(format!("{:5.1} fps  engine {:4.2} ms  CPU {:4.1}%  {:4.0} MB", s.fps, s.render_ms, s.cpu_percent, s.memory_mb));
                    } else if ui.available_width() > 120.0 {
                        ui.monospace(format!("{:3.0} fps", s.fps));
                    }
                });
            });
            ui.add_space(2.0);
        });
    }

    /// File / Edit / View / Output / Help inside the window (same actions as the macOS menu bar).
    pub(crate) fn menu_bar(&mut self, ui: &mut egui::Ui) -> Vec<crate::menu::Action> {
        use crate::menu::{Action, Entry, MENUS, shortcut_text};
        let mut clicked: Vec<Action> = Vec::new();
        let can_undo = self.state.history.can_undo(&self.state.comp);
        let can_redo = self.state.history.can_redo();
        let has_layer = self.state.selected.is_some_and(|id| id != phosphor_engine::MASTER_ID && self.state.comp.layers.iter().any(|l| l.id == id));
        for (title, entries) in MENUS {
            ui.menu_button(*title, |ui| {
                ui.set_min_width(230.0);
                for e in entries.iter() {
                    match e {
                        Entry::Sep => {
                            ui.separator();
                        }
                        Entry::Item(a, label, sc) => {
                            let enabled = match a {
                                Action::Undo => can_undo,
                                Action::Redo => can_redo,
                                Action::DuplicateLayer | Action::DeleteLayer => has_layer,
                                Action::Group => has_layer && self.state.selected.and_then(|id| self.state.comp.layers.iter().find(|l| l.id == id)).is_some_and(|l| l.parent.is_none()),
                                Action::Ungroup => panels::selected_group(&self.state.comp, self.state.selected).is_some(),
                                Action::Syphon | Action::Ndi => cfg!(target_os = "macos"),
                                _ => true,
                            };
                            let text = match a {
                                Action::Syphon => format!("{label}  ({})", if self.state.comp.syphon { "on" } else { "off" }),
                                Action::Ndi => format!("{label}  ({})", if self.state.comp.ndi.enabled { "on" } else { "off" }),
                                _ => label.to_string(),
                            };
                            if ui.add_enabled(enabled, egui::Button::new(text).shortcut_text(shortcut_text(*sc))).clicked() {
                                clicked.push(*a);
                                ui.close();
                            }
                        }
                        Entry::Dynamic("Open Recent") => {
                            ui.add_enabled_ui(!self.state.recent.is_empty(), |ui| {
                                ui.menu_button("Open Recent", |ui| {
                                    for p in self.state.recent.clone() {
                                        let name = std::path::Path::new(&p).file_name().map_or(p.clone(), |n| n.to_string_lossy().into_owned());
                                        if ui.button(name).on_hover_text(&p).clicked() {
                                            self.state.open_project_file(&p);
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    if ui.button("Clear Menu").clicked() {
                                        self.state.recent.clear();
                                        ui.close();
                                    }
                                });
                            });
                        }
                        Entry::Dynamic("Panels") => {
                            ui.menu_button("Panels", |ui| {
                                crate::ui::hint(ui, "Drag any tab to move it; drag it outside to float it.");
                                for (i, tab) in Tab::ALL.iter().enumerate() {
                                    let open = self.detached.contains(tab) || self.dock.find_tab(tab).is_some();
                                    if ui.selectable_label(open, tab.title()).clicked() {
                                        clicked.push(Action::Panel(i));
                                    }
                                }
                            });
                        }
                        Entry::Dynamic("Layouts") => {
                            ui.menu_button("Layouts", |ui| {
                                let names = self.layouts.names();
                                let startup = self.layouts.startup.clone();
                                for (i, name) in names.iter().enumerate() {
                                    if i == LAYOUTS.len() && i < names.len() {
                                        ui.separator();
                                    }
                                    let text = if startup.as_deref() == Some(name.as_str()) { format!("{name}  (startup)") } else { name.clone() };
                                    if ui.button(text).clicked() {
                                        clicked.push(Action::Layout(i));
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if ui.button(format!("{} Save Current Layout…", icon::FLOPPY_DISK)).clicked() {
                                    clicked.push(Action::SaveLayout);
                                    ui.close();
                                }
                                ui.menu_button("Open at Startup", |ui| {
                                    if ui.selectable_label(startup.is_none(), "However I left it").clicked() {
                                        self.layouts.startup = None;
                                        ui.close();
                                    }
                                    for name in &names {
                                        if ui.selectable_label(startup.as_deref() == Some(name.as_str()), name).clicked() {
                                            self.layouts.startup = Some(name.clone());
                                            ui.close();
                                        }
                                    }
                                });
                                if !self.layouts.saved.is_empty() {
                                    ui.menu_button("Delete", |ui| {
                                        for name in self.layouts.saved.iter().map(|l| l.name.clone()).collect::<Vec<_>>() {
                                            if ui.button(&name).clicked() {
                                                self.layouts.remove(&name);
                                                self.install_native_menu();
                                                ui.close();
                                            }
                                        }
                                    });
                                }
                            });
                        }
                        Entry::Dynamic("Send Output To") => {
                            ui.menu_button("Send Output To", |ui| crate::ui::screen_menu(ui, &mut self.state));
                        }
                        Entry::Dynamic(_) => {}
                    }
                }
            });
        }
        clicked
    }
}

/// Short label for the launch settings, e.g. "Fade 0.5 s · On beat".
pub(crate) fn launch_summary(comp: &Composition) -> String {
    let how = match (comp.launch.fade, comp.launch.style) {
        (false, _) => "Cut".to_string(),
        (true, phosphor_engine::TransitionKind::Crossfade) => format!("Fade {:.1}s", comp.launch.fade_secs),
        (true, style) => format!("{} {:.1}s", style.name(), comp.launch.fade_secs),
    };
    format!("{how} · {}", comp.launch.timing.name())
}

pub(crate) fn label_text(icon: &str, text: &str) -> String {
    format!("{icon}  {text}")
}

/// Opens one of the docs (notes, roadmap) from the source folder, or the website if not there.
pub(crate) fn open_doc(name: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(name);
        let target = if path.is_file() { path.to_string_lossy().into_owned() } else { "https://phosphorfx.com".into() };
        let _ = std::process::Command::new("open").arg(target).spawn();
    }
    #[cfg(target_arch = "wasm32")]
    let _ = name;
}

/// Top bar: who's signed in (or "Sign in"); opens the Account window.
fn account_button(ui: &mut egui::Ui, st: &mut State) {
    let text = match &st.account.user {
        Some(u) => {
            let name = u.email.split('@').next().unwrap_or(&u.email);
            let short: String = name.chars().take(14).collect();
            format!("{} {short}", icon::USER_CIRCLE)
        }
        None => format!("{} Sign in", icon::USER_CIRCLE),
    };
    let hover = if st.account.user.is_some() { "Your phosphorfx.com account: projects and layouts" } else { "Optional: a free phosphorfx.com account keeps your projects and layouts on every computer" };
    if ui.selectable_label(st.account_window, text).on_hover_text(hover).clicked() {
        st.account_window = !st.account_window;
    }
}

impl PhosphorApp {
    /// The Account window (top bar button, File › Save to / Open from Account), and the one-time hello that
    /// mentions the free account at startup.
    pub(crate) fn account_windows(&mut self, ctx: &egui::Context) {
        let st = &mut self.state;
        if st.account_window {
            let mut open = true;
            egui::Window::new(format!("{} Account", icon::USER_CIRCLE)).open(&mut open).collapsible(false).resizable(true).default_width(440.0).default_height(420.0).show(ctx, |ui| {
                crate::panels::account(ui, st);
            });
            st.account_window = open;
        }
        if st.account_hello && st.account.checked && st.account.user.is_none() && !st.account_window {
            let mut close = false;
            egui::Window::new("Welcome to Phosphor").collapsible(false).resizable(false).anchor(egui::Align2::RIGHT_TOP, [-12.0, 44.0]).show(ctx, |ui| {
                ui.set_max_width(320.0);
                ui.label("Phosphor is free. A free phosphorfx.com account keeps your projects and layouts on every computer you use. It's optional.");
                ui.horizontal(|ui| {
                    if ui.button(format!("{} Sign in or create one", icon::USER_CIRCLE)).clicked() {
                        st.account_window = true;
                        close = true;
                    }
                    if ui.button("Not now").clicked() {
                        close = true;
                    }
                });
                ui.checkbox(&mut st.account_hello_never, "Don't show this again");
            });
            if close {
                st.account_hello = false;
            }
        }
    }
}
