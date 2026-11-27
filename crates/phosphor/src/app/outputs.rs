//! Output windows (borderless, per screen), mapping sync and the phone remote sync.

use super::*;
use crate::ui::RightMenu;

/// Seconds the way back to a window stays on an output that has just covered Phosphor's own window.
const WAY_BACK_SECS: f64 = 8.0;

impl PhosphorApp {
    /// An output filling a display (F, double-click, the right-click menu) is remembered in the project as that
    /// output's screen, so it goes back there when its window reopens or the project is opened again.
    pub(crate) fn remember_output_screens(&mut self) {
        let displays = &self.state.displays;
        let filled: Vec<(u64, u32)> = self.state.output.lock().unwrap().iter().filter(|(_, o)| o.fullscreen && o.place.is_none()).filter_map(|(id, o)| o.display.map(|d| (*id, d))).collect();
        for (id, did) in filled {
            let Some(d) = displays.iter().find(|d| d.id == did) else { continue };
            let key = crate::screens::screen_key(displays, d);
            if let Some(o) = self.state.comp.outputs.iter_mut().find(|o| o.id == id)
                && o.screen != key
            {
                log::info!(target: "phosphor::action", "output \"{}\" now fills the screen \"{key}\"", o.name);
                o.screen = key;
            }
        }
    }

    /// An output filling the screen Phosphor's own window is on leaves a picture and no controls: no menu bar, no
    /// Dock, nothing to click (reports 2026-11-25: runs ended from outside a minute after the control window was
    /// covered, with one output open). The output is told so (it shows the way back and keeps the pointer), takes
    /// the keys, and Esc typed in the covered window brings it back to a window too.
    ///
    /// "Thy word is a lamp unto my feet, and a light unto my path." (Psalm 119:105, KJV)
    pub(crate) fn outputs_over_controls(&mut self, ctx: &egui::Context) {
        let mut map = self.state.output.lock().unwrap();
        if !map.values().any(|o| o.open && o.fullscreen || o.over_controls) {
            return;
        }
        let own = crate::screens::display_of_window(&self.state.displays, ctx);
        let (now, escape) = ctx.input(|i| (i.time, i.key_pressed(egui::Key::Escape)));
        for (id, out) in map.iter_mut() {
            let over = out.open && out.fullscreen && out.place.is_none() && own.is_some() && out.display == own;
            if over && !out.over_controls {
                log::info!(target: "phosphor::action", "output \"{}\" covers the screen Phosphor's own window is on", out.name);
                out.way_back_until = now + WAY_BACK_SECS;
                ctx.send_viewport_cmd_to(egui::ViewportId::from_hash_of(("phosphor_output", *id)), egui::ViewportCommand::Focus);
            }
            out.over_controls = over;
            if over && escape {
                out.uncover = true;
            }
        }
    }

    /// A screen unplugged mid-show hides the output filling it (rather than letting the system drop it onto another
    /// screen); plugged back in, the output fills it again. Reads the display list only when due (see `ScreenWatch`).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn watch_screens(&mut self, now: f64) {
        use crate::screen_watch::{Act, Placed};
        let placed: Vec<Placed> = {
            let map = self.state.output.lock().unwrap();
            self.state
                .comp
                .outputs
                .iter()
                .map(|o| {
                    let w = map.get(&o.id);
                    let filling = w.and_then(|w| w.place.as_ref().map(|p| p.0.id).or(if w.fullscreen { w.display } else { None }));
                    Placed { output: o.id, name: o.name.clone(), open: w.is_some_and(|w| w.open), filling, screen: o.screen.clone() }
                })
                .collect()
        };
        let watching = placed.iter().any(|p| p.open);
        if !self.state.screen_watch.due(now, watching) {
            return;
        }
        let displays = crate::screens::displays();
        let acts = self.state.screen_watch.update(&displays, &placed, now);
        if !displays.is_empty() && displays != self.state.displays {
            self.state.displays = displays;
        }
        if acts.is_empty() {
            return;
        }
        for act in &acts {
            match act {
                Act::Hide(away) => {
                    log::warn!(target: "phosphor::action", "screen \"{}\" unplugged: output \"{}\" hidden until it's back", away.key, away.name);
                    if let Some(w) = self.state.output.lock().unwrap().get_mut(&away.output) {
                        w.open = false;
                        w.fullscreen = false;
                        w.place = None;
                    }
                }
                Act::Restore { output, name, display } => {
                    log::info!(target: "phosphor::action", "screen \"{}\" back: output \"{name}\" fills it again", display.name);
                    if self.state.comp.outputs.iter().any(|o| o.id == *output && o.enabled) {
                        self.state.send_output_id_to(*output, display.clone());
                    }
                }
            }
        }
        self.state.notice = crate::screen_watch::notice(&acts);
    }

    /// Phone/tablet remote: apply what remotes sent, then publish the state (≈20 times a second).
    pub(crate) fn remote_sync(&mut self, now: f64) {
        if !self.state.net.running() {
            return;
        }
        let pw = self.state.require_password.then(|| self.state.remote_password.clone());
        self.state.net.set_password(pw);
        let cmds = self.state.net.take_remote_commands();
        if !self.state.comp.web_control_off {
            for cmd in cmds {
                self.state.note_web_activity(&cmd, now);
                crate::remote::apply(&mut self.state, &cmd, now);
            }
        }
        if now - self.remote_published > 0.05 {
            self.remote_published = now;
            let alive: Vec<u64> = self.state.comp.layers.iter().map(|l| l.id).collect();
            self.state.net.retain_thumbs(&alive);
            let json = crate::remote::state_json(&mut self.state, now);
            self.state.net.set_remote_state(json);
        }
    }

    /// Pulls corner drags made in the output window back into the composition, then any Undo / Redo asked for there.
    pub(crate) fn sync_mapping_from_output(&mut self) {
        let mut steps: Vec<i8> = Vec::new();
        {
            let mut map = self.state.output.lock().unwrap();
            for (id, out) in map.iter_mut() {
                if out.mapping_changed {
                    if let Some(o) = self.state.comp.outputs.iter_mut().find(|o| o.id == *id) {
                        o.mapping = out.mapping.clone();
                    }
                    self.state.selected_surface = out.selected_surface;
                    self.state.mapping_output = *id;
                    out.mapping_changed = false;
                }
                steps.append(&mut out.undo_steps);
            }
        }
        for step in steps {
            self.state.undo_step(step);
        }
        // Only for output windows being edited: with nothing to undo yet the check compares the whole project, and
        // it ran every frame with no window open (review 2026-11-15).
        let mut map = self.state.output.lock().unwrap();
        if map.values().any(|o| o.open && o.edit_mapping) {
            let (can_undo, can_redo) = (self.state.history.can_undo(&self.state.comp), self.state.history.can_redo());
            for out in map.values_mut() {
                (out.can_undo, out.can_redo) = (can_undo, can_redo);
            }
        }
    }

    /// A mouse button is down in an output window (a mapping drag there is one undo step, like one in the main window).
    pub(crate) fn output_pointer_down(&self) -> bool {
        self.state.output.lock().unwrap().values().any(|o| o.open && o.pointer_down)
    }

    // "Let your light so shine before men, that they may see your good works, and glorify your Father which is in heaven." Matthew 5:16 (KJV)
    pub(crate) fn output_windows(&mut self, ctx: &egui::Context) {
        // Any output open: the displays stay awake (no display sleep / screen saver / lock in the middle of a show).
        let any_open = self.state.output.lock().unwrap().values().any(|o| o.open);
        crate::screens::keep_display_awake(any_open);
        let open: Vec<(u64, String)> =
            self.state.output.lock().unwrap().iter().filter(|(_, o)| o.open).map(|(id, o)| (*id, o.name.clone())).collect();
        #[cfg(not(target_arch = "wasm32"))]
        for (oid, name) in open {
            self.output_window(ctx, oid, &name);
        }
        // In the browser: pop-up windows (a panel inside the interface can't go to a projector).
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            self.web_output_windows(open);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn web_output_windows(&mut self, open: Vec<(u64, String)>) {
        let (Some(rs), Some(engine)) = (self.state.render_state.as_ref(), self.state.engine.as_ref()) else { return };
        let comp = &self.state.comp;
        let open: Vec<(u64, String, (u32, u32))> = open
            .into_iter()
            .map(|(id, name)| {
                let size = comp.outputs.iter().find(|o| o.id == id).map_or((comp.width, comp.height), |o| o.size((comp.width, comp.height)));
                (id, name, size)
            })
            .collect();
        let shown = self.web_outputs.show(rs, &open, |id| engine.output_texture(id).or_else(|| engine.output()).map(|t| &t.texture));
        if !shown.closed.is_empty() {
            let mut map = self.state.output.lock().unwrap();
            for id in &shown.closed {
                if let Some(o) = map.get_mut(id) {
                    o.open = false;
                }
            }
        }
        if shown.blocked {
            self.state.notice = Some("The browser blocked the output window: allow pop-ups for this site (the icon at the right of the address bar), then press Output again.".into());
        }
    }

    pub(crate) fn output_window(&self, ctx: &egui::Context, oid: u64, name: &str) {
        let id = egui::ViewportId::from_hash_of(("phosphor_output", oid));
        let shared = self.state.output.clone();
        let title = format!("Phosphor · {name}");
        ctx.show_viewport_deferred(
            id,
            // Borderless: nothing but picture, so a projector sees no title bar or menu bar.
            egui::ViewportBuilder::default()
                .with_title(title.clone())
                .with_decorations(false)
                .with_inner_size([960.0, 540.0]),
            move |ui, _class| {
                let ctx = ui.ctx().clone();
                let mut map = shared.lock().unwrap();
                let Some(out) = map.get_mut(&oid) else { return };
                // "Fullscreen" = this window covering one display exactly (not macOS full screen,
                // which would black out the other displays).
                let fullscreen = out.fullscreen;
                if ctx.input(|i| i.key_pressed(egui::Key::E)) {
                    out.edit_mapping = !out.edit_mapping;
                }
                // Undo / Redo typed here (the main window's shortcuts don't see this window's keys; on the Mac the
                // menu bar takes them first, so they never arrive twice). Ctrl+Y = Redo too.
                out.pointer_down = ctx.input(|i| i.pointer.any_down());
                let (undo, redo) = ctx.input_mut(|i| {
                    let redo = i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z))
                        || i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y));
                    (i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z)), redo)
                });
                if redo {
                    out.undo_steps.push(1);
                } else if undo {
                    out.undo_steps.push(-1);
                }
                // Screen placement: leave macOS full screen if somehow in it, then cover the display.
                if let Some((d, step, wait)) = out.place.as_mut() {
                    *wait += 1;
                    match *step {
                        0 => {
                            if ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                                *step = 1;
                            } else {
                                *step = 2;
                            }
                            *wait = 0;
                        }
                        1 if *wait > 45 => {
                            *step = 2;
                            *wait = 0;
                        }
                        2 => {
                            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(d.x, d.y)));
                            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(d.w, d.h)));
                            *step = 3;
                            *wait = 0;
                        }
                        3 if *wait > 2 => {
                            if !crate::screens::cover_window(&ctx, &title, Some(d)) {
                                log::warn!("output window '{title}' not found to cover the display");
                            }
                            out.display = Some(d.id);
                            out.fullscreen = true;
                            *step = 4;
                        }
                        _ => {}
                    }
                    if *step == 4 {
                        out.place = None;
                    }
                }
                // Still covering its screen? macOS puts a window back under the menu bar now and then, which slid the
                // picture (and every slice mapped on it) down by the menu bar's height. Looked at once a second.
                let now = ctx.input(|i| i.time);
                if fullscreen && out.place.is_none() && now - out.cover_checked >= 1.0 {
                    out.cover_checked = now;
                    if let Some(d) = out.display.and_then(|id| crate::screens::displays().into_iter().find(|d| d.id == id))
                        && crate::screens::covers(&title, &d) == Some(false)
                    {
                        if now - out.cover_logged >= 60.0 || out.cover_logged == 0.0 {
                            out.cover_logged = now;
                            log::info!(target: "phosphor::action", "output \"{}\" had moved off its screen's edges (macOS): put back", out.name);
                        }
                        crate::screens::cover_window(&ctx, &title, Some(&d));
                    }
                }
                if ctx.input(|i| i.viewport().close_requested()) {
                    out.open = false;
                }
                let toggle = ctx.input(|i| i.key_pressed(egui::Key::F));
                let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                // Back to a normal window on the same display.
                let uncover = |out: &mut OutputShared| {
                    crate::screens::cover_window(&ctx, &title, None);
                    out.fullscreen = false;
                    if let Some(d) = out.display.and_then(|id| crate::screens::displays().into_iter().find(|d| d.id == id)) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(960.0, 540.0)));
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(d.x + 60.0, d.y + 60.0)));
                    }
                };
                // Fill the display the window is on (the first one when that can't be told: Wayland says no positions).
                let cover_here = |out: &mut OutputShared| {
                    let all = crate::screens::displays();
                    let on = crate::screens::display_of_window(&all, &ctx);
                    if let Some(d) = all.iter().find(|d| Some(d.id) == on).or(all.first()) {
                        out.place = Some((d.clone(), 2, 0));
                    }
                };
                if toggle {
                    if fullscreen { uncover(out) } else { cover_here(out) }
                }
                if std::mem::take(&mut out.uncover) && fullscreen {
                    uncover(out);
                }
                if escape {
                    if fullscreen {
                        uncover(out);
                    } else {
                        out.open = false;
                    }
                }
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
                let frame = panels::fit_rect(rect, out.aspect);
                if let Some(tex) = out.texture {
                    panels::paint_texture(ui.painter(), tex, frame);
                }
                // A projector shows no pointer, except while its mapping handles are out: then you need it to drag them
                // (bug report: Edit on output in fullscreen on Windows, no pointer to move anything with).
                let over_controls = fullscreen && out.over_controls;
                if fullscreen && !out.edit_mapping && !over_controls {
                    ctx.set_cursor_icon(egui::CursorIcon::None);
                }
                // Over Phosphor's own window: the way back, for a few seconds and whenever the pointer moves.
                if over_controls && ctx.input(|i| i.time < out.way_back_until || i.pointer.is_moving()) {
                    if ctx.input(|i| i.pointer.is_moving()) {
                        out.way_back_until = ctx.input(|i| i.time) + 3.0;
                    }
                    let words = "This output covers Phosphor's own window · Esc or double-click = back to a window";
                    let font = egui::FontId::proportional(18.0);
                    let at = rect.center_bottom() + egui::vec2(0.0, -28.0);
                    let size = ui.painter().layout_no_wrap(words.into(), font.clone(), Color32::WHITE).size();
                    ui.painter().rect_filled(egui::Rect::from_center_size(at - egui::vec2(0.0, size.y / 2.0), size + egui::vec2(28.0, 16.0)), 0.0, Color32::from_black_alpha(190));
                    ui.painter().text(at, egui::Align2::CENTER_BOTTOM, words, font, Color32::WHITE);
                }
                if !out.edit_mapping {
                    // No title bar, so dragging the picture moves the window (to the projector).
                    let drag = ui.interact(rect, egui::Id::new("output_drag"), egui::Sense::click_and_drag());
                    if drag.drag_started() && !fullscreen {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag.double_clicked() {
                        if fullscreen { uncover(out) } else { cover_here(out) }
                    }
                    // Right-click: fill any display from here (no dragging across screens needed).
                    let mut go: Option<crate::screens::Display> = None;
                    let mut back = false;
                    drag.right_menu(|ui| {
                        crate::ui::heading(ui, "Fullscreen on");
                        for d in crate::screens::displays() {
                            let here = out.display == Some(d.id) && fullscreen;
                            if crate::ui::pick_item(ui, here, format!("{} ({}×{})", d.name, d.w as u32, d.h as u32)).clicked() {
                                go = Some(d);
                                ui.close();
                            }
                        }
                        ui.separator();
                        if crate::ui::command_if(ui, fullscreen, "Back to a window (Esc)").clicked() {
                            back = true;
                            ui.close();
                        }
                    });
                    if let Some(d) = go {
                        out.place = Some((d, 2, 0));
                    }
                    if back {
                        uncover(out);
                    }
                    if !fullscreen {
                        ui.painter().text(
                            rect.left_bottom() + egui::vec2(12.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            "Drag to move · double-click or F = fullscreen · E = mapping handles · Esc = close",
                            egui::FontId::proportional(12.0),
                            Color32::from_white_alpha(110),
                        );
                    }
                }
                if ctx.input(|i| i.time) < out.identify_until {
                    let c = rect.center();
                    ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(140));
                    ui.painter().text(c, egui::Align2::CENTER_CENTER, out.number.to_string(), egui::FontId::proportional(rect.height() * 0.45), theme::accent());
                    ui.painter().text(c + egui::vec2(0.0, rect.height() * 0.28), egui::Align2::CENTER_CENTER, &out.name, egui::FontId::proportional(28.0), Color32::WHITE);
                }
                if out.edit_mapping {
                    let OutputShared { mapping, selected_surface, mapping_changed, .. } = &mut *out;
                    if panels::mapping_handles(ui, frame, mapping, selected_surface, true) {
                        *mapping_changed = true;
                    }
                    // Undo / Redo right here: on a projector there's no menu bar to reach for after a bad drag.
                    let bar = egui::Rect::from_min_size(rect.left_bottom() + egui::vec2(12.0, -42.0), egui::vec2(rect.width() - 24.0, 30.0));
                    ui.scope_builder(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                        if crate::ui::command_if(ui, out.can_undo, crate::ui::label(icon::ARROW_COUNTER_CLOCKWISE, "Undo")).on_hover_text("Undo the last change (Cmd/Ctrl+Z)").clicked() {
                            out.undo_steps.push(-1);
                        }
                        if crate::ui::command_if(ui, out.can_redo, crate::ui::label(icon::ARROW_CLOCKWISE, "Redo")).on_hover_text("Redo (Shift+Cmd/Ctrl+Z)").clicked() {
                            out.undo_steps.push(1);
                        }
                        ui.label(egui::RichText::new("Mapping edit: drag corners · Cmd/Ctrl+Z undo · E hides handles · F fullscreen").size(13.0).color(Color32::from_white_alpha(160)));
                    });
                }
            },
        );
        ctx.request_repaint_of(id);
    }
}

impl State {
    /// One step back (-1) or forward (+1) in the project's history: the Edit menu, the shortcuts and the output
    /// windows all come through here.
    pub(crate) fn undo_step(&mut self, step: i8) {
        let got = if step < 0 { self.history.undo(&self.comp) } else { self.history.redo(&self.comp) };
        if let Some(mut c) = got {
            crate::history::keep_imports(&self.comp, &mut c);
            self.comp = c;
            self.comp.ensure_outputs();
            // What was selected may not be in this step (a layer made after it, a clip slot since removed):
            // a selection pointing at nothing is let go, so no panel edits the wrong thing.
            let has_layer = |c: &phosphor_engine::Composition, id: u64| id == phosphor_engine::MASTER_ID || c.layers.iter().any(|l| l.id == id) || c.groups.iter().any(|g| g.id == id) || c.comps.iter().any(|k| k.id == id);
            let has_clip = |c: &phosphor_engine::Composition, (l, slot): (u64, usize)| c.layer(l).is_some_and(|l| slot < l.clips.len());
            if self.selected.is_some_and(|id| !has_layer(&self.comp, id)) {
                self.selected = self.comp.layers.last().map(|l| l.id);
            }
            self.also_selected.retain(|id| has_layer(&self.comp, *id));
            if self.selected_clip.is_some_and(|k| !has_clip(&self.comp, k)) {
                self.selected_clip = None;
            }
            self.also_clips.retain(|k| has_clip(&self.comp, *k));
            // A slice, mask or slice group made after this step.
            let has_slice = |c: &phosphor_engine::Composition, id: u64| c.outputs.iter().any(|o| o.mapping.surfaces.iter().any(|s| s.id == id) || o.mapping.masks.iter().any(|m| m.id == id) || o.mapping.groups.iter().any(|g| g.id == id));
            if self.selected_surface.is_some_and(|id| !has_slice(&self.comp, id)) {
                self.selected_surface = None;
            }
            self.pending_triggers.clear();
            self.notice = Some(if step < 0 { "Undone".into() } else { "Redone".into() });
        } else {
            self.notice = Some(if step < 0 { "Nothing to undo".into() } else { "Nothing to redo".into() });
        }
    }
}
