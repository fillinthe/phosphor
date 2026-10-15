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
mod devices;
mod effects;
mod groups;
mod inspector;
mod layers;
mod mapping;
mod media;
mod mixer;
mod monitor;
mod network;
mod output;
mod preset_list;
mod preview;
mod stream;
mod surface;

pub(crate) use account::*;
pub(crate) use audio::*;
pub(crate) use clips::*;
pub(crate) use control::*;
pub(crate) use devices::*;
pub(crate) use effects::*;
pub(crate) use groups::*;
pub(crate) use inspector::*;
pub(crate) use layers::*;
pub(crate) use mapping::*;
pub(crate) use media::*;
pub(crate) use mixer::*;
pub(crate) use monitor::*;
pub(crate) use network::*;
pub(crate) use output::*;
pub(crate) use preset_list::*;
pub(crate) use preview::*;
pub(crate) use stream::*;
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
}

impl Tab {
    pub const ALL: [Tab; 18] = [
        Tab::Preview,
        Tab::Layers,
        Tab::Clips,
        Tab::Inspector,
        Tab::Media,
        Tab::Library,
        Tab::Presets,
        Tab::Audio,
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
        match tab {
            Tab::Preview => preview(ui, self.st),
            Tab::Layers => layers(ui, self.st),
            Tab::Clips => clips(ui, self.st),
            Tab::Inspector => inspector(ui, self.st),
            Tab::Media => media(ui, self.st),
            Tab::Library => crate::explorer::explorer(ui, self.st),
            Tab::Presets => presets(ui, self.st),
            Tab::Audio => audio(ui, self.st),
            Tab::Control => control(ui, self.st),
            Tab::Mapping => mapping(ui, self.st),
            Tab::Stream => stream(ui, self.st),
            Tab::Network => network(ui, self.st),
            Tab::Output => output(ui, self.st),
            Tab::ShaderEditor => crate::explorer::shader_editor(ui, self.st),
            Tab::Monitor(n) => monitor(ui, self.st, *n),
            Tab::Mixer => mixer(ui, self.st),
            Tab::Devices => devices(ui, self.st),
            Tab::Surface => surface(ui, self.st),
            Tab::Account => account(ui, self.st),
        }
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
