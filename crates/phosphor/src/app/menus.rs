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
        log::info!("menu: {a:?}");
        use crate::menu::Action as A;
        if self.lock.active && !super::lock::allowed_when_locked(&a) {
            return;
        }
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
            A::Lock => self.lock.window = true,
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
            A::Quick(q) => st.quick(q, now),
            A::MapMode => {
                st.map_mode = !st.map_mode;
                st.control.learning = None;
            }
            A::ResetLayout => self.reset_area_layout(),
            A::NewFromTemplate => st.templates_open = true,
            A::MergeProject => {
                #[cfg(not(target_arch = "wasm32"))]
                self.state.dialogs.pick_file(crate::dialogs::Purpose::MergeProject, Some(("Phosphor project", &["phosphor"])));
            }
            A::LockPanels => {
                self.panels_locked = !self.panels_locked;
                self.state.notice = Some(if self.panels_locked { "Panels locked: they stay where they are (View › Lock Panels to move them again)".into() } else { "Panels unlocked".into() });
            }
            A::AddMonitor => {
                let _ = self.add_monitor();
            }
            A::Gather => self.gather_windows = true,
            A::ZoomIn => ctx.set_zoom_factor((ctx.zoom_factor() * 1.1).min(3.0)),
            A::ZoomOut => ctx.set_zoom_factor((ctx.zoom_factor() / 1.1).max(0.5)),
            A::ZoomReset => ctx.set_zoom_factor(1.0),
            A::ToggleOutput => st.toggle_output(),
            A::Identify => st.identify_outputs(now),
            A::Syphon => *st.comp.share_out_mut() = !st.comp.share_out(),
            A::Ndi => st.comp.ndi.enabled = !st.comp.ndi.enabled,
            A::MuteAll => st.comp.sound_muted = !st.comp.sound_muted,
            A::WebControl => {
                let on = st.comp.web_control_off;
                st.set_web_control(on);
            }
            // What changed in each version (the release notes come from CHANGELOG.md).
            A::Notes => crate::os::open("https://github.com/fillinthe/phosphor/releases".to_string()),
            A::Roadmap => open_doc("../ROADMAP.md"),
            A::Website => ctx.open_url(egui::OpenUrl::new_tab("https://phosphorfx.com")),
            A::Donate => ctx.open_url(egui::OpenUrl::new_tab(crate::support::DONATE_URL)),
            A::ShaderFolder => {
                // ~/Library/Application Support/Phosphor/isf: scanned at startup like VDMX's ISF folders.
                if let Some(dir) = crate::show::storage_dir().map(|d| d.join("isf")) {
                    let _ = std::fs::create_dir_all(&dir);
                    #[cfg(target_os = "macos")]
                    crate::os::open(&dir);
                    st.notice = Some(format!("ISF shaders you drop into {} appear in the browsers at the next launch", dir.display()));
                }
            }
            A::Autosave => {
                st.autosave = !st.autosave;
                st.notice = Some(if st.autosave {
                    format!("Autosave on: the project saves itself every {} minutes when it has changed (after you've saved it once)", crate::app::AUTOSAVE_MINUTES)
                } else {
                    "Autosave off".into()
                });
            }
            A::ShowBackups => {
                #[cfg(not(target_arch = "wasm32"))]
                match &st.project_path {
                    Some(p) if crate::backups::folder(std::path::Path::new(p)).exists() => crate::os::open(crate::backups::folder(std::path::Path::new(p))),
                    Some(_) => st.notice = Some("No backups yet: they're made each time you save over the project".into()),
                    None => st.notice = Some("Save the project first: backups go in a folder next to it".into()),
                }
            }
            A::LutFolder => {
                // ~/Library/Application Support/Phosphor/luts: each .cube becomes an effect (category LUT).
                if let Some(dir) = crate::show::storage_dir().map(|d| d.join("luts")) {
                    let _ = std::fs::create_dir_all(&dir);
                    crate::os::open(&dir);
                    st.notice = Some("Colour-grading LUTs (.cube) you drop in this folder become effects (category LUT) at the next launch".into());
                }
            }
            A::ReportBug => st.bug.open = true,
            A::CheckUpdates => {
                st.updater.window = true;
                st.updater.check(ctx);
            }
            A::Panel(i) => {
                if let Some(tab) = Tab::ALL.get(i).copied() {
                    self.toggle_panel(tab);
                }
            }
            A::Layout(i) => self.apply_layout(i),
            A::Area(i) => self.switch_workspace(i),
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
            crate::ui::bar_row(ui, |ui| {
                ui.label(egui::RichText::new("PHOSPHOR").strong().size(13.5).color(theme::accent()));
                let now = ui.ctx().input(|i| i.time);
                // On the Mac the menus live in the macOS menu bar; the in-window copy is for the browser build.
                #[cfg(target_os = "macos")]
                let in_window = self.native_menu.is_none();
                #[cfg(not(target_os = "macos"))]
                let in_window = true;
                if in_window {
                    let clicked = self.menu_bar(ui);
                    let ctx = ui.ctx().clone();
                    for a in clicked {
                        self.run_action(&ctx, a, now);
                    }
                }
                ui.separator();
                // The areas first: where you are. Each is a free arrangement of panels that it remembers. (Adding
                // layers, launch settings and sending outputs to screens live in the areas: Layers, the Clips toolbar,
                // the Outputs strip.)
                for (i, (name, _)) in super::layout::LAYOUTS.iter().enumerate() {
                    let (glyph, text, tip) = match *name {
                        "Show" => (icon::PLAY_CIRCLE, "Show", "Build and play: preview, layers, clips, media, effects, mixer, cues (⌘1)"),
                        "Outputs" => (icon::PROJECTOR_SCREEN, "Outputs", "Outputs and inputs: every output, mapping, screens, streaming, session, LEDs, audio in (⌘2)"),
                        "Control" => (icon::SLIDERS_HORIZONTAL, "Control", "Controllers: MIDI / OSC, TouchOSC, Control Surface, modulators (⌘3)"),
                        other => (icon::LAYOUT, other, ""),
                    };
                    let tip = format!("{tip}. Drag any panel to rearrange this area; it remembers. View > Reset Layout restores it.");
                    if crate::ui::pick_item(ui, self.workspace == Some(i), format!("{glyph} {text}")).on_hover_text(tip).clicked() {
                        self.switch_workspace(i);
                    }
                }
                ui.separator();
                // Short on room (narrow window, in-window menus): the next buttons become icons, names on hover.
                let compact = ui.available_width() < 560.0;
                let named = |glyph: &str, name: &str| if compact { glyph.to_string() } else { format!("{glyph} {name}") };
                // One-click visuals: the dice = Surprise me; the arrow = every Quick action.
                // The dice says where it will land: the selected layer, else every layer.
                let target = self.state.selected.filter(|id| self.state.comp.layers.iter().any(|l| l.id == *id));
                let where_ = target.map_or("all layers".to_string(), |id| crate::ui::layer_name(&self.state.comp.layers, id));
                // One width whatever's selected (a long layer name is cut short; the whole name is in the tooltip).
                let short: String = if where_.chars().count() > 10 { format!("{}…", where_.chars().take(9).collect::<String>()) } else { where_.clone() };
                let (text, widest) = (named(icon::DICE_FIVE, &format!("Surprise · {short}")), named(icon::DICE_FIVE, "Surprise · all layers"));
                let dice = crate::ui::steady_action_button(ui, crate::ui::Act::Go, text.clone(), &[&text, &widest]).on_hover_text(format!(
                    "{} (⌘R: all layers). Click: a new look on {where_}. The arrow: pick several layers. Right-click to map a pad or key.",
                    super::Quick::Surprise.about()
                ));
                crate::ui::map_menu(ui, &dice, phosphor_engine::ControlTarget::App(phosphor_engine::AppControl::Surprise));
                if dice.clicked() {
                    match target {
                        Some(id) => self.state.quick_on(super::Quick::SurpriseLayer, Some(id), now),
                        None => self.state.quick(super::Quick::Surprise, now),
                    }
                }
                let mut picked = None;
                let mut surprise: Vec<u64> = Vec::new();
                let key = egui::Id::new("surprise_picks");
                ui.menu_button(icon::CARET_DOWN, |ui| {
                    ui.label(egui::RichText::new("Surprise which layers").strong());
                    let mut picks: Vec<u64> = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_default();
                    for l in self.state.comp.layers.iter().rev() {
                        let mut on = picks.contains(&l.id);
                        if crate::ui::toggle(ui, &mut on, &l.name).changed() {
                            if on { picks.push(l.id) } else { picks.retain(|x| *x != l.id) }
                        }
                    }
                    picks.retain(|id| self.state.comp.layers.iter().any(|l| l.id == *id));
                    ui.horizontal(|ui| {
                        if crate::ui::command_if(ui, !picks.is_empty(), format!("{} Surprise these", icon::DICE_FIVE)).clicked() {
                            surprise = picks.clone();
                            ui.close();
                        }
                        if crate::ui::command(ui, "All layers").clicked() {
                            surprise = self.state.comp.layers.iter().map(|l| l.id).collect();
                            ui.close();
                        }
                    });
                    ui.ctx().data_mut(|d| d.insert_temp(key, picks));
                    ui.separator();
                    picked = super::quick_menu(ui);
                })
                .response
                .on_hover_text("Surprise several layers at once, or any Quick action: shuffle, react to the music, fill clips, instant set");
                for id in surprise {
                    self.state.quick_on(super::Quick::SurpriseLayer, Some(id), now);
                }
                if let Some(q) = picked {
                    self.state.quick(q, now);
                }
                let (stop, play) = (named(icon::STOP, "Output"), named(icon::PLAY, "Output"));
                let output_label = if self.state.output_open() { stop.clone() } else { play.clone() };
                if crate::ui::steady_button(ui, output_label, &[&stop, &play]).on_hover_text("Output windows on / off. F = fullscreen, Esc = exit").clicked() {
                    self.state.toggle_output();
                }
                ui.separator();
                if crate::ui::command(ui, named(icon::MAGNIFYING_GLASS, "Search  ⌘K")).on_hover_text("Command palette: effects, sources, panels, actions").clicked() {
                    self.state.palette.toggle();
                }
                if crate::ui::mode_button(ui, self.state.map_mode, format!("{} MAP", icon::PLUG))
                    .on_hover_text("Map mode (Shift ⌘M): click any control, then move a MIDI knob, send OSC or press a key to link them")
                    .clicked()
                {
                    self.state.map_mode = !self.state.map_mode;
                    self.state.control.learning = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    account_button(ui, &mut self.state);
                    crate::support::donate_button(ui);
                    // Only while a new version is waiting: right next to Donate, filled so it stands out.
                    self.state.updater.top_bar_button(ui);
                    let before = self.ui_brightness;
                    ui.menu_button(icon::SUN, |ui| {
                        ui.label(egui::RichText::new("Interface brightness").strong());
                        crate::ui::Level::percent(&mut self.ui_brightness, "Brightness").default(theme::DEFAULT_BRIGHTNESS).width(220.0).show(ui);
                        ui.horizontal(|ui| {
                            for (name, b) in theme::BRIGHTNESS_PRESETS {
                                if crate::ui::pick_item(ui, (self.ui_brightness - b).abs() < 0.01, *name).clicked() {
                                    self.ui_brightness = *b;
                                }
                            }
                        });
                        crate::ui::hint(ui, "Darker for dark rooms and shows, lighter for daylight.");
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Accent").small());
                            for (k, (name, dark, _)) in theme::ACCENTS.iter().enumerate() {
                                let (r, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
                                ui.painter().circle_filled(r.center(), 7.0, egui::Color32::from_rgb(dark[0], dark[1], dark[2]));
                                if theme::accent_index() == k {
                                    ui.painter().circle_stroke(r.center(), 8.5, egui::Stroke::new(1.5, ui.visuals().strong_text_color()));
                                }
                                if resp.on_hover_text(*name).clicked() {
                                    theme::set_accent(k);
                                    theme::set_brightness(ui.ctx(), self.ui_brightness);
                                }
                            }
                        });
                    })
                    .response
                    .on_hover_text("Interface brightness: from darkest to light");
                    if self.ui_brightness != before {
                        theme::set_brightness(ui.ctx(), self.ui_brightness);
                    }
                    if crate::ui::command(ui, icon::BUG).on_hover_text("Report a bug").clicked() {
                        self.state.bug.open = true;
                    }
                    // Frame rate and CPU live in the status line (bottom right), where there's room.
                });
            });
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
                                Action::Syphon | Action::Ndi => cfg!(any(target_os = "macos", windows)),
                                _ => true,
                            };
                            let text = match a {
                                Action::Syphon => format!("{label}  ({})", if self.state.comp.share_out() { "on" } else { "off" }),
                                Action::Ndi => format!("{label}  ({})", if self.state.comp.ndi.enabled { "on" } else { "off" }),
                                _ => label.to_string(),
                            };
                            if crate::ui::menu_item(ui, enabled, text, shortcut_text(*sc)).clicked() {
                                clicked.push(*a);
                                ui.close();
                            }
                        }
                        Entry::Dynamic("Open Recent") => {
                            ui.add_enabled_ui(!self.state.recent.is_empty(), |ui| {
                                ui.menu_button("Open Recent", |ui| {
                                    for p in self.state.recent.clone() {
                                        let name = std::path::Path::new(&p).file_name().map_or(p.clone(), |n| n.to_string_lossy().into_owned());
                                        if crate::ui::command(ui, name).on_hover_text(&p).clicked() {
                                            self.state.open_project_file(&p);
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    if crate::ui::command(ui, "Clear Menu").clicked() {
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
                                    if crate::ui::pick_item(ui, open, tab.title()).clicked() {
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
                                    if crate::ui::command(ui, text).clicked() {
                                        clicked.push(Action::Layout(i));
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if crate::ui::command(ui, format!("{} Save Current Layout…", icon::FLOPPY_DISK)).clicked() {
                                    clicked.push(Action::SaveLayout);
                                    ui.close();
                                }
                                ui.menu_button("Open at Startup", |ui| {
                                    if crate::ui::pick_item(ui, startup.is_none(), "However I left it").clicked() {
                                        self.layouts.startup = None;
                                        ui.close();
                                    }
                                    for name in &names {
                                        if crate::ui::pick_item(ui, startup.as_deref() == Some(name.as_str()), name).clicked() {
                                            self.layouts.startup = Some(name.clone());
                                            ui.close();
                                        }
                                    }
                                });
                                if !self.layouts.saved.is_empty() {
                                    ui.menu_button("Delete", |ui| {
                                        for name in self.layouts.saved.iter().map(|l| l.name.clone()).collect::<Vec<_>>() {
                                            if crate::ui::command(ui, &name).clicked() {
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
        crate::os::open(target);
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
    if crate::ui::pick_item(ui, st.account_window, text).on_hover_text(hover).clicked() {
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
                    if crate::ui::command(ui, format!("{} Sign in or create one", icon::USER_CIRCLE)).clicked() {
                        st.account_window = true;
                        close = true;
                    }
                    if crate::ui::command(ui, "Not now").clicked() {
                        close = true;
                    }
                });
                crate::ui::toggle(ui, &mut st.account_hello_never, "Don't show this again");
            });
            if close {
                st.account_hello = false;
            }
        }
    }
}
