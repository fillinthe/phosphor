//! The top bar and the File/Edit/View/Output/Help menus (in-window and macOS) and their actions.

use super::*;
use crate::ui::RightMenu;

impl PhosphorApp {
    /// Menu bar clicks (system and in-window) and their shortcuts.
    pub(crate) fn menu_actions(&mut self, ctx: &egui::Context, now: f64) {
        let mut actions = crate::menu::pressed(ctx);
        #[cfg(target_os = "macos")]
        if let Some((_, ids)) = &self.native_menu {
            actions.extend(crate::menu::native::poll(ids));
        }
        actions.extend(ctx.data_mut(|d| d.remove_temp::<Vec<crate::menu::Action>>(egui::Id::new(MENU_REQUEST))).unwrap_or_default());
        actions.append(&mut self.state.menu_requests);
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
            // (Asks first when the project has unsaved edits or is on the outputs: Undo can't bring it back.)
            A::NewProject => self.guard(ctx, super::guard::Guarded::NewProject),
            A::Open => {
                #[cfg(not(target_arch = "wasm32"))]
                st.dialogs.pick_file(crate::dialogs::Purpose::OpenProject, Some(("Phosphor project or packed show", &["phosphor", crate::pack::EXT])));
                #[cfg(target_arch = "wasm32")]
                crate::web_files::pick_text(".phosphor,.json");
            }
            // The browser keeps projects in the account (Download a Copy puts one on the computer); the installed app
            // keeps them in files, and a project that also has a copy in the account keeps that copy up to date.
            A::Save if crate::account::Account::web() => {
                if !st.save_to_account(ctx) {
                    st.account_window = true;
                }
            }
            A::Save => {
                st.save_project();
                #[cfg(not(target_arch = "wasm32"))]
                st.save_project_picture();
                if st.project_path.is_some() && st.cloud_project.is_some() {
                    st.save_to_account(ctx);
                }
            }
            A::SaveAs => st.save_project_as(),
            A::SaveToAccount => {
                if !st.save_to_account(ctx) {
                    st.account_window = true;
                }
            }
            A::Account | A::OpenFromAccount => self.state.account_window = true,
            A::Projects => self.state.projects.open = true,
            A::ImportResolume => {
                #[cfg(not(target_arch = "wasm32"))]
                st.dialogs.pick_file(crate::dialogs::Purpose::ImportResolume, Some(("Resolume, VDMX, MadMapper or Morpholio Trace", &["avc", "vdmx5", "mad", "morpholiotrace"])));
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
            A::SavePacked => {
                #[cfg(not(target_arch = "wasm32"))]
                crate::panels::ask_save_pack(st);
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
            A::Undo => st.undo_step(-1),
            A::Redo => st.undo_step(1),
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
            A::NewPanel => {
                let page = self.state.comp.surface.add_page();
                if let Some(name) = self.state.comp.surface.pages.get_mut(page) {
                    *name = format!("Panel {}", page + 1);
                }
                self.open_my_panel(page);
            }
            A::PanelBrowser => self.state.panel_browser = !self.state.panel_browser,
            A::ZoomIn => ctx.set_zoom_factor((ctx.zoom_factor() * 1.1).min(3.0)),
            A::ZoomOut => ctx.set_zoom_factor((ctx.zoom_factor() / 1.1).max(0.5)),
            A::ZoomReset => ctx.set_zoom_factor(1.0),
            // Closing the outputs while they show asks first (`guard`); opening them is at once.
            A::ToggleOutput if st.output_open() => self.guard(ctx, super::guard::Guarded::OutputsOff),
            A::ToggleOutput => st.toggle_output(),
            A::Blackout => {
                let target = phosphor_engine::ControlTarget::App(phosphor_engine::AppControl::Blackout);
                if !st.comp.control_locked(&target) {
                    st.comp.blackout_on = !st.comp.blackout_on;
                }
            }
            A::Identify => st.identify_outputs(now),
            A::Syphon => *st.comp.share_out_mut() = !st.comp.share_out(),
            A::Ndi => st.comp.ndi.enabled = !st.comp.ndi.enabled,
            A::MuteAll => st.comp.sound_muted = !st.comp.sound_muted,
            A::WebControl => {
                let on = st.comp.web_control_off;
                st.set_web_control(on);
            }
            // What changed in each version (the release notes come from CHANGELOG.md).
            A::Notes => ctx.open_url(egui::OpenUrl::new_tab("https://github.com/fillinthe/phosphor/releases")),
            A::Roadmap => open_doc("../ROADMAP.md"),
            A::Website => ctx.open_url(egui::OpenUrl::new_tab("https://phosphorfx.com")),
            A::Manual => ctx.open_url(egui::OpenUrl::new_tab(crate::support::manual_url())),
            A::Donate => ctx.open_url(egui::OpenUrl::new_tab(crate::support::DONATE_URL)),
            #[cfg(target_arch = "wasm32")]
            A::ShaderFolder | A::LutFolder => {}
            #[cfg(not(target_arch = "wasm32"))]
            A::ShaderFolder => {
                // The settings folder's isf (Mac: ~/Library/Application Support/Phosphor/isf; Windows: %APPDATA%\Phosphor\data\isf),
                // scanned at startup like VDMX's ISF folders and watched for new files after.
                if let Some(dir) = crate::show::storage_dir().map(|d| d.join("isf")) {
                    let _ = std::fs::create_dir_all(&dir);
                    crate::os::open(&dir);
                    st.notice = Some(format!("ISF shaders you drop into {} appear in the browsers within a few seconds", dir.display()));
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
            #[cfg(not(target_arch = "wasm32"))]
            A::LutFolder => {
                // ~/Library/Application Support/Phosphor/luts: each .cube becomes an effect (category LUT).
                if let Some(dir) = crate::show::storage_dir().map(|d| d.join("luts")) {
                    let _ = std::fs::create_dir_all(&dir);
                    crate::os::open(&dir);
                    st.notice = Some("Color-grading LUTs (.cube) you drop in this folder become effects (category LUT) within a few seconds".into());
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
        // Edits not saved yet: "— Edited" after the name, as Mac documents say it (settled edits only, so a drag
        // doesn't make the title flicker).
        let edited = self.state.history.generation() != self.state.saved_generation;
        let title = match (show, project, edited) {
            (Some(s), _, _) => format!("{s} — Phosphor VJ show"),
            (None, Some(p), true) => format!("{p} — Edited — Phosphor VJ"),
            (None, Some(p), false) => format!("{p} — Phosphor VJ"),
            (None, None, true) => "Untitled — Edited — Phosphor VJ".into(),
            (None, None, false) => "Phosphor VJ".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    pub(crate) fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar").show(ui, |ui| {
            crate::ui::bar_row(ui, |ui| {
                ui.label(theme::logo(13.5));
                let now = ui.ctx().input(|i| i.time);
                // On the Mac the menus live in the macOS menu bar; the in-window copy is for the browser build.
                #[cfg(target_os = "macos")]
                let in_window = self.native_menu.is_none();
                #[cfg(not(target_os = "macos"))]
                let in_window = true;
                // Out of room even with icon buttons (level 2), or a small window: the menus fold into one ☰ button.
                let squeeze: u8 = ui.ctx().data(|d| d.get_temp(egui::Id::new("top_bar_squeezed"))).unwrap_or(0);
                let folded = squeeze >= 2 || ui.ctx().content_rect().width() < 1100.0;
                if in_window {
                    let clicked = self.menu_bar(ui, folded);
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
                        "Show" => (icon::PLAY_CIRCLE, "Show", "Build and play: preview, layers, clips, media, effects, mixer, cues"),
                        "Outputs" => (icon::PROJECTOR_SCREEN, "Outputs", "Outputs and inputs: every output, mapping, screens, streaming, session, LEDs, audio in"),
                        "Control" => (icon::SLIDERS_HORIZONTAL, "Control", "Controllers: MIDI / OSC, TouchOSC, Control Surface, modulators"),
                        "Designer" => (icon::GRAPH, "Designer", "Build a comp as boxes wired together: pictures, effects, mixes"),
                        other => (icon::LAYOUT, other, ""),
                    };
                    let tip = format!("{tip} ({}). Drag any panel to rearrange this area; it remembers. View > Reset Layout restores it.", crate::menu::shortcut_of(crate::menu::Action::Area(i)));
                    // Each area in its color: the show in the accent, outputs and controllers in theirs (their panels' tabs match).
                    let color = match *name {
                        "Outputs" => theme::Hue::Output.color(),
                        "Control" => theme::Hue::Control.color(),
                        "Designer" => theme::Hue::Effect.color(),
                        _ => theme::accent(),
                    };
                    if crate::ui::section_tab(ui, self.workspace == Some(i), &format!("{glyph} {text}"), color).on_hover_text(tip).clicked() {
                        self.switch_workspace(i);
                    }
                }
                ui.separator();
                // Short on room (narrow window, in-window menus): the next buttons become icons, names on hover.
                // (Also when, last frame, these ran into the buttons on the right: longer words in another language,
                // a narrow window. It goes back once there's plenty of room.)
                // Search goes to an icon and Donate and the account's name step aside; Output and Blackout keep their
                // words at any width (what ends or saves a show must read at a glance).
                let squeezed_key = egui::Id::new("top_bar_squeezed");
                let squeezed: u8 = ui.ctx().data(|d| d.get_temp(squeezed_key)).unwrap_or(0);
                let narrow = ui.available_width() < 560.0;
                let compact = narrow || squeezed >= 1;
                let named = |glyph: &str, name: &str| if compact { glyph.to_string() } else { format!("{glyph} {name}") };
                // (Surprise and the other Quick actions: the Quick menu, ⌘R, the Layers panel's Quick menu and each
                // layer's own buttons. Not a big button here: most shows never press it.)
                let (stop, play) = (format!("{} Output", icon::STOP), format!("{} Output", icon::PLAY));
                let output_label = if self.state.output_open() { stop.clone() } else { play.clone() };
                let r = crate::ui::steady_button(ui, output_label, &[&stop, &play]).on_hover_text("Output windows on / off (they go back to the screens they filled last time) · right-click: pick a screen");
                if r.clicked() {
                    self.state.request_output_toggle();
                }
                r.right_menu(|ui| {
                    crate::ui::heading(ui, "Fullscreen on");
                    crate::ui::screen_menu(ui, &mut self.state);
                });
                blackout_button(ui, &mut self.state);
                ui.separator();
                if crate::ui::command(ui, named(icon::MAGNIFYING_GLASS, &format!("Search  {}", crate::menu::shortcut_of(crate::menu::Action::Palette)))).on_hover_text("Command palette: effects, sources, panels, actions").clicked() {
                    self.state.palette.toggle();
                }
                if crate::ui::icon_toggle(ui, &mut self.state.panel_browser, icon::SQUARES_FOUR).on_hover_text(format!("Panel browser ({}): every panel here, open or closed: show, close, bring back", crate::menu::shortcut_of(crate::menu::Action::PanelBrowser))).changed() {
                    // (the toggle flips it)
                }
                if crate::ui::mode_button(ui, self.state.map_mode, format!("{} MAP", icon::PLUG))
                    .on_hover_text(format!("Map mode ({}): click any control, then move a MIDI knob, send OSC or press a key to link them", crate::menu::shortcut_of(crate::menu::Action::MapMode)))
                    .clicked()
                {
                    self.state.map_mode = !self.state.map_mode;
                    self.state.control.learning = None;
                }
                let left_end = ui.cursor().left();
                let right = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    account_button(ui, &mut self.state, compact);
                    if !compact {
                        crate::support::donate_button(ui);
                    }
                    // Only while a new version is waiting: right next to Donate, filled so it stands out.
                    self.state.updater.top_bar_button(ui);
                    // Help: the manual lives on the website (always the latest), in the app's language.
                    if crate::ui::command(ui, icon::QUESTION).on_hover_text("Help: the user manual on phosphorfx.com, always up to date").clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(crate::support::manual_url()));
                    }
                    let before = self.ui_brightness;
                    crate::ui::field_menu(ui, icon::SUN, |ui| {
                        ui.label(egui::RichText::new("Interface brightness").strong());
                        crate::ui::Level::percent(&mut self.ui_brightness, "Brightness").default(theme::DEFAULT_BRIGHTNESS).width(220.0).show(ui);
                        crate::ui::toolbar(ui, |ui| {
                            for (name, b) in theme::BRIGHTNESS_PRESETS {
                                if crate::ui::pick_item(ui, (self.ui_brightness - b).abs() < 0.01, *name).clicked() {
                                    self.ui_brightness = *b;
                                }
                            }
                        });
                        crate::ui::help(ui, "Darker for dark rooms and shows, lighter for daylight.");
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
                        ui.separator();
                        crate::ui::toolbar(ui, |ui| {
                            let mut icons = crate::ui::help_icons();
                            if crate::ui::toggle(ui, &mut icons, "Help icons").on_hover_text("Help sentences in the panels shrink to a small (i): hover or click it to read them").changed() {
                                crate::ui::set_help_icons(icons);
                            }
                            let mut jump = crate::ui::sliders_jump();
                            if crate::ui::toggle(ui, &mut jump, "Sliders jump to the click").on_hover_text("On: a click puts the slider where you clicked. Off: drag to move it from where it is (a click alone leaves it)").changed() {
                                crate::ui::set_sliders_jump(jump);
                            }
                            let mut ask = crate::ui::confirm_removes();
                            if crate::ui::toggle(ui, &mut ask, "Ask before removing").on_hover_text("On: an \"are you sure?\" before a watched folder, a collection or its folder is removed. Ticking \"Don't ask again\" there switches this off").changed() {
                                crate::ui::set_confirm_removes(ask);
                            }
                            let mut click_plays = crate::panels::media_click_plays();
                            if crate::ui::toggle(ui, &mut click_plays, "A click in Media plays").on_hover_text("Off (the usual): a click on a file in Media picks it, a double-click or a drag plays it. On: a single click puts it on air").changed() {
                                crate::panels::set_media_click_plays(click_plays);
                            }
                            let mut launches = crate::history::undo_playback();
                            if crate::ui::toggle(ui, &mut launches, "Undo includes clip launches").on_hover_text("Off (the usual): Undo steps through your edits (settings, effects, values, layout) and leaves what's playing on each layer alone. On: launching a clip or changing a layer's source is an undo step too").changed() {
                                crate::history::set_undo_playback(launches);
                            }
                            let mut donate = crate::support::donate_shown();
                            if crate::ui::toggle(ui, &mut donate, "Donate button").on_hover_text("The orange Donate button in the top bar (right-click it to hide it). Donating is always in the Help menu").changed() {
                                crate::support::set_donate_shown(donate);
                            }
                            let mut tips = theme::tooltips_on();
                            if crate::ui::toggle(ui, &mut tips, "Tooltips").on_hover_text("Off: no hover text over buttons (for people who know the app). (i) help still opens on a click").changed() {
                                theme::set_tooltips(ui.ctx(), tips);
                            }
                        });
                        #[cfg(not(target_arch = "wasm32"))]
                        crate::panels::cache::cache_rows(ui, &mut self.state);
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            ui.separator();
                            crate::support::privacy_section(ui, &mut self.state);
                            ui.separator();
                        }
                        crate::ui::field(ui, &format!("{} Language", icon::TRANSLATE), |ui| {
                            let now = crate::i18n::chosen();
                            let shown = if now == crate::i18n::Lang::Auto { format!("{} ({})", now.name(), if now.drawable() { now.resolved() } else { crate::i18n::Lang::English }.name()) } else { now.name().to_string() };
                            let mut pick = None;
                            crate::ui::pick_list(ui, "language", shown, 150.0, |ui| {
                                for lang in crate::i18n::Lang::ALL.into_iter().filter(|l| *l == crate::i18n::Lang::Auto || l.drawable()) {
                                    if crate::ui::pick_item(ui, lang == now, lang.name()).clicked() {
                                        pick = Some(lang);
                                        ui.close();
                                    }
                                }
                            });
                            if let Some(lang) = pick {
                                crate::i18n::set_language(ui.ctx(), lang);
                                // The Mac menu bar isn't drawn by us: built again in the new language.
                                self.install_native_menu();
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
                    ui.min_rect().left()
                });
                // Squeezed: the left part reached the right part.
                let gap = right.inner - left_end;
                // Level 1: the buttons go to icons; still touching: level 2, the menus fold into ☰ (in-window menus
                // only). Each level comes back only with well more room than folding saved (~330 / ~260 points).
                let now_squeezed = match squeezed {
                    0 if gap < 8.0 => 1,
                    1 if gap < 8.0 && in_window => 2,
                    2 if gap > 330.0 => 1,
                    1 if gap > 300.0 => 0,
                    s => s,
                };
                if now_squeezed != squeezed {
                    ui.ctx().data_mut(|d| d.insert_temp(squeezed_key, now_squeezed));
                    ui.ctx().request_repaint();
                }
            });
        });
    }

    /// File / Edit / View / Output / Help inside the window (same actions as the macOS menu bar). `folded`: one
    /// menu button (☰) holding them all as submenus, for a top bar that has run out of room.
    pub(crate) fn menu_bar(&mut self, ui: &mut egui::Ui, folded: bool) -> Vec<crate::menu::Action> {
        use crate::menu::MENUS;
        let mut clicked = Vec::new();
        if folded {
            ui.menu_button(icon::LIST, |ui| {
                for (title, entries) in MENUS {
                    ui.menu_button(*title, |ui| self.menu_entries(ui, entries, &mut clicked));
                }
            })
            .response
            .on_hover_text("Menu: File, Edit, Quick, View, Output, Help");
        } else {
            // Like any menu bar: once one menu is open, moving onto the next title opens that one (no second click).
            let mut bar: Vec<(egui::Id, bool)> = Vec::new();
            for (title, entries) in MENUS {
                let r = ui.menu_button(*title, |ui| self.menu_entries(ui, entries, &mut clicked)).response;
                bar.push((egui::Popup::default_response_id(&r), r.contains_pointer()));
            }
            if let Some(next) = hover_switch(&bar, |id| egui::Popup::is_id_open(ui.ctx(), id)) {
                egui::Popup::open_id(ui.ctx(), next);
                ui.ctx().request_repaint();
            }
        }
        clicked
    }

    /// One menu's items (shared by the menu bar and the folded ☰ menu); picked actions go into `clicked`.
    fn menu_entries(&mut self, ui: &mut egui::Ui, entries: &[crate::menu::Entry], clicked: &mut Vec<crate::menu::Action>) {
        use crate::menu::{Action, Entry, shortcut_text};
        let can_undo = self.state.history.can_undo(&self.state.comp);
        let can_redo = self.state.history.can_redo();
        let has_layer = self.state.selected.is_some_and(|id| id != phosphor_engine::MASTER_ID && self.state.comp.layers.iter().any(|l| l.id == id));
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
                        // Translated in pieces: the whole line isn't in the language packs.
                        Action::Syphon => format!("{}  ({})", crate::i18n::tr(label), crate::i18n::tr(if self.state.comp.share_out() { "On" } else { "Off" })),
                        Action::Ndi => format!("{}  ({})", crate::i18n::tr(label), crate::i18n::tr(if self.state.comp.ndi.enabled { "On" } else { "Off" })),
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
                                    // (Asks first when the project open now has unsaved edits or is on the outputs.)
                                    let ctx = ui.ctx().clone();
                                    self.guard(&ctx, super::guard::Guarded::OpenFile(p, false));
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
                        crate::ui::help(ui, "Drag any tab to move it; drag it outside to float it.");
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
    }
}

/// Short label for the launch settings, e.g. "Fade 0.5 s · On beat".
/// A menu bar's titles as (menu id, pointer on the title): the menu to open because the pointer moved onto its
/// title while another of the bar's menus is open. None while no menu is open, or the pointer is on the open one.
fn hover_switch(bar: &[(egui::Id, bool)], is_open: impl Fn(egui::Id) -> bool) -> Option<egui::Id> {
    let open = bar.iter().map(|(id, _)| *id).find(|id| is_open(*id))?;
    bar.iter().find(|(id, hovered)| *hovered && *id != open).map(|(id, _)| *id)
}

#[cfg(test)]
mod menu_bar_tests {
    use eframe::egui;

    #[test]
    fn hovering_another_title_switches_only_while_a_menu_is_open() {
        let (file, edit, view) = (egui::Id::new("file"), egui::Id::new("edit"), egui::Id::new("view"));
        let bar = |hovered: egui::Id| vec![(file, hovered == file), (edit, hovered == edit), (view, hovered == view)];
        assert_eq!(super::hover_switch(&bar(edit), |_| false), None, "nothing open: hovering does nothing");
        assert_eq!(super::hover_switch(&bar(edit), |id| id == file), Some(edit), "File open, pointer on Edit");
        assert_eq!(super::hover_switch(&bar(file), |id| id == file), None, "pointer still on File");
        assert_eq!(super::hover_switch(&[(file, false), (edit, false)], |id| id == file), None, "pointer in the menu itself");
    }
}

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
/// Blackout in the top bar (also while locked): every output fades to black and back on the next press; right-click
/// maps it. Always in words, at any width.
pub(crate) fn blackout_button(ui: &mut egui::Ui, st: &mut State) {
    let target = phosphor_engine::ControlTarget::App(phosphor_engine::AppControl::Blackout);
    let black = st.comp.blackout_on;
    let tip = format!("Every output fades to black; press again to bring the show back ({}; fade time: Outputs › Master) · right-click: map to a key or pad", crate::menu::shortcut_of(crate::menu::Action::Blackout));
    let r = crate::ui::mode_button(ui, black, format!("{} Blackout", icon::MOON)).on_hover_text(tip);
    if r.clicked() && !crate::ui::is_locked(ui, &target) {
        st.comp.blackout_on = !black;
    }
    crate::ui::map_menu(ui, &r, target);
}

/// `compact`: the icon alone (the top bar is short of room; the name is in its tooltip).
fn account_button(ui: &mut egui::Ui, st: &mut State, compact: bool) {
    let text = match &st.account.user {
        _ if compact => icon::USER_CIRCLE.to_string(),
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
            crate::ui::dialog(ctx, format!("{} Account", icon::USER_CIRCLE)).open(&mut open).collapsible(false).resizable(true).default_width(440.0).default_height(420.0).show(ctx, |ui| {
                crate::panels::account(ui, st);
            });
            st.account_window = open;
        }
        if st.account_hello && st.account.checked && st.account.user.is_none() && !st.account_window {
            let mut close = false;
            egui::Window::new("Welcome to Phosphor").collapsible(false).resizable(false).anchor(egui::Align2::RIGHT_TOP, [-12.0, 44.0]).show(ctx, |ui| {
                ui.set_max_width(320.0);
                ui.label("Phosphor is free. A free account keeps your projects and layouts on every computer you use. It's optional: sign in any time from File › Account & Sign In… or Sign in at the top right.");
                crate::ui::toolbar(ui, |ui| {
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
