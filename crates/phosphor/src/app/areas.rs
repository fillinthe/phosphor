//! The unified frame: every area (Show, Outputs, Control) has the same shape. The Library on the left (media,
//! effects and generators, presets: drag from it onto anything), the area's stage in the middle, one Inspector on
//! the right for what's selected, the status line at the bottom. Each panel lives in exactly one place
//! (`Area::tabs`). The free dock is still there as "Custom" (`PhosphorApp::use_areas` off).

use eframe::egui::{self, Color32, RichText};

use super::PhosphorApp;
use crate::panels::{self, Tab};

/// Which tabs each area's stage shows under its main view (the first is the default).
const SHOW_BOTTOM: &[Tab] = &[Tab::Clips, Tab::Mixer, Tab::Cues, Tab::ShaderEditor];
const OUTPUTS_BOTTOM: &[Tab] = &[Tab::Output, Tab::Stream, Tab::Network, Tab::Leds, Tab::Audio, Tab::Devices];
const LIBRARY: &[Tab] = &[Tab::Media, Tab::Library, Tab::Presets];

/// Where a panel lives in the areas: (area, sub-tab group it belongs to).
pub(crate) fn area_of(tab: Tab) -> Option<usize> {
    match tab {
        Tab::Preview | Tab::Layers | Tab::Inspector => Some(0),
        t if SHOW_BOTTOM.contains(&t) => Some(0),
        Tab::Studio | Tab::Mapping => Some(1),
        t if OUTPUTS_BOTTOM.contains(&t) => Some(1),
        Tab::Control | Tab::Surface | Tab::Modulators => Some(2),
        _ => None,
    }
}

/// The per-area choices of sub-tab (remembered while the app runs).
#[derive(Clone, Copy)]
pub(crate) struct AreaTabs {
    pub library: Tab,
    pub show_bottom: Tab,
    pub outputs_bottom: Tab,
    /// The side columns can be hidden (status line toggles) to give the stage the room.
    pub show_library: bool,
    pub show_inspector: bool,
}

impl Default for AreaTabs {
    fn default() -> Self {
        Self { library: Tab::Media, show_bottom: Tab::Clips, outputs_bottom: Tab::Output, show_library: true, show_inspector: true }
    }
}

/// A row of small tabs (the Library's kinds, the stage's lower half).
fn sub_tabs(ui: &mut egui::Ui, current: &mut Tab, tabs: &[Tab]) {
    ui.horizontal(|ui| {
        for t in tabs {
            let text = RichText::new(t.title()).size(12.5);
            if ui.selectable_label(*current == *t, text).clicked() {
                *current = *t;
            }
        }
    });
    ui.add_space(2.0);
}

/// A panel that may be taller than its space, kept inside it.
fn scrolled(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical().id_salt(id).auto_shrink(false).show(ui, add);
}

/// Small caps heading for a column.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text.to_uppercase()).size(11.0).strong().color(Color32::from_gray(140)));
}

/// The middle of an area.
fn stage(ui: &mut egui::Ui, area: usize, both: &mut (&mut crate::app::State, &mut AreaTabs)) {
    match area {
        0 => crate::ui::split_vertical(
            ui,
            "show_bottom",
            0.4,
            140.0,
            both,
            |ui, (st, _)| {
                crate::ui::split_horizontal(ui, "show_layers", 0.3, 310.0, *st, |ui, st| panels::draw_tab(ui, st, Tab::Preview), |ui, st| {
                    heading(ui, "Layers");
                    panels::draw_tab(ui, st, Tab::Layers);
                })
            },
            |ui, (st, tabs)| {
                sub_tabs(ui, &mut tabs.show_bottom, SHOW_BOTTOM);
                panels::draw_tab(ui, st, tabs.show_bottom);
            },
        ),
        1 => crate::ui::split_vertical(ui, "outputs_bottom", 0.32, 120.0, both, |ui, (st, _)| panels::studio_stage(ui, st), |ui, (st, tabs)| {
            sub_tabs(ui, &mut tabs.outputs_bottom, OUTPUTS_BOTTOM);
            scrolled(ui, "outputs_bottom", |ui| panels::draw_tab(ui, st, tabs.outputs_bottom));
        }),
        _ => crate::ui::split_horizontal(
            ui,
            "control",
            0.58,
            320.0,
            &mut *both.0,
            |ui, st| {
                heading(ui, "MIDI / OSC / TouchOSC");
                panels::draw_tab(ui, st, Tab::Control);
            },
            |ui, st| {
                heading(ui, "Control Surface");
                panels::draw_tab(ui, st, Tab::Surface);
            },
        ),
    }
}

impl PhosphorApp {
    /// Draws the current area in the unified frame.
    pub(crate) fn areas_frame(&mut self, ui: &mut egui::Ui) {
        let area = self.workspace.unwrap_or(0).min(2);
        let tabs = &mut self.area_tabs;
        let st = &mut self.state;
        // Side columns: their width changes only when you drag their edge, capped at about a quarter of the window
        // each, so the stage keeps its room; their content can't push them wider.
        let w = ui.available_width();
        let left = tabs.show_library.then_some((250.0f32.min(w * 0.22), 170.0, (w * 0.22).max(180.0)));
        let right = tabs.show_inspector.then_some((330.0f32.min(w * 0.26), 220.0, (w * 0.26).max(220.0)));
        let mut both = (st, tabs);
        crate::ui::columns(
            ui,
            "areas",
            left,
            right,
            &mut both,
            |ui, (st, tabs)| {
                heading(ui, "Library");
                sub_tabs(ui, &mut tabs.library, LIBRARY);
                panels::draw_tab(ui, st, tabs.library);
            },
            |ui, both| stage(ui, area, both),
            |ui, (st, _)| {
                heading(ui, match area {
                    1 => "Output",
                    2 => "Modulators",
                    _ => "Inspector",
                });
                match area {
                    1 => scrolled(ui, "area_inspector_out", |ui| panels::studio_properties(ui, st)),
                    2 => panels::draw_tab(ui, st, Tab::Modulators),
                    _ => panels::draw_tab(ui, st, Tab::Inspector),
                }
            },
        );
    }

    /// Something asked for a panel: in the areas, go to the area that has it (choosing its sub-tab), or open it
    /// as a floating window if no area does.
    pub(crate) fn show_tab_in_areas(&mut self, tab: Tab) {
        match area_of(tab) {
            Some(a) => {
                self.workspace = Some(a);
                if SHOW_BOTTOM.contains(&tab) {
                    self.area_tabs.show_bottom = tab;
                } else if OUTPUTS_BOTTOM.contains(&tab) {
                    self.area_tabs.outputs_bottom = tab;
                }
            }
            None if LIBRARY.contains(&tab) => self.area_tabs.library = tab,
            None => self.detach(tab),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every panel has one home: an area, the Library, or (rare ones) a floating window.
    #[test]
    fn each_panel_has_one_home() {
        for t in Tab::ALL {
            let in_library = LIBRARY.contains(&t);
            assert!(!(in_library && area_of(t).is_some()), "{t:?} is in the Library and an area");
        }
        for t in SHOW_BOTTOM.iter().chain(OUTPUTS_BOTTOM) {
            assert!(area_of(*t).is_some());
        }
    }
}
