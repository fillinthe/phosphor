//! Locked "player" mode: the show runs, clips and columns fire, outputs work, but the editing UI is gone
//! until someone unlocks it (optionally with a password). Exported shows can open locked.

use eframe::egui;
use egui_phosphor::regular as icon;

use super::PhosphorApp;

#[derive(Default)]
pub struct Lock {
    pub active: bool,
    /// Empty = unlock without a password.
    pub password: String,
    /// The "Lock Controls" / "Unlock" dialog.
    pub window: bool,
    pub draft: String,
    pub wrong: bool,
    /// The panels as they were before locking, put back on unlock.
    pub saved: Option<(egui_dock::DockState<crate::panels::Tab>, Vec<crate::panels::Tab>)>,
}

impl Lock {
    /// Settings text: "1:<password>" while locked (so a relaunch stays locked), "" otherwise.
    pub fn to_setting(&self) -> String {
        if self.active { format!("1:{}", self.password) } else { String::new() }
    }

    pub fn from_setting(s: &str) -> Self {
        match s.strip_prefix("1:") {
            Some(p) => Self { active: true, password: p.to_string(), ..Default::default() },
            None => Self::default(),
        }
    }
}

/// Menu actions that still work while locked.
pub fn allowed_when_locked(a: &crate::menu::Action) -> bool {
    use crate::menu::Action as A;
    matches!(a, A::Quit | A::ToggleOutput | A::Lock | A::Identify)
}

impl PhosphorApp {
    pub(crate) fn lock_now(&mut self, password: String) {
        self.lock.active = true;
        self.lock.password = password;
        self.lock.window = false;
        self.lock.draft.clear();
        // Preview, mixer and the clip grid; everything else comes back on unlock.
        if self.lock.saved.is_none() {
            self.lock.saved = Some((self.dock.clone(), std::mem::take(&mut self.detached)));
        }
        self.dock = super::layout::layout_locked();
        self.state.map_mode = false;
    }

    pub(crate) fn unlock(&mut self) {
        if let Some((dock, detached)) = self.lock.saved.take() {
            self.dock = dock;
            self.detached = detached;
        }
        self.lock = Lock::default();
    }

    /// The Lock / Unlock dialog (View › Lock Controls…, or the lock button in the top bar).
    pub(crate) fn lock_window(&mut self, ctx: &egui::Context) {
        if !self.lock.window {
            return;
        }
        let mut open = true;
        let locked = self.lock.active;
        let title = if locked { format!("{} Unlock", icon::LOCK_OPEN) } else { format!("{} Lock the controls", icon::LOCK) };
        let mut action: Option<bool> = None; // Some(true) = lock, Some(false) = unlock
        egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).default_width(360.0).show(ctx, |ui| {
            if locked {
                if self.lock.password.is_empty() {
                    ui.label("The controls are locked. Unlock to edit the show again.");
                } else {
                    ui.label("Type the password to edit the show again.");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.lock.draft).password(true).hint_text("Password").desired_width(220.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        action = Some(false);
                    }
                    if self.lock.wrong {
                        ui.colored_label(crate::theme::WARN, "That's not it.");
                    }
                }
                if crate::ui::command(ui, format!("{} Unlock", icon::LOCK_OPEN)).clicked() {
                    action = Some(false);
                }
            } else {
                ui.label("Locked, Phosphor only plays: the preview, the clip grid, keys and controllers, the outputs. Menus, panels and editing come back when you unlock.");
                ui.horizontal(|ui| {
                    ui.label("Password (optional)");
                    ui.add(egui::TextEdit::singleline(&mut self.lock.draft).password(true).desired_width(180.0));
                });
                crate::ui::hint(ui, "Leave it empty if anyone may unlock. Export Show can open the show locked too.");
                if crate::ui::command(ui, format!("{} Lock now", icon::LOCK)).clicked() {
                    action = Some(true);
                }
            }
        });
        match action {
            Some(true) => {
                let p = std::mem::take(&mut self.lock.draft);
                self.lock_now(p);
            }
            Some(false) => {
                if self.lock.password.is_empty() || self.lock.draft == self.lock.password {
                    self.unlock();
                } else {
                    self.lock.wrong = true;
                    self.lock.draft.clear();
                }
            }
            None => {}
        }
        if !open {
            self.lock.window = false;
            self.lock.wrong = false;
        }
    }

    /// Top bar while locked: just what a show needs.
    pub(crate) fn locked_top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar").show(ui, |ui| {
            crate::ui::bar_row(ui, |ui| {
                ui.label(crate::theme::logo(13.5));
                ui.label(egui::RichText::new(format!("{} Locked", icon::LOCK)).color(crate::theme::accent()));
                ui.separator();
                let (stop, play) = (format!("{} Output", icon::STOP), format!("{} Output", icon::PLAY));
                let output_label = if self.state.output_open() { stop.clone() } else { play.clone() };
                if crate::ui::steady_button(ui, output_label, &[&stop, &play]).on_hover_text("Output window. F = fullscreen, Esc = exit").clicked() {
                    self.state.toggle_output();
                }
                ui.label(format!("{} {}", icon::LIGHTNING, super::menus::launch_summary(&self.state.comp)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::ui::command(ui, format!("{} Unlock", icon::LOCK_OPEN)).clicked() {
                        self.lock.window = true;
                    }
                    let s = self.state.stats.latest;
                    if ui.available_width() > 200.0 {
                        ui.monospace(format!("{:5.1} fps  CPU {:4.1}%", s.fps, s.cpu_percent));
                    }
                });
            });
        });
    }
}
