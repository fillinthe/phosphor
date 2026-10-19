//! Dockable panels. Drag a tab to split or stack it with another, or drag it out of the dock
//! area to float it in its own window.

#![allow(unused_imports)]

use eframe::egui::{self, Color32, ComboBox, RichText, Sense, Slider, Stroke, StrokeKind};
use egui_phosphor::regular as icon;
use phosphor_engine::{BlendMode, ControlTarget, EffectState, FitMode, LayerState, MASTER_ID, Mapping, ModSource, Modulation, ParamMod, PlayDirection, SourceSpec, TransitionKind, shaders};
use serde::{Deserialize, Serialize};

use crate::account::Account;
use crate::app::{State, Trigger};
use crate::theme::{ACCENT, WARN};

pub(crate) use crate::ui::*;

mod account;
mod audio;
mod clips;
mod control;
mod cues;
mod devices;
mod effects;
mod groups;
mod inspector;
mod layer_windows;
mod layers;
mod modulators;
mod leds;
mod mapping;
mod media;
mod mixer;
mod monitor;
mod network;
mod output;
mod preset_list;
mod preview;
mod stream;
mod studio;
mod surface;

pub(crate) use account::*;
pub(crate) use audio::*;
pub(crate) use clips::*;
pub(crate) use control::*;
pub(crate) use cues::*;
pub(crate) use devices::*;
pub(crate) use effects::*;
pub(crate) use groups::*;
pub(crate) use inspector::*;
pub(crate) use layer_windows::*;
pub(crate) use layers::*;
pub(crate) use leds::*;
pub(crate) use mapping::*;
pub(crate) use media::*;
pub(crate) use mixer::*;
pub(crate) use monitor::*;
pub(crate) use network::*;
pub(crate) use output::*;
pub(crate) use preset_list::*;
pub(crate) use preview::*;
pub(crate) use stream::*;
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
}

impl Tab {
    pub const ALL: [Tab; 23] = [
        Tab::Preview,
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
        Tab::Mixer,
        Tab::Devices,
        Tab::Surface,
        Tab::Account,
        Tab::Leds,
        Tab::Cues,
        Tab::LayerWindows,
        Tab::Studio,
    ];

    pub fn title(self) -> String {
        match self {
            Tab::Monitor(n) => return format!("Monitor {n}"),
            _ => {}
        }
        match self {
            Tab::Preview => "Preview",
            Tab::Layers => "Layers",
            Tab::Clips => "Clips",
            Tab::Inspector => "Inspector",
            Tab::Media => "Media",
            Tab::Library => "Effects",
            Tab::Presets => "Presets",
            Tab::Audio => "Audio",
            Tab::Modulators => "Modulators",
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
            Tab::Cues => "Cues",
            Tab::LayerWindows => "Layer Windows",
            Tab::Studio => "Mapping Studio",
            Tab::Monitor(_) => unreachable!(),
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
            _ => tab.title().into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        draw_tab(ui, self.st, *tab);
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
        if ui.button(label(icon::ARROW_SQUARE_OUT, "Detach to its own window")).on_hover_text("Move this panel to a separate window (drag it to another screen)").clicked() {
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
        Tab::Inspector => inspector(ui, st),
        Tab::Media => media(ui, st),
        Tab::Library => crate::explorer::explorer(ui, st),
        Tab::Presets => presets(ui, st),
        Tab::Audio => audio(ui, st),
        Tab::Modulators => modulators::modulators(ui, st),
        Tab::Control => control(ui, st),
        Tab::Mapping => mapping(ui, st),
        Tab::Stream => stream(ui, st),
        Tab::Network => network(ui, st),
        Tab::Output => output(ui, st),
        Tab::ShaderEditor => crate::explorer::shader_editor(ui, st),
        Tab::Monitor(n) => monitor(ui, st, n),
        Tab::Mixer => mixer(ui, st),
        Tab::Devices => devices(ui, st),
        Tab::Surface => surface(ui, st),
        Tab::Account => account(ui, st),
        Tab::Leds => leds(ui, st),
        Tab::Cues => cues(ui, st),
        Tab::LayerWindows => layer_windows(ui, st),
        Tab::Studio => studio(ui, st),
    }
}
