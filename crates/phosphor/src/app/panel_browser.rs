//! The panel browser (View › Panel Browser…, the top bar's panels button): every panel in this area at a glance:
//! docked, floating, in its own window, or closed. Show one (a floating one lost off screen comes back on it),
//! close one to make things simpler, open a closed one, or dock a separate window back.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use super::PhosphorApp;
use crate::panels::Tab;

enum Act {
    Show(Tab),
    Close(Tab),
    Open(Tab),
    DockBack(Tab),
    Gather,
}

impl PhosphorApp {
    pub(crate) fn panel_browser(&mut self, ctx: &egui::Context) {
        if !self.state.panel_browser {
            return;
        }
        let mut open = true;
        let mut acts = Vec::new();
        let name = |st: &crate::app::State, t: Tab| match t {
            Tab::MyPanel(n) => st.comp.surface.pages.get(n as usize).cloned().unwrap_or_else(|| t.title()),
            _ => t.title(),
        };
        // Where each open panel is: (tab, docked / floating, showing now).
        let main = egui_dock::SurfaceIndex::main();
        let here: Vec<(Tab, bool, bool)> = self
            .dock
            .iter_all_tabs()
            .map(|(path, t)| {
                let showing = self.dock[path.surface][path.node].get_leaf().is_some_and(|l| l.active == path.tab);
                (*t, path.surface != main, showing)
            })
            .collect();
        let detached = self.detached.clone();
        let pages = self.state.comp.surface.pages.len();
        let closed: Vec<Tab> = Tab::ALL
            .iter()
            .copied()
            .chain((0..pages.min(255)).map(|p| Tab::MyPanel(p as u8)))
            .filter(|t| !here.iter().any(|(h, ..)| h == t) && !detached.contains(t))
            .collect();
        egui::Window::new(RichText::new(format!("{} Panels", icon::SQUARES_FOUR)).strong()).open(&mut open).default_pos(egui::pos2(40.0, 70.0)).default_width(320.0).resizable(true).collapsible(false).show(ctx, |ui| {
            crate::ui::toolbar(ui, |ui| {
                if crate::ui::command(ui, crate::ui::label(icon::ARROWS_IN, "Gather floating")).on_hover_text("Bring every floating panel back onto the screen").clicked() {
                    acts.push(Act::Gather);
                }
                crate::ui::help(ui, "Show a panel, close it to keep things simple, or open one you closed. Floating ones lost off screen come back when you Show them.");
            });
            crate::ui::column_scroll(ui).max_height(520.0).show(ui, |ui| {
                crate::ui::heading(ui, "Open here");
                for (t, floating, showing) in &here {
                    crate::ui::toolbar(ui, |ui| {
                        let where_ = if *floating { "floating" } else { "docked" };
                        let text = RichText::new(format!("{}  ", name(&self.state, *t))).color(if *showing { crate::theme::accent() } else { ui.visuals().text_color() });
                        if crate::ui::pick_item(ui, *showing, text).on_hover_text("Show it (in front, on screen)").clicked() {
                            acts.push(Act::Show(*t));
                        }
                        crate::ui::hint(ui, where_);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if crate::ui::small_command(ui, icon::X).on_hover_text("Close it (it stays below, under Closed)").clicked() {
                                acts.push(Act::Close(*t));
                            }
                        });
                    });
                }
                if !detached.is_empty() {
                    ui.separator();
                    crate::ui::heading(ui, "In their own windows");
                    for t in &detached {
                        crate::ui::toolbar(ui, |ui| {
                            ui.label(name(&self.state, *t));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if crate::ui::small_command(ui, "Dock back").clicked() {
                                    acts.push(Act::DockBack(*t));
                                }
                            });
                        });
                    }
                }
                ui.separator();
                crate::ui::heading(ui, "Closed");
                crate::ui::toolbar(ui, |ui| {
                    for t in &closed {
                        if crate::ui::small_command(ui, crate::ui::label(icon::PLUS, &name(&self.state, *t))).on_hover_text("Open it here").clicked() {
                            acts.push(Act::Open(*t));
                        }
                    }
                });
            });
        });
        self.state.panel_browser = open;
        for a in acts {
            match a {
                Act::Show(t) => {
                    self.show_tab(t);
                    // A floating one somewhere off screen comes back.
                    self.gather_windows = true;
                }
                Act::Close(t) => {
                    if let Some(path) = self.dock.find_tab(&t) {
                        self.dock.remove_tab(path);
                    }
                }
                Act::Open(Tab::MyPanel(n)) => self.open_my_panel(n as usize),
                Act::Open(t) => self.show_tab(t),
                Act::DockBack(t) => {
                    self.detached.retain(|x| *x != t);
                    if self.dock.find_tab(&t).is_none() {
                        self.dock.push_to_focused_leaf(t);
                    }
                }
                Act::Gather => self.gather_windows = true,
            }
        }
    }
}
