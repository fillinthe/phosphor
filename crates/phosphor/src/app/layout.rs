//! Dock layout: panels, monitors, detached windows and the preset layouts.

use super::*;

impl PhosphorApp {
    /// Switches workspace (a built-in layout): the one you leave keeps how you arranged it, for this session.
    pub(crate) fn switch_workspace(&mut self, i: usize) {
        log::info!("area: {}", LAYOUTS.get(i).map_or("?", |(n, _)| n));
        if self.workspace == Some(i) {
            return;
        }
        if let Some(cur) = self.workspace {
            self.workspace_docks.insert(cur, (self.dock.clone(), self.detached.clone()));
        }
        match self.workspace_docks.get(&i) {
            Some((dock, detached)) => {
                self.dock = dock.clone();
                self.detached = detached.clone();
            }
            None => self.apply_layout(i),
        }
        self.workspace = Some(i);
    }

    pub(crate) fn apply_layout(&mut self, i: usize) {
        if let Some((dock, detached)) = self.layouts.get(i) {
            self.dock = dock;
            self.detached = detached;
        }
    }

    /// Rebuilds the macOS menu bar (the Layouts list changes when you save or delete one).
    pub(crate) fn install_native_menu(&mut self) {
        #[cfg(target_os = "macos")]
        if self.bench.is_none() {
            let panels: Vec<String> = Tab::ALL.iter().map(|t| t.title().to_string()).collect();
            let names = self.layouts.names();
            let layouts: Vec<&str> = names.iter().map(String::as_str).collect();
            self.native_menu = crate::menu::native::install(&panels, &layouts);
        }
    }

    /// The Save Layout window.
    pub(crate) fn save_layout_window(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.layout_draft.as_mut() else { return };
        let mut open = true;
        let mut save = false;
        egui::Window::new(format!("{} Save Layout", icon::LAYOUT)).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label("Saves where every panel is, including floating windows.");
            crate::ui::toolbar(ui, |ui| {
                ui.label("Name");
                let r = ui.text_edit_singleline(&mut draft.0);
                r.request_focus();
                save = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            });
            crate::ui::toggle(ui, &mut draft.1, "Open with this layout when Phosphor starts");
            if self.layouts.saved.iter().any(|l| l.name == draft.0.trim()) {
                crate::ui::hint(ui, "Replaces the saved layout with this name.");
            }
            save |= crate::ui::command_if(ui, !draft.0.trim().is_empty(), "Save").clicked();
        });
        if save && let Some((name, startup)) = self.layout_draft.take() {
            let name = name.trim().to_string();
            self.layouts.save(&name, &self.dock, &self.detached);
            if startup {
                self.layouts.startup = Some(name.clone());
            }
            self.state.notice = Some(format!("Saved layout \"{name}\""));
            self.install_native_menu();
        } else if !open {
            self.layout_draft = None;
        }
    }

    pub(crate) fn add_monitor_with(&mut self, source: panels::MonitorSource) {
        let n = self.add_monitor();
        if let Some(cfg) = self.state.monitors.get_mut(&n) {
            cfg.source = source;
        }
    }

    /// Opens a new Monitor panel as a floating window and returns its number.
    pub(crate) fn add_monitor(&mut self) -> u8 {
        let used: Vec<u8> = self.dock.iter_all_tabs().filter_map(|(_, t)| if let Tab::Monitor(n) = t { Some(*n) } else { None }).collect();
        let n = (1..=99).find(|n| !used.contains(n) && !self.detached.contains(&Tab::Monitor(*n))).unwrap_or(99);
        self.state.monitors.entry(n).or_default();
        self.dock.add_window(vec![Tab::Monitor(n)]);
        n
    }

    /// Brings a panel to the front, reopening it if it was closed.
    pub(crate) fn show_tab(&mut self, tab: Tab) {
        if self.detached.contains(&tab) {
            return;
        }
        match self.dock.find_tab(&tab) {
            Some(path) => {
                let _ = self.dock.set_active_tab(path);
            }
            None => {
                self.dock.push_to_focused_leaf(tab);
            }
        }
    }

    pub(crate) fn run_command(&mut self, cmd: crate::explorer::Command, now: f64) {
        use crate::explorer::{Command, PaletteDrag, apply_palette};
        let st = &mut self.state;
        match cmd {
            Command::AddEffect(id) => apply_palette(st, PaletteDrag { effect: Some(id), generator: None }, None),
            Command::SetGenerator(id) => apply_palette(st, PaletteDrag { effect: None, generator: Some(id) }, None),
            Command::OpenTab(tab) => self.show_tab(tab),
            Command::TriggerColumn(c) => st.trigger(Trigger::Column(c), now),
            Command::ToggleOutput => st.toggle_output(),
            Command::ResetLayout => self.reset_area_layout(),
            Command::NewLayer => {
                st.add_layer();
            }
            Command::TapTempo => st.clock.tap(now),
            Command::Quick(q) => st.quick(q, now),
            Command::NewShader(generator) => {
                crate::explorer::new_shader(st, phosphor_engine::custom::template(generator));
                self.show_tab(Tab::ShaderEditor);
            }
            Command::AddMonitor => {
                let _ = self.add_monitor();
            }
            Command::ToggleMapping => {
                let id = st.primary_output();
                if let Some(o) = st.comp.output_mut(id) {
                    o.mapping.enabled = !o.mapping.enabled;
                }
            }
            Command::ToggleSession => {
                if st.net.running() {
                    st.net.stop();
                } else if let Err(e) = st.net.start() {
                    st.notice = Some(e);
                }
            }
        }
    }

    /// Moves a panel out of the dock into its own OS window.
    pub(crate) fn detach(&mut self, tab: Tab) {
        if let Some(path) = self.dock.find_tab(&tab) {
            self.dock.remove_tab(path);
        }
        if !self.detached.contains(&tab) {
            self.detached.push(tab);
        }
    }

    /// Shows every detached panel in its own window; closing the window docks it again.
    pub(crate) fn detached_windows(&mut self, ctx: &egui::Context) {
        let mut redock = Vec::new();
        for tab in self.detached.clone() {
            let id = egui::ViewportId::from_hash_of(("phosphor_panel", tab));
            let builder = egui::ViewportBuilder::default().with_title(format!("Phosphor · {}", tab.title())).with_inner_size([460.0, 640.0]);
            ctx.show_viewport_immediate(id, builder, |ctx, _class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    redock.push(tab);
                }
                egui::CentralPanel::default().show(ctx, |ui| {
                    crate::ui::toolbar(ui, |ui| {
                        ui.label(egui::RichText::new(tab.title()).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if crate::ui::small_command(ui, "Dock back").clicked() {
                                redock.push(tab);
                            }
                        });
                    });
                    ui.separator();
                    let mut tab = tab;
                    egui_dock::TabViewer::ui(&mut Viewer { st: &mut self.state, detach: None }, ui, &mut tab);
                });
            });
        }
        for tab in redock {
            self.detached.retain(|t| *t != tab);
            if self.dock.find_tab(&tab).is_none() {
                self.dock.push_to_focused_leaf(tab);
            }
        }
    }

    pub(crate) fn toggle_panel(&mut self, tab: Tab) {
        if self.detached.contains(&tab) {
            self.detached.retain(|t| *t != tab);
            self.dock.push_to_focused_leaf(tab);
            return;
        }
        match self.dock.find_tab(&tab) {
            Some(path) => {
                self.dock.remove_tab(path);
            }
            None => {
                self.dock.add_window(vec![tab]);
            }
        }
    }
}

/// Everything at once: preview + monitor, clips, mixer/effects along the bottom, layers and
/// inspector down the right.
pub(crate) fn default_layout() -> DockState<Tab> {
    layout_show()
}

/// Panels added in newer builds show up in layouts saved by older ones; panels the user closed
/// on purpose stay closed.
/// Only for upgrades (a fresh install has no `known` list and starts from the defaults), and only panels whose
/// home is this area (`area`); others appear when you open their area or from View › Panels.
pub(crate) fn ensure_new_tabs(dock: &mut DockState<Tab>, known: &[Tab], area: usize) {
    if known.is_empty() {
        return;
    }
    for tab in Tab::ALL {
        if !known.contains(&tab) && super::areas::area_of(tab) == Some(area) && dock.find_tab(&tab).is_none() {
            dock.push_to_focused_leaf(tab);
        }
    }
}

/// Keeps floating panels reachable: a window saved on a bigger display, or dragged off the
/// edge, is moved back so its title bar is inside `screen`. `force` pulls every window fully inside.
pub(crate) fn rescue_windows(dock: &mut DockState<Tab>, screen: egui::Rect, force: bool) {
    let margin = 48.0;
    for surface in dock.iter_surfaces_mut() {
        let egui_dock::Surface::Window(_, ws) = surface else { continue };
        let r = ws.rect();
        if !r.is_finite() || r.width() <= 0.0 {
            continue;
        }
        let max_size = (screen.size() - egui::vec2(16.0, 16.0)).max(egui::vec2(200.0, 120.0));
        if r.width() > max_size.x || r.height() > max_size.y {
            ws.set_size(r.size().min(max_size));
        }
        let visible = egui::Rect::from_min_max(
            egui::pos2(screen.left() - r.width() + margin, screen.top()),
            egui::pos2(screen.right() - margin, screen.bottom() - margin),
        );
        let lost = !visible.contains(r.min) || r.min.y < screen.top();
        if lost || force {
            let max_pos = (screen.max - r.size()).max(screen.min);
            let target = egui::pos2(r.min.x.clamp(screen.left(), max_pos.x.max(screen.left())), r.min.y.clamp(screen.top(), max_pos.y.max(screen.top())));
            ws.set_position(target);
        }
    }
}

/// Layouts are stored as RON text: unlike JSON it keeps infinite sizes (unopened windows), which
/// JSON turns into `null` and then can't read back. Old JSON-stored layouts still load.
pub(crate) fn to_text<T: serde::Serialize>(value: &T) -> Option<String> {
    ron::to_string(value).ok()
}

pub(crate) fn from_text<T: serde::de::DeserializeOwned>(text: &str) -> Option<T> {
    ron::from_str(text).ok().or_else(|| serde_json::from_str(text).ok())
}

/// A layout you saved: the panel arrangement plus panels floating in their own windows.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct SavedLayout {
    pub name: String,
    pub dock: DockState<Tab>,
    #[serde(default)]
    pub detached: Vec<Tab>,
}

/// Built-in layouts plus yours, and which one opens at startup. Index `i` in menus counts the
/// built-ins first, then saved ones.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct Layouts {
    pub saved: Vec<SavedLayout>,
    /// `None`: reopen however it was left.
    pub startup: Option<String>,
}

impl Layouts {
    pub fn names(&self) -> Vec<String> {
        LAYOUTS.iter().map(|(n, _)| n.to_string()).chain(self.saved.iter().map(|l| l.name.clone())).collect()
    }

    pub fn get(&self, i: usize) -> Option<(DockState<Tab>, Vec<Tab>)> {
        match LAYOUTS.get(i) {
            Some((_, make)) => Some((make(), Vec::new())),
            None => self.saved.get(i - LAYOUTS.len()).map(|l| (l.dock.clone(), l.detached.clone())),
        }
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.names().iter().position(|n| n == name)
    }

    /// Saves (or replaces, same name) a layout.
    pub fn save(&mut self, name: &str, dock: &DockState<Tab>, detached: &[Tab]) {
        let l = SavedLayout { name: name.to_string(), dock: dock.clone(), detached: detached.to_vec() };
        match self.saved.iter_mut().find(|s| s.name == name) {
            Some(s) => *s = l,
            None => self.saved.push(l),
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.saved.retain(|l| l.name != name);
        if self.startup.as_deref() == Some(name) {
            self.startup = None;
        }
    }
}

/// What the account keeps of your layouts: every saved one, the startup choice and the arrangement in use.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct AccountLayouts {
    pub layouts: Layouts,
    pub current: Option<SavedLayout>,
}

/// The three areas (top-bar buttons, ⌘1 / ⌘2 / ⌘3): each job in one place, nothing shown twice.
pub(crate) const LAYOUTS: &[(&str, fn() -> DockState<Tab>)] = &[("Show", layout_show), ("Outputs", layout_outputs), ("Control", layout_control)];

/// Building and playing the show: a big preview, the clip grid (each row is a layer) and the other show panels
/// under it, the Inspector on the right. The Library opens on the left when you want it.
pub(crate) fn layout_show() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Preview]);
    let surface = dock.main_surface_mut();
    let [left, _] = surface.split_right(NodeIndex::root(), 0.75, vec![Tab::Inspector]);
    surface.split_below(left, 0.5, vec![Tab::Clips, Tab::Layers, Tab::Mixer, Tab::Cues, Tab::ShaderEditor]);
    // Media (the Library) open from the start: it's the daily tool.
    // (Wide enough for its sources list beside the tiles.)
    surface.split_left(NodeIndex::root(), 0.27, super::areas::LIBRARY.to_vec());
    dock
}

/// Where the picture goes and what comes in: every output (strip + mapping), screens, streaming and recording,
/// the phone session, LEDs, audio in, connected devices.
pub(crate) fn layout_outputs() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Studio, Tab::Stage]);
    let surface = dock.main_surface_mut();
    surface.split_below(NodeIndex::root(), 0.68, vec![Tab::Output, Tab::Stream, Tab::Network, Tab::Leds, Tab::Audio, Tab::Devices, Tab::Scopes]);
    dock
}

/// Controllers: MIDI / OSC mappings (and TouchOSC), your Control Surface, modulators.
pub(crate) fn layout_control() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Control]);
    let surface = dock.main_surface_mut();
    let [_, right] = surface.split_right(NodeIndex::root(), 0.45, vec![Tab::Surface]);
    surface.split_below(right, 0.66, vec![Tab::Modulators]);
    dock
}



/// Locked (player) mode: the preview with the mixer beside it, the clip grid underneath, nothing to edit with.
pub(crate) fn layout_locked() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Preview]);
    let surface = dock.main_surface_mut();
    let [top, _] = surface.split_below(NodeIndex::root(), 0.62, vec![Tab::Clips]);
    surface.split_right(top, 0.72, vec![Tab::Mixer]);
    dock
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_layouts_round_trip() {
        let mut l = Layouts::default();
        let builtins = LAYOUTS.len();
        l.save("Gig", &layout_control(), &[Tab::Mixer]);
        l.save("Gig", &default_layout(), &[]); // same name replaces
        l.save("Mapping night", &layout_outputs(), &[]);
        l.startup = Some("Gig".into());
        assert_eq!(l.names().len(), builtins + 2);
        assert_eq!(l.find("Gig"), Some(builtins));
        let back: Layouts = from_text(&to_text(&l).unwrap()).unwrap();
        let (dock, detached) = back.get(builtins).unwrap();
        assert!(detached.is_empty());
        assert_eq!(dock.iter_all_tabs().count(), default_layout().iter_all_tabs().count());
        assert_eq!(back.startup.as_deref(), Some("Gig"));
        let mut back = back;
        back.remove("Gig");
        assert!(back.startup.is_none());
        assert_eq!(back.names().len(), builtins + 1);
    }
}

#[cfg(test)]
mod area_tests {
    use super::*;

    /// Each panel the areas use lives in one area only (the preview may repeat: it's the picture you check).
    #[test]
    fn areas_dont_repeat_panels() {
        let mut seen = std::collections::HashSet::new();
        for (name, make) in LAYOUTS {
            for (_, tab) in make().iter_all_tabs() {
                if *tab != Tab::Preview {
                    assert!(seen.insert(*tab), "{tab:?} is in more than one area ({name})");
                }
            }
        }
        assert!(!seen.contains(&Tab::Mapping), "the Studio covers mapping");
    }
}
