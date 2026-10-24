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
                ui.horizontal(|ui| {
                    if crate::ui::wide_button(ui, *name, 130.0).clicked() {
                        pick = Some(k);
                    }
                    crate::ui::hint(ui, *about);
                });
            }
            crate::ui::hint(ui, "Undo (⌘Z) brings back the project you had.");
        });
        if let Some(k) = pick {
            let (name, _, make) = phosphor_engine::TEMPLATES[k];
            self.state.load_composition(make());
            self.state.project_path = None;
            self.state.notice = Some(format!("New project from \"{name}\". Undo (⌘Z) brings the previous one back."));
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
                ui.label("They become clips. Press 1–8 or click a cell to play them. Each movie's own rhythm is locked to the tempo as it lands.");
                ui.add_space(4.0);
                ui.label(egui::RichText::new("2. Let it run: AUTO and Auto show").strong());
                ui.label("The tempo follows the music by itself (your mic, an input, or the clips' own sound). Auto show plays the next column every few bars and a Surprise on drops.");
                ui.add_space(4.0);
                ui.label(egui::RichText::new("3. Map it, and hand out the controls").strong());
                ui.label("Output › send to your projector, drag the corners in Mapping. The Session panel's QR code makes a phone the remote, and a second one lets the crowd play.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if crate::ui::command(ui, "Open the Session panel").clicked() {
                        action = Some(super::Tab::Network);
                        close = true;
                    }
                    if crate::ui::command(ui, "Show the Mapping panel").clicked() {
                        action = Some(super::Tab::Mapping);
                        close = true;
                    }
                    if crate::ui::command(ui, "Start").clicked() {
                        close = true;
                    }
                });
                ui.add_space(4.0);
                crate::ui::hint(ui, "Help › What's New has everything else. This card shows once.");
            });
        if let Some(tab) = action {
            self.show_tab(tab);
        }
        if close || !open {
            self.welcome_open = false;
        }
    }
}
