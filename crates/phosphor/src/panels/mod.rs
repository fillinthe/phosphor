//! Dockable panels. Drag a tab to split or stack it with another, or drag it out of the dock
//! area to float it in its own window.

#![allow(unused_imports)]

use eframe::egui::{self, Color32, ComboBox, RichText, Sense, Slider, Stroke, StrokeKind};
use egui_phosphor::regular as icon;
use phosphor_engine::{BlendMode, ControlTarget, EffectState, FitMode, LayerState, MASTER_ID, Mapping, ModSource, Modulation, ParamMod, PlayDirection, SourceSpec, TransitionKind, shaders};
use serde::{Deserialize, Serialize};

use crate::account::Account;
use crate::app::{State, Trigger};
use crate::theme::{accent, WARN};

pub(crate) use crate::ui::*;

mod account;
mod audio;
mod clips;
pub(crate) mod clip_bar;
mod clip_grid;
mod stack;
mod control;
mod cues;
mod devices;
mod drag_ghost;
pub(crate) use drag_ghost::drag_ghost;
mod effects;
#[cfg(not(target_arch = "wasm32"))]
mod file_info;
mod groups;
mod inspector;
mod layer_windows;
mod layers;
#[cfg(not(target_arch = "wasm32"))]
mod link_import;
mod inputs;
#[cfg(not(target_arch = "wasm32"))]
mod online;
#[cfg(not(target_arch = "wasm32"))]
mod live_data;
#[cfg(not(target_arch = "wasm32"))]
mod light_library;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use light_library::*;
#[cfg(not(target_arch = "wasm32"))]
mod ai_shader;
mod modulators;
mod sequencer;
mod leds;
mod mapping;
mod mapping_points;
mod collections;
mod comps;
mod media;
mod media_browser;
mod media_sources;
pub(crate) use media_sources::MediaSource;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use stack::layers_from_folder;
mod mixer;
mod monitor;
mod network;
mod output;
mod patch;
mod preset_list;
mod preview;
#[cfg(not(target_arch = "wasm32"))]
mod projector;
mod stream;
mod stage;
mod scopes;
#[cfg(not(target_arch = "wasm32"))]
mod scanner;
#[cfg(not(target_arch = "wasm32"))]
mod led_scan;
mod slice_tree;
mod inspect;
mod studio;
mod surface;
#[cfg(not(target_arch = "wasm32"))]
mod tracking;
mod timeline;

pub(crate) use account::*;
pub(crate) use audio::*;
pub(crate) use clips::*;
pub(crate) use comps::*;
pub(crate) use control::*;
pub(crate) use cues::*;
pub(crate) use devices::*;
pub(crate) use effects::*;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use file_info::{FileInfo, file_info_window};
pub(crate) use groups::*;
pub(crate) use inspector::*;
pub(crate) use layer_windows::*;
pub(crate) use layers::*;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use link_import::*;
pub(crate) use inputs::*;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use online::*;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use ai_shader::*;
pub(crate) use leds::*;
pub(crate) use mapping::*;
pub(crate) use media::*;
pub(crate) use mixer::*;
pub(crate) use monitor::*;
pub(crate) use network::*;
pub(crate) use output::*;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use scanner::*;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use led_scan::*;
pub(crate) use patch::*;
pub(crate) use preset_list::*;
pub(crate) use preview::*;
pub(crate) use stream::*;
pub(crate) use stage::*;
pub(crate) use inspect::*;
pub(crate) use studio::*;
pub(crate) use surface::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    Preview,
    Layers,
    Clips,
    Inspector,
    Media,
    Library,
    Presets,
    Audio,
    Modulators,
    Control,
    Mapping,
    Stream,
    Network,
    Output,
    ShaderEditor,
    /// Every output on one screen, live.
    Stage,
    /// An extra preview panel; the number tells monitors apart.
    Monitor(u8),
    Mixer,
    Devices,
    Surface,
    Account,
    Leds,
    Cues,
    LayerWindows,
    Studio,
    /// Waveform / parade / vectorscope / histogram.
    Scopes,
    /// Live inputs (Library): cameras, screens, Syphon, NDI, streams.
    Inputs,
    /// Notes kept with the project (show running order, venue details…).
    Notes,
    /// Keyframe timeline: tracks of keys over time.
    Timeline,
    /// Free videos and pictures from online libraries (Library).
    Online,
    /// Every layer with its controls and its clips side by side; groups and comps as bands (replaces Clips + Layers).
    Stack,
    /// A panel you made (View › New Panel): Control Surface page N, your own buttons and sliders.
    MyPanel(u8),
}

impl Tab {
    pub const ALL: [Tab; 30] = [
        Tab::Preview,
        Tab::Stack,
        Tab::Layers,
        Tab::Clips,
        Tab::Inspector,
        Tab::Media,
        Tab::Library,
        Tab::Presets,
        Tab::Audio,
        Tab::Modulators,
        Tab::Control,
        Tab::Mapping,
        Tab::Stream,
        Tab::Network,
        Tab::Output,
        Tab::ShaderEditor,
        Tab::Stage,
        Tab::Mixer,
        Tab::Devices,
        Tab::Surface,
        Tab::Account,
        Tab::Leds,
        Tab::Cues,
        Tab::LayerWindows,
        Tab::Studio,
        Tab::Scopes,
        Tab::Inputs,
        Tab::Notes,
        Tab::Timeline,
        Tab::Online,
    ];

    /// The kind of panel, shown by its tab's colour (`theme::Hue`); None = the show itself (preview, layers, clips…).
    pub fn hue(self) -> Option<crate::theme::Hue> {
        use crate::theme::Hue;
        Some(match self {
            Tab::Media | Tab::Online => Hue::Media,
            Tab::Library | Tab::ShaderEditor => Hue::Effect,
            Tab::Inputs | Tab::Audio => Hue::Live,
            Tab::Output | Tab::Mapping | Tab::Studio | Tab::Stage | Tab::Stream | Tab::Network | Tab::Leds | Tab::LayerWindows => Hue::Output,
            Tab::Control | Tab::Surface | Tab::MyPanel(_) | Tab::Modulators | Tab::Devices => Hue::Control,
            _ => return None,
        })
    }

    pub fn title(self) -> String {
        match self {
            Tab::Monitor(n) => return format!("Preview {n}"),
            Tab::MyPanel(n) => return format!("Panel {}", n + 1),
            _ => {}
        }
        match self {
            Tab::Preview => "Preview",
            Tab::Layers => "Layers",
            Tab::Clips => "Clips",
            Tab::Stack => "Stack",
            Tab::Inspector => "Inspector",
            Tab::Media => "Media",
            Tab::Library => "Effects",
            Tab::Presets => "Presets",
            Tab::Inputs => "Inputs",
            Tab::Online => "Online",
            Tab::Audio => "Audio",
            Tab::Modulators => "LFOs & Modulators",
            Tab::Control => "MIDI / OSC",
            Tab::Mapping => "Mapping",
            Tab::Stream => "Stream & Record",
            Tab::Network => "Session",
            Tab::Output => "Output & Stats",
            Tab::ShaderEditor => "Shader Editor",
            Tab::Mixer => "Mixer",
            Tab::Devices => "Devices",
            Tab::Surface => "Control Surface",
            Tab::Account => "Account",
            Tab::Leds => "LED / DMX",
            Tab::Scopes => "Scopes",
            Tab::Notes => "Notes",
            Tab::Timeline => "Timeline",
            Tab::Cues => "Cues",
            Tab::LayerWindows => "Layer Windows",
            Tab::Studio => "Mapping Studio",
            Tab::Stage => "Stage",
            Tab::Monitor(_) | Tab::MyPanel(_) => unreachable!(),
        }
        .to_string()
    }
}

pub struct Viewer<'a> {
    pub st: &'a mut State,
    /// Set when the user picks "Detach" from a tab's menu; the app moves it to its own window.
    pub detach: Option<Tab>,
}

impl egui_dock::TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Monitor(n) => format!("{} {}", icon::MONITOR, monitor_source_name(self.st, *n)).into(),
            Tab::MyPanel(n) => {
                let name = self.st.comp.surface.pages.get(*n as usize).cloned().unwrap_or_else(|| tab.title());
                RichText::new(format!("{} {name}", icon::SLIDERS_HORIZONTAL)).color(crate::theme::Hue::Control.color()).into()
            }
            // Coloured by what kind of panel it is.
            _ => match tab.hue() {
                Some(h) => RichText::new(tab.title()).color(h.color()).into(),
                None => tab.title().into(),
            },
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        crate::ui::press::set_panel(ui.ctx(), Some((&format!("{tab:?}"), &tab.title())));
        draw_tab(ui, self.st, *tab);
        crate::ui::press::set_panel(ui.ctx(), None);
    }

    /// Closed panels come back from View ▸ Panels.
    fn is_closeable(&self, _tab: &Tab) -> bool {
        true
    }

    /// Panels manage their own scrolling; the dock must never pan content sideways.
    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        [false, false]
    }

    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Tab, _path: egui_dock::NodePath) {
        if let Tab::MyPanel(n) = *tab
            && let Some(name) = self.st.comp.surface.pages.get_mut(n as usize)
        {
            ui.label(RichText::new("Panel name").small());
            ui.text_edit_singleline(name);
            if self.st.comp.surface.pages.len() > 1 && crate::ui::danger_button(ui, label(icon::TRASH, "Delete this panel and its controls")).clicked() {
                surface::remove_page(ui.ctx(), &mut self.st.comp.surface, n as usize);
                ui.close();
            }
            ui.separator();
        }
        if crate::ui::command(ui, label(icon::ARROW_SQUARE_OUT, "Detach to its own window")).on_hover_text("Move this panel to a separate window (drag it to another screen)").clicked() {
            self.detach = Some(*tab);
            ui.close();
        }
    }
}

/// Draws one panel (the dock and the areas both use this).
pub(crate) fn draw_tab(ui: &mut egui::Ui, st: &mut State, tab: Tab) {
    match tab {
        Tab::Preview => preview(ui, st),
        Tab::Layers => layers(ui, st),
        Tab::Clips => clips(ui, st),
        Tab::Stack => stack::stack(ui, st),
        Tab::Inspector => inspector(ui, st),
        Tab::Media => media(ui, st),
        Tab::Library => crate::explorer::explorer(ui, st),
        Tab::Presets => presets(ui, st),
        Tab::Inputs => inputs_panel(ui, st),
        #[cfg(not(target_arch = "wasm32"))]
        Tab::Online => online_panel(ui, st),
        #[cfg(target_arch = "wasm32")]
        Tab::Online => {
            help(ui, "Online libraries work in the Mac, Windows and Linux apps.");
        }
        Tab::Notes => notes_panel(ui, st),
        Tab::Timeline => timeline::timeline_panel(ui, st),
        // (Panels with no scrolling of their own scroll here, so nothing is hidden when they're short.)
        Tab::Audio => scrolled(ui, "audio_scroll", |ui| audio(ui, st)),
        Tab::Modulators => modulators::modulators(ui, st),
        Tab::Control => control(ui, st),
        Tab::Mapping => mapping(ui, st),
        Tab::Stream => scrolled(ui, "stream_scroll", |ui| stream(ui, st)),
        Tab::Network => scrolled(ui, "network_scroll", |ui| network(ui, st)),
        Tab::Output => scrolled(ui, "output_scroll", |ui| output(ui, st)),
        Tab::ShaderEditor => crate::explorer::shader_editor(ui, st),
        Tab::Monitor(n) => monitor(ui, st, n),
        Tab::Mixer => mixer(ui, st),
        Tab::Devices => devices(ui, st),
        Tab::Surface => surface(ui, st),
        Tab::MyPanel(n) => surface::my_panel(ui, st, n as usize),
        Tab::Account => account(ui, st),
        Tab::Leds => leds(ui, st),
        Tab::Scopes => scopes::scopes(ui, st),
        Tab::Cues => cues(ui, st),
        Tab::LayerWindows => layer_windows(ui, st),
        Tab::Studio => studio(ui, st),
        Tab::Stage => stage(ui, st),
    }
}

/// A panel's whole contents in a scrolling column (for panels that don't scroll by themselves).
fn scrolled(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    crate::ui::column_scroll(ui).id_salt(id).auto_shrink(false).show(ui, add);
}

/// Notes saved with the project: running order, venue contacts, what each cue is for.
fn notes_panel(ui: &mut egui::Ui, st: &mut State) {
    hint(ui, "Kept in the project file.");
    crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
        ui.add(egui::TextEdit::multiline(&mut st.comp.notes).hint_text("Running order, venue details, reminders…").desired_width(f32::INFINITY).desired_rows(20));
    });
}
