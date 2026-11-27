//! First launch: three things to try, shown once (then only from Help).

use super::*;

impl PhosphorApp {
    /// File › New from Template: the ready-made starting points, one click each.
    pub(crate) fn templates_window(&mut self, ctx: &egui::Context) {
        if !self.state.templates_open {
            return;
        }
        let mut open = true;
        let mut pick: Option<usize> = None;
        egui::Window::new("New from a template").collapsible(false).resizable(false).open(&mut open).anchor(egui::Align2::CENTER_CENTER, [0.0, -40.0]).show(ctx, |ui| {
            ui.set_width(420.0);
            for (k, (name, about, _)) in phosphor_engine::TEMPLATES.iter().enumerate() {
                crate::ui::toolbar(ui, |ui| {
                    if crate::ui::wide_button(ui, *name, 130.0).clicked() {
                        pick = Some(k);
                    }
                    crate::ui::hint(ui, *about);
                });
            }
            crate::ui::hint(ui, "It replaces the project open now (you're asked first when that has unsaved changes).");
        });
        if let Some(k) = pick {
            self.guard(ctx, super::guard::Guarded::Template(k));
            open = false;
        }
        self.state.templates_open = open;
    }

    /// The welcome card. `self.welcome_open` is set on the very first launch (no saved settings yet).
    pub(crate) fn welcome_window(&mut self, ctx: &egui::Context) {
        if !self.welcome_open {
            return;
        }
        let mut open = true;
        let mut close = false;
        let mut action: Option<super::Tab> = None;
        let mut go: Option<crate::menu::Action> = None;
        egui::Window::new("Welcome to Phosphor")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -40.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label("Free live visuals that find the beat by themselves. Three things to try:");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("1. Drop movies, pictures or a track onto the grid").strong());
                ui.label(format!("They become clips. Press 1–{} or click a cell to play them. Each movie's own rhythm is locked to the tempo as it lands.", crate::panels::CLIP_PAGE_SIZE));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("2. Let it run: AUTO and Auto show").strong());
                ui.label("The tempo follows the music by itself (your mic, an input, or the clips' own sound). Auto show plays the next column every few bars and a Surprise on drops.");
                ui.add_space(4.0);
                ui.label(egui::RichText::new("3. Map it, and hand out the controls").strong());
                ui.label("Output › send to your projector, drag the corners in Mapping. The Session panel's QR code makes a phone the remote, and a second one lets the crowd play.");
                ui.add_space(10.0);
                // The two ways to start (Go): your own media, or a set made for you. Then the panels for later.
                crate::ui::toolbar(ui, |ui| {
                    if crate::ui::action_button(ui, crate::ui::Act::Go, crate::ui::label(icon::FILE_PLUS, "Import media…")).on_hover_text("Movies, pictures or a track: they become clips").clicked() {
                        go = Some(crate::menu::Action::Import);
                        close = true;
                    }
                    if crate::ui::action_button(ui, crate::ui::Act::Go, crate::ui::label(icon::LIGHTNING, "Instant set")).on_hover_text("Layers and clips made for you, ready to play to the music").clicked() {
                        go = Some(crate::menu::Action::Quick(super::Quick::InstantSet));
                        close = true;
                    }
                });
                crate::ui::toolbar(ui, |ui| {
                    if crate::ui::command(ui, "Open the Session panel").clicked() {
                        action = Some(super::Tab::Network);
                        close = true;
                    }
                    if crate::ui::command(ui, "Show the Mapping panel").clicked() {
                        action = Some(super::Tab::Mapping);
                        close = true;
                    }
                    if crate::ui::command(ui, "Close").clicked() {
                        close = true;
                    }
                    crate::ui::help(ui, "Help › What's New has everything else. This card shows once.");
                });
            });
        if let Some(a) = go {
            self.state.menu_requests.push(a);
        }
        if let Some(tab) = action {
            self.show_tab(tab);
        }
        if close || !open {
            self.welcome_open = false;
        }
    }
}
