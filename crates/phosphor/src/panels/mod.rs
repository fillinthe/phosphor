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
mod phone_osc;
mod audio;
mod clips;
mod clip_keep;
pub(crate) mod designer;
pub(crate) mod clip_bar;
mod clip_grid;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod cache;
#[cfg(not(target_arch = "wasm32"))]
mod led_network;
mod stack;
mod control;
mod links;
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
mod clip_lasers;
mod laser_box;
mod timeline_lasers;
mod laser_zones;
mod lasers;
mod mapping;
pub(crate) mod mapping_points;
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
pub(crate) use phone_osc::*;
pub(crate) use effects::*;
pub(crate) use inspector::reveal_clip_effects;
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
    /// A comp built as boxes wired together (pictures, effects, mixes).
    Designer,
    /// Every layer with its controls and its clips side by side; groups and comps as bands (replaces Clips + Layers).
    Stack,
    /// A panel you made (View › New Panel): Control Surface page N, your own buttons and sliders.
    MyPanel(u8),
    /// Laser projectors and the Laser Designer.
    Lasers,
}

impl Tab {
    pub const ALL: [Tab; 32] = [
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
        Tab::Lasers,
        Tab::Cues,
        Tab::LayerWindows,
        Tab::Studio,
        Tab::Scopes,
        Tab::Inputs,
        Tab::Notes,
        Tab::Timeline,
        Tab::Online,
        Tab::Designer,
    ];

    /// The kind of panel, shown by its tab's color (`theme::Hue`); None = the show itself (preview, layers, clips…).
    pub fn hue(self) -> Option<crate::theme::Hue> {
        use crate::theme::Hue;
        Some(match self {
            Tab::Media | Tab::Online => Hue::Media,
            Tab::Library | Tab::ShaderEditor | Tab::Designer => Hue::Effect,
            Tab::Inputs | Tab::Audio => Hue::Live,
            Tab::Output | Tab::Mapping | Tab::Studio | Tab::Stage | Tab::Stream | Tab::Network | Tab::Leds | Tab::Lasers | Tab::LayerWindows => Hue::Output,
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
            Tab::Designer => "Designer",
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
            Tab::Lasers => "Lasers",
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
        // (No color here: `tab_style_override` colors it, full for the open tab, dimmed for the others.)
        match tab {
            Tab::Monitor(n) => format!("{} {}", icon::MONITOR, monitor_source_name(self.st, *n)).into(),
            Tab::MyPanel(n) => {
                let name = self.st.comp.surface.pages.get(*n as usize).cloned().unwrap_or_else(|| tab.title());
                format!("{} {name}", icon::SLIDERS_HORIZONTAL).into()
            }
            _ => tab.title().into(),
        }
    }

    /// The open tab of each group in its panel's color (what kind of panel it is) with an outline of it; the
    /// panel last clicked gets a faint wash of it too (owner, 2026-11-04: a solid fill was too loud); the others
    /// dimmed, so which one is open (and which one you're in) shows without shouting.
    fn tab_style_override(&self, tab: &Tab, global: &egui_dock::TabStyle) -> Option<egui_dock::TabStyle> {
        let hue = match tab {
            Tab::MyPanel(_) => Some(crate::theme::Hue::Control),
            _ => tab.hue(),
        };
        let full = hue.map_or(crate::theme::ON_PICTURE, |h| h.color());
        let dim = hue.map_or(crate::theme::faint(), |h| h.color().gamma_multiply(0.6));
        let mut style = global.clone();
        // No outline on the open tab (owner, 2026-11-04): the color of the words says which one is open.
        for s in [&mut style.active, &mut style.active_with_kb_focus] {
            s.text_color = full;
            s.outline_color = Color32::TRANSPARENT;
        }
        // The panel last clicked (where keys and clicks go): a faint wash of its color under the words.
        for s in [&mut style.focused, &mut style.focused_with_kb_focus] {
            s.bg_fill = full.gamma_multiply(0.18);
            s.text_color = full;
            s.outline_color = Color32::TRANSPARENT;
        }
        for s in [&mut style.inactive, &mut style.inactive_with_kb_focus] {
            s.text_color = dim;
        }
        // Rolling over a closed tab: a wash of its color, no outline.
        style.hovered.text_color = full;
        style.hovered.bg_fill = full.gamma_multiply(0.12);
        style.hovered.outline_color = Color32::TRANSPARENT;
        Some(style)
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
            crate::ui::rename_item(ui, ("panel_name", n), name);
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

/// Temp-data key: each movie layer's CPU share (of one core) for its decoding, refreshed once a second.
pub(crate) const CPU_LAYERS: &str = "phosphor_cpu_layers";

/// A movie layer's decoding as a share of one CPU core (0..1+), when it's measured.
pub(crate) fn cpu_share(ctx: &egui::Context, layer: u64) -> Option<f32> {
    ctx.data(|d| d.get_temp::<(f64, std::sync::Arc<std::collections::HashMap<u64, f32>>)>(egui::Id::new(CPU_LAYERS))).and_then(|(_, m)| m.get(&layer).copied())
}

/// The color for a CPU share (of one core): gray while light, amber from 30%, red from 70%.
pub(crate) fn cpu_color(share: f32) -> Color32 {
    if share >= 0.7 {
        crate::theme::DANGER
    } else if share >= 0.3 {
        WARN
    } else {
        Color32::GRAY
    }
}

/// What to know about each playing movie, by layer: its file and the advice (`media::health`), set once a second.
pub(crate) const PLAY_HEALTH: &str = "phosphor_play_health";
/// A file whose File info was asked for from a card (taken in by the File info window).
pub(crate) const FILE_INFO_REQUEST: &str = "phosphor_file_info_request";

#[cfg(not(target_arch = "wasm32"))]
type PlayHealth = std::sync::Arc<std::collections::HashMap<u64, (String, std::sync::Arc<Vec<crate::media::health::Advice>>)>>;

/// The advice for the movie on `layer`, with its file.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn play_health(ctx: &egui::Context, layer: u64) -> Option<(String, std::sync::Arc<Vec<crate::media::health::Advice>>)> {
    ctx.data(|d| d.get_temp::<PlayHealth>(egui::Id::new(PLAY_HEALTH))).and_then(|m| m.get(&layer).cloned())
}

/// A warning sign beside a layer's CPU figure while its movie stutters or is likely to (dropping frames, a heavy
/// decode, a network share): hover for what's wrong and what to do, click for File info, where it's all listed.
/// Its place is always kept, so the row never shifts when the sign comes and goes.
pub(crate) fn health_chip(ui: &mut egui::Ui, layer: u64) {
    let slot = egui::vec2(18.0, crate::ui::CONTROL_H);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((path, advice)) = play_health(ui.ctx(), layer).filter(|(_, a)| a.iter().any(|x| x.serious)) {
        let serious: Vec<crate::media::health::Advice> = advice.iter().filter(|x| x.serious).cloned().collect();
        let sign = egui::Label::new(RichText::new(icon::WARNING).color(WARN)).selectable(false).sense(egui::Sense::click());
        let r = ui.add_sized(slot, sign).on_hover_text(format!("{}\n\nClick for the whole list (File info)", crate::media::health::as_text(&serious)));
        if r.clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(FILE_INFO_REQUEST), path));
        }
        return;
    }
    let _ = layer;
    ui.add_space(slot.x);
}

/// A layer's CPU: "CPU 34%" in its color, one width whatever the number (the row never shifts), the advice on hover.
pub(crate) fn cpu_chip(ui: &mut egui::Ui, share: Option<f32>) {
    let Some(share) = share else {
        ui.add_space(62.0);
        return;
    };
    let pct = (share * 100.0).round() as u32;
    let text = RichText::new(format!("CPU {pct:>3}%")).small().monospace().color(cpu_color(share));
    let tip = if share >= 0.3 {
        format!("Decoding this movie takes {pct}% of one CPU core (this Mac has {} cores). Hap plays on the graphics card instead: Media › right-click the file › Convert… › Hap", cores())
    } else {
        format!("Decoding this movie takes {pct}% of one CPU core (this Mac has {} cores)", cores())
    };
    ui.add_sized([62.0, crate::ui::CONTROL_H], egui::Label::new(text).selectable(false)).on_hover_text(tip);
}

/// This computer's CPU cores.
pub(crate) fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
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
        Tab::Designer => designer::designer_panel(ui, st),
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
        Tab::Lasers => scrolled(ui, "lasers_scroll", |ui| lasers::lasers(ui, st)),
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
