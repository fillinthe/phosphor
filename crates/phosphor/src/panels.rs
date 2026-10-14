//! Dockable panels. Drag a tab to split or stack it with another, or drag it out of the dock
//! area to float it in its own window.

use eframe::egui::{self, Color32, ComboBox, RichText, Sense, Slider, Stroke, StrokeKind};
use egui_phosphor::regular as icon;
use phosphor_engine::{BlendMode, EffectState, FitMode, LayerState, MASTER_ID, Mapping, ModSource, Modulation, ParamMod, SourceSpec, shaders};
use serde::{Deserialize, Serialize};

use crate::app::State;
use crate::theme::{ACCENT, WARN};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    Preview,
    Layers,
    Inspector,
    Library,
    Presets,
    Audio,
    Control,
    Mapping,
    Stream,
    Output,
}

impl Tab {
    pub const ALL: [Tab; 10] = [
        Tab::Preview,
        Tab::Layers,
        Tab::Inspector,
        Tab::Library,
        Tab::Presets,
        Tab::Audio,
        Tab::Control,
        Tab::Mapping,
        Tab::Stream,
        Tab::Output,
    ];

    fn title(self) -> &'static str {
        match self {
            Tab::Preview => "Preview",
            Tab::Layers => "Layers",
            Tab::Inspector => "Inspector",
            Tab::Library => "Library",
            Tab::Presets => "Presets",
            Tab::Audio => "Audio",
            Tab::Control => "MIDI / OSC",
            Tab::Mapping => "Mapping",
            Tab::Stream => "Stream & Record",
            Tab::Output => "Output & Stats",
        }
    }
}

pub struct Viewer<'a> {
    pub st: &'a mut State,
}

impl egui_dock::TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Preview => preview(ui, self.st),
            Tab::Layers => layers(ui, self.st),
            Tab::Inspector => inspector(ui, self.st),
            Tab::Library => library(ui, self.st),
            Tab::Presets => presets(ui, self.st),
            Tab::Audio => audio(ui, self.st),
            Tab::Control => control(ui, self.st),
            Tab::Mapping => mapping(ui, self.st),
            Tab::Stream => stream(ui, self.st),
            Tab::Output => output(ui, self.st),
        }
    }

    fn is_closeable(&self, _tab: &Tab) -> bool {
        false
    }

    /// Panels manage their own scrolling; the dock must never pan content sideways.
    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        [false, false]
    }
}

pub fn fit_rect(area: egui::Rect, aspect: f32) -> egui::Rect {
    let size = if area.width() / area.height().max(1.0) > aspect {
        egui::vec2(area.height() * aspect, area.height())
    } else {
        egui::vec2(area.width(), area.width() / aspect)
    };
    egui::Rect::from_center_size(area.center(), size)
}

const FULL_UV: egui::Rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

pub fn paint_texture(painter: &egui::Painter, tex: egui::TextureId, rect: egui::Rect) {
    painter.image(tex, rect, FULL_UV, Color32::WHITE);
}

fn label(icon: &str, text: &str) -> String {
    format!("{icon} {text}")
}

/// A layer's live picture; dimmed when the layer is off, a placeholder when it has no source.
fn thumbnail(ui: &mut egui::Ui, tex: Option<egui::TextureId>, size: egui::Vec2, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, Color32::from_gray(10));
    match tex {
        Some(t) => {
            let tint = if enabled { Color32::WHITE } else { Color32::from_white_alpha(80) };
            p.image(t, rect, FULL_UV, tint);
        }
        None => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "no source", egui::FontId::proportional(11.0), Color32::GRAY);
        }
    }
    let stroke = if resp.hovered() { Stroke::new(1.0, ACCENT) } else { Stroke::new(1.0, Color32::from_gray(52)) };
    p.rect_stroke(rect, 4.0, stroke, StrokeKind::Inside);
    resp
}

fn preview(ui: &mut egui::Ui, st: &mut State) {
    egui::Panel::bottom("preview_bar").show_separator_line(false).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("MASTER").small().strong());
            ui.spacing_mut().slider_width = (ui.available_width() - 170.0).max(80.0);
            ui.add(Slider::new(&mut st.comp.master_fade, 0.0..=1.0).show_value(false));
            ui.monospace(format!("{:3.0}%", st.comp.master_fade * 100.0));
            let text = if st.output_open() { label(icon::STOP, "Output") } else { label(icon::PLAY, "Output") };
            if ui.button(text).on_hover_text("Open the output window (F = fullscreen, E = edit mapping)").clicked() {
                st.toggle_output();
            }
        });
    });
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 4.0, Color32::from_gray(6));
    match st.texture {
        Some(tex) => paint_texture(ui.painter(), tex, fit_rect(rect.shrink(6.0), st.aspect())),
        None => {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "GPU unavailable", egui::FontId::proportional(16.0), Color32::GRAY);
        }
    }
    let s = st.stats.latest;
    let mut info = format!("{}×{}  {:.0} fps", st.comp.width, st.comp.height, s.fps);
    if st.comp.mapping.active() {
        info = format!("mapped · {info}");
    }
    let font = egui::FontId::monospace(11.0);
    let galley = ui.painter().layout_no_wrap(info, font.clone(), Color32::from_white_alpha(170));
    let pill = egui::Rect::from_min_max(
        rect.right_top() + egui::vec2(-galley.size().x - 18.0, 6.0),
        rect.right_top() + egui::vec2(-4.0, galley.size().y + 14.0),
    );
    ui.painter().rect_filled(pill, 4.0, Color32::from_black_alpha(150));
    ui.painter().galley(pill.min + egui::vec2(7.0, 4.0), galley, Color32::WHITE);
    ui.allocate_rect(rect, Sense::hover());
}

enum LayerAction {
    Up,
    Down,
    Duplicate,
    Remove,
}

fn layers(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        if ui.button(label(icon::PLUS, "Add Layer")).clicked() {
            st.add_layer();
        }
        ui.weak("Top of the list is the top of the stack");
    });
    ui.add_space(2.0);
    let aspect = st.aspect();
    let output_tex = st.texture;
    let State { comp, selected, media, layer_textures, .. } = st;
    let count = comp.layers.len();
    let mut action = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if master_card(ui, output_tex, aspect, *selected == Some(MASTER_ID), &comp.master_effects) {
            *selected = Some(MASTER_ID);
        }
        ui.add_space(4.0);
        for idx in (0..count).rev() {
            let layer = &mut comp.layers[idx];
            let thumb = layer_textures.get(&layer.id).map(|(t, _)| *t);
            let err = media.errors.get(&layer.id).cloned();
            let is_selected = *selected == Some(layer.id);
            let (select, act) = layer_card(ui, layer, thumb, aspect, is_selected, err.as_deref());
            if select {
                *selected = Some(layer.id);
            }
            if let Some(a) = act {
                action = Some((idx, a));
            }
            ui.add_space(4.0);
        }
    });
    match action {
        Some((i, LayerAction::Up)) if i + 1 < count => comp.layers.swap(i, i + 1),
        Some((i, LayerAction::Down)) if i > 0 => comp.layers.swap(i, i - 1),
        Some((i, LayerAction::Duplicate)) => {
            let mut copy = comp.layers[i].clone();
            copy.id = comp.new_id();
            copy.name = format!("{} copy", copy.name);
            for fx in &mut copy.effects {
                fx.id = comp.new_id();
            }
            *selected = Some(copy.id);
            comp.layers.insert(i + 1, copy);
        }
        Some((i, LayerAction::Remove)) => {
            let removed = comp.layers.remove(i);
            if *selected == Some(removed.id) {
                *selected = comp.layers.last().map(|l| l.id);
            }
        }
        _ => {}
    }
}

fn card_frame(ui: &egui::Ui, is_selected: bool) -> egui::Frame {
    let stroke = if is_selected { Stroke::new(1.5, ACCENT) } else { ui.visuals().widgets.noninteractive.bg_stroke };
    let fill = if is_selected { Color32::from_rgb(24, 34, 30) } else { ui.visuals().faint_bg_color };
    egui::Frame::new().fill(fill).stroke(stroke).corner_radius(6).inner_margin(egui::Margin::same(8))
}

/// The pinned card for the whole mix: shows the output and the master effect count.
fn master_card(ui: &mut egui::Ui, tex: Option<egui::TextureId>, aspect: f32, is_selected: bool, effects: &[EffectState]) -> bool {
    let mut select = false;
    let frame = card_frame(ui, is_selected).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if thumbnail(ui, tex, egui::vec2(112.0, 112.0 / aspect), true).clicked() {
                select = true;
            }
            ui.vertical(|ui| {
                if ui.selectable_label(is_selected, RichText::new("MASTER").strong().size(14.0).color(ACCENT)).clicked() {
                    select = true;
                }
                ui.weak("Effects here run over all layers");
                let fx = effects.iter().filter(|e| e.enabled).count();
                if fx > 0 {
                    ui.label(RichText::new(format!("{fx} fx")).small().color(ACCENT));
                }
            });
        });
    });
    if frame.response.interact(Sense::click()).clicked() {
        select = true;
    }
    select
}

fn layer_card(
    ui: &mut egui::Ui,
    layer: &mut LayerState,
    thumb: Option<egui::TextureId>,
    aspect: f32,
    is_selected: bool,
    err: Option<&str>,
) -> (bool, Option<LayerAction>) {
    let mut select = false;
    let mut action = None;
    let frame = card_frame(ui, is_selected).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let r = thumbnail(ui, thumb, egui::vec2(112.0, 112.0 / aspect), layer.enabled)
                .on_hover_text("Click to select · double-click to toggle on/off");
            if r.clicked() {
                select = true;
            }
            if r.double_clicked() {
                layer.enabled = !layer.enabled;
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.toggle_value(&mut layer.enabled, RichText::new("ON").small().strong()).on_hover_text("Layer on/off");
                    if ui.selectable_label(is_selected, RichText::new(&layer.name).strong().size(14.0)).clicked() {
                        select = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::TRASH).on_hover_text("Delete layer").clicked() {
                            action = Some(LayerAction::Remove);
                        }
                        if ui.small_button(icon::COPY).on_hover_text("Duplicate layer").clicked() {
                            action = Some(LayerAction::Duplicate);
                        }
                        if ui.small_button(icon::ARROW_DOWN).on_hover_text("Move down the stack").clicked() {
                            action = Some(LayerAction::Down);
                        }
                        if ui.small_button(icon::ARROW_UP).on_hover_text("Move up the stack").clicked() {
                            action = Some(LayerAction::Up);
                        }
                    });
                });
                source_picker(ui, layer);
                ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width = (ui.available_width() - 100.0).max(60.0);
                    ui.add(Slider::new(&mut layer.opacity, 0.0..=1.0).show_value(false)).on_hover_text("Opacity");
                    ui.monospace(format!("{:3.0}%", layer.opacity * 100.0));
                    mod_button(ui, &mut layer.opacity_mod);
                });
                ui.horizontal(|ui| {
                    ComboBox::from_id_salt(("blend", layer.id)).width(104.0).selected_text(layer.blend.name()).show_ui(ui, |ui| {
                        for mode in BlendMode::ALL {
                            ui.selectable_value(&mut layer.blend, mode, mode.name());
                        }
                    });
                    ComboBox::from_id_salt(("fit", layer.id)).width(74.0).selected_text(layer.fit.name()).show_ui(ui, |ui| {
                        for mode in FitMode::ALL {
                            ui.selectable_value(&mut layer.fit, mode, mode.name());
                        }
                    });
                    let fx = layer.effects.iter().filter(|e| e.enabled).count();
                    if fx > 0 {
                        ui.label(RichText::new(format!("{fx} fx")).small().color(ACCENT));
                    }
                });
            });
        });
        if let Some(err) = err {
            ui.colored_label(WARN, err);
        }
    });
    if frame.response.interact(Sense::click()).clicked() {
        select = true;
    }
    (select, action)
}

fn source_picker(ui: &mut egui::Ui, layer: &mut LayerState) {
    ui.horizontal(|ui| {
        ComboBox::from_id_salt(("source", layer.id)).width(150.0).selected_text(layer.source.label()).show_ui(ui, |ui| {
            if ui.selectable_label(layer.source == SourceSpec::None, "No source").clicked() {
                layer.source = SourceSpec::None;
            }
            ui.separator();
            for g in shaders::GENERATORS {
                let active = matches!(&layer.source, SourceSpec::Generator(id) if id == g.id);
                if ui.selectable_label(active, g.name).clicked() {
                    layer.set_generator(g.id);
                }
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        if ui.button(label(icon::FOLDER_OPEN, "Open")).on_hover_text("Image or movie file (or drag one onto the window)").clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Media", &crate::media::dialog_extensions()).pick_file()
        {
            let path = path.to_string_lossy().to_string();
            layer.source = crate::media::spec_for_path(&path);
            layer.name = layer.source.label();
        }
    });
}

fn param_slider(ui: &mut egui::Ui, values: &mut Vec<f32>, mods: &mut Vec<ParamMod>, i: usize, p: &shaders::Param) {
    if values.len() <= i {
        values.resize(i + 1, p.default);
    }
    ui.horizontal(|ui| {
        ui.add(Slider::new(&mut values[i], p.min..=p.max).text(p.name));
        let slot = mods.iter().position(|m| m.index == i);
        let mut m = slot.map(|k| mods[k].m);
        mod_button(ui, &mut m);
        match (slot, m) {
            (Some(k), Some(nm)) => mods[k].m = nm,
            (Some(k), None) => {
                mods.remove(k);
            }
            (None, Some(nm)) => mods.push(ParamMod { index: i, m: nm }),
            (None, None) => {}
        }
    });
}

/// The `~` button next to a control: opens the modulation editor; lit when a signal is attached.
fn mod_button(ui: &mut egui::Ui, m: &mut Option<Modulation>) {
    let color = if m.is_some() { ACCENT } else { Color32::GRAY };
    ui.menu_button(RichText::new(icon::WAVE_SINE).color(color), |ui| modulation_editor(ui, m))
        .response
        .on_hover_text("Drive this with audio or an LFO");
}

fn modulation_editor(ui: &mut egui::Ui, m: &mut Option<Modulation>) {
    ui.set_min_width(240.0);
    ui.label(RichText::new("Modulate with").strong());
    let current = m.map(|m| m.source);
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(current.is_none(), "Off").clicked() {
            *m = None;
        }
        for src in ModSource::ALL {
            if ui.selectable_label(current == Some(src), src.name()).clicked() {
                *m = Some(Modulation { source: src, ..m.unwrap_or_default() });
            }
        }
    });
    if let Some(mm) = m {
        ui.add(Slider::new(&mut mm.amount, -1.0..=1.0).text("Amount"));
        if mm.source.is_lfo() {
            ui.add(Slider::new(&mut mm.rate, 0.05..=10.0).text("Rate (Hz)").logarithmic(true));
        }
    }
}

/// An effect chain editor. Returns the id of an effect to append, if the user picked one.
fn effects_editor(ui: &mut egui::Ui, effects: &mut Vec<EffectState>) -> Option<&'static str> {
    let mut add: Option<&'static str> = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Effects").strong());
        ui.menu_button(label(icon::PLUS, "Add effect"), |ui| {
            for e in shaders::EFFECTS {
                if ui.button(e.name).clicked() {
                    add = Some(e.id);
                    ui.close();
                }
            }
        });
    });
    if effects.is_empty() {
        ui.weak("No effects yet. Effects run top to bottom.");
    }
    let mut remove = None;
    let mut swap = None;
    let count = effects.len();
    for (i, fx) in effects.iter_mut().enumerate() {
        let Some(desc) = shaders::effect(&fx.effect) else { continue };
        let fill = if fx.enabled { ui.visuals().faint_bg_color } else { ui.visuals().panel_fill };
        egui::Frame::group(ui.style()).fill(fill).corner_radius(6).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.checkbox(&mut fx.enabled, RichText::new(desc.name).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(icon::TRASH).on_hover_text("Remove effect").clicked() {
                        remove = Some(i);
                    }
                    if ui.small_button(icon::ARROW_DOWN).on_hover_text("Later in the chain").clicked() && i + 1 < count {
                        swap = Some((i, i + 1));
                    }
                    if ui.small_button(icon::ARROW_UP).on_hover_text("Earlier in the chain").clicked() && i > 0 {
                        swap = Some((i, i - 1));
                    }
                });
            });
            for (pi, p) in desc.params.iter().enumerate() {
                param_slider(ui, &mut fx.values, &mut fx.mods, pi, p);
            }
            ui.add(Slider::new(&mut fx.mix, 0.0..=1.0).text("Mix"));
        });
    }
    if let Some((a, b)) = swap {
        effects.swap(a, b);
    }
    if let Some(i) = remove {
        effects.remove(i);
    }
    add
}

fn inspector(ui: &mut egui::Ui, st: &mut State) {
    let aspect = st.aspect();
    let Some(sel) = st.selected.filter(|id| *id == MASTER_ID || st.comp.layers.iter().any(|l| l.id == *id)) else {
        ui.weak("Select a layer (or MASTER) to edit its source and effects.");
        return;
    };
    let mut add_effect: Option<&'static str> = None;
    if sel == MASTER_ID {
        let tex = st.texture;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
            let w = ui.available_width().min(360.0);
            thumbnail(ui, tex, egui::vec2(w, w / aspect), true);
            ui.label(RichText::new("MASTER").strong().color(ACCENT));
            ui.weak("These effects run over the whole mix, after every layer has been blended.");
            ui.separator();
            add_effect = effects_editor(ui, &mut st.comp.master_effects);
        });
    } else {
        let thumb = st.layer_textures.get(&sel).map(|(t, _)| *t);
        let info = st.media.video_info(sel);
        let layer = st.comp.layer_mut(sel).unwrap();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
            let w = ui.available_width().min(360.0);
            thumbnail(ui, thumb, egui::vec2(w, w / aspect), layer.enabled);
            ui.horizontal(|ui| {
                ui.toggle_value(&mut layer.enabled, RichText::new("ON").small().strong());
                ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(f32::INFINITY));
            });
            ui.separator();

            if let SourceSpec::Generator(id) = &layer.source
                && let Some(desc) = shaders::generator(id)
            {
                ui.label(RichText::new(format!("Generator · {}", desc.name)).strong());
                for (i, p) in desc.params.iter().enumerate() {
                    param_slider(ui, &mut layer.generator_values, &mut layer.generator_mods, i, p);
                }
                ui.separator();
            }
            if matches!(layer.source, SourceSpec::Movie { .. }) {
                ui.label(RichText::new("Playback").strong());
                if let Some(info) = &info {
                    ui.weak(info);
                }
                ui.horizontal(|ui| {
                    let text = if layer.playing { label(icon::PAUSE, "Pause") } else { label(icon::PLAY, "Play") };
                    ui.toggle_value(&mut layer.playing, text);
                    ui.checkbox(&mut layer.looping, "Loop");
                });
                ui.add(Slider::new(&mut layer.rate, 0.1..=4.0).text("Speed").logarithmic(true));
                ui.separator();
            }
            add_effect = effects_editor(ui, &mut layer.effects);
        });
    }
    if let Some(id) = add_effect {
        add_effect_to(st, sel, id);
    }
}

fn add_effect_to(st: &mut State, target: u64, effect: &str) {
    let Some(fx) = st.comp.new_effect(effect) else { return };
    if target == MASTER_ID {
        st.comp.master_effects.push(fx);
    } else if let Some(l) = st.comp.layer_mut(target) {
        l.effects.push(fx);
    }
}

fn library(ui: &mut egui::Ui, st: &mut State) {
    let has_selection = st.selected.is_some_and(|id| id == MASTER_ID || st.comp.layers.iter().any(|l| l.id == id));
    let on_master = st.selected == Some(MASTER_ID);
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.label(RichText::new("Generators").strong());
        ui.weak("Sets the selected layer's source (or adds a layer).");
        ui.horizontal_wrapped(|ui| {
            for g in shaders::GENERATORS {
                if ui.button(g.name).clicked() {
                    let id = match st.selected {
                        Some(id) if has_selection && !on_master => id,
                        _ => st.add_layer(),
                    };
                    if let Some(l) = st.comp.layer_mut(id) {
                        l.set_generator(g.id);
                        l.name = g.name.to_string();
                    }
                }
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new("Effects").strong());
        ui.weak(match (has_selection, on_master) {
            (true, true) => "Adds to the MASTER chain (over all layers).",
            (true, false) => "Adds to the selected layer's chain.",
            _ => "Select a layer or MASTER first.",
        });
        ui.horizontal_wrapped(|ui| {
            for e in shaders::EFFECTS {
                if ui.add_enabled(has_selection, egui::Button::new(e.name)).clicked()
                    && let Some(id) = st.selected
                {
                    add_effect_to(st, id, e.id);
                }
            }
        });
        ui.add_space(8.0);
        ui.weak("Drag images or movies onto the window to add them as layers.");
    });
}

fn presets(ui: &mut egui::Ui, st: &mut State) {
    st.presets.ensure_loaded();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut st.preset_name).hint_text("Preset name").desired_width(180.0));
        if ui.button(label(icon::FLOPPY_DISK, "Save current look")).clicked() {
            let json = st.comp.to_json();
            match st.presets.save(&st.preset_name, &json) {
                Ok(()) => {
                    st.preset_name.clear();
                    st.notice = None;
                }
                Err(e) => st.notice = Some(e),
            }
        }
    });
    if let Some(msg) = &st.notice {
        ui.colored_label(WARN, msg);
    }
    ui.separator();
    let mut load: Option<String> = None;
    let mut delete: Option<String> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.label(RichText::new("Built in").strong());
        for p in crate::presets::PresetStore::builtin() {
            ui.horizontal(|ui| {
                if ui.button("Load").clicked() {
                    load = Some(p.json.clone());
                }
                ui.label(&p.name);
            });
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Yours").strong());
        if st.presets.user.is_empty() {
            ui.weak("Nothing saved yet. Name the current look above and save it.");
        }
        for p in &st.presets.user {
            ui.horizontal(|ui| {
                if ui.button("Load").clicked() {
                    load = Some(p.json.clone());
                }
                ui.label(&p.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(icon::TRASH).on_hover_text("Delete preset").clicked() {
                        delete = Some(p.name.clone());
                    }
                });
            });
        }
    });
    if let Some(json) = load {
        match phosphor_engine::Composition::from_json(&json) {
            Ok(comp) => st.load_composition(comp),
            Err(e) => st.notice = Some(format!("Couldn't load preset: {e}")),
        }
    }
    if let Some(name) = delete {
        st.presets.delete(&name);
    }
}

fn audio(ui: &mut egui::Ui, st: &mut State) {
    let State { audio, notice, mod_inputs, .. } = st;
    ui.horizontal(|ui| {
        let current = audio.current().unwrap_or("Default input").to_string();
        ComboBox::from_id_salt("audio_device").width(220.0).selected_text(current).show_ui(ui, |ui| {
            let devices: Vec<String> = audio.devices().to_vec();
            if ui.selectable_label(audio.current().is_none(), "Default input").clicked()
                && let Err(e) = audio.open(None)
            {
                *notice = Some(e);
            }
            for d in devices {
                if ui.selectable_label(audio.current() == Some(d.as_str()), &d).clicked()
                    && let Err(e) = audio.open(Some(&d))
                {
                    *notice = Some(e);
                }
            }
        });
        if ui.button(icon::ARROWS_CLOCKWISE).on_hover_text("Rescan audio devices").clicked() {
            audio.refresh_devices();
        }
        let mut on = audio.is_open();
        let text = if on { label(icon::MICROPHONE, "Listening") } else { label(icon::MICROPHONE, "Off") };
        if ui.toggle_value(&mut on, text).clicked() {
            if on {
                if let Err(e) = audio.open(audio.current().map(str::to_string).as_deref()) {
                    *notice = Some(e);
                }
            } else {
                audio.close();
            }
        }
    });
    if let Some(msg) = notice {
        ui.colored_label(WARN, msg.as_str());
    }
    ui.add(Slider::new(&mut audio.gain, 0.1..=10.0).text("Gain").logarithmic(true));
    ui.add(Slider::new(&mut audio.release, 0.02..=0.6).text("Release (s)"));
    ui.separator();
    for (name, v) in [
        ("Level", mod_inputs.audio_level),
        ("Low", mod_inputs.audio_low),
        ("Mid", mod_inputs.audio_mid),
        ("High", mod_inputs.audio_high),
    ] {
        ui.add(egui::ProgressBar::new(v).text(name).fill(ACCENT.gamma_multiply(0.6)));
    }
    ui.add_space(4.0);
    let spectrum = audio.spectrum();
    if !spectrum.is_empty() {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 64.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 4.0, Color32::from_gray(12));
        let bw = rect.width() / spectrum.len() as f32;
        for (i, v) in spectrum.iter().enumerate() {
            let h = rect.height() * v.clamp(0.0, 1.0);
            let r = egui::Rect::from_min_max(
                egui::pos2(rect.left() + i as f32 * bw + 1.0, rect.bottom() - h),
                egui::pos2(rect.left() + (i + 1) as f32 * bw - 1.0, rect.bottom()),
            );
            p.rect_filled(r, 1.0, ACCENT.gamma_multiply(0.4 + 0.6 * v));
        }
    }
    ui.add_space(4.0);
    ui.weak(format!("Press {} next to any slider to drive it with these signals or an LFO.", icon::WAVE_SINE));
}

/// Draws every surface's outline, corner handles and a centre handle over `frame` (the rect the
/// output occupies on screen) and applies drags. Returns true if anything was touched.
pub fn mapping_handles(ui: &mut egui::Ui, frame: egui::Rect, mapping: &mut Mapping, selected: &mut Option<u64>, labels: bool) -> bool {
    let to_screen = |p: [f32; 2]| egui::pos2(frame.left() + p[0] * frame.width(), frame.top() + p[1] * frame.height());
    let mut touched = false;
    for s in &mut mapping.surfaces {
        let is_selected = *selected == Some(s.id);
        let base = if s.enabled { ACCENT } else { Color32::GRAY };
        let line = if is_selected { Stroke::new(2.0, base) } else { Stroke::new(1.0, base.gamma_multiply(0.55)) };
        let pts: Vec<egui::Pos2> = s.corners.iter().map(|c| to_screen(*c)).collect();
        ui.painter().add(egui::Shape::closed_line(pts.clone(), line));
        let id = ui.id().with(("surface", s.id));

        let c = to_screen(s.center());
        let r = ui.interact(egui::Rect::from_center_size(c, egui::vec2(16.0, 16.0)), id.with("move"), Sense::click_and_drag());
        if r.clicked() || r.drag_started() {
            *selected = Some(s.id);
            touched = true;
        }
        if r.dragged() {
            let d = r.drag_delta();
            s.translate(d.x / frame.width(), d.y / frame.height());
            touched = true;
        }
        let fill = if r.hovered() || r.dragged() { Color32::WHITE } else { base };
        ui.painter().rect_filled(egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)), 2.0, fill);
        r.on_hover_text("Drag to move the whole surface");

        for (i, corner) in s.corners.iter_mut().enumerate() {
            let p = to_screen(*corner);
            let r = ui.interact(egui::Rect::from_center_size(p, egui::vec2(20.0, 20.0)), id.with(i), Sense::click_and_drag());
            if r.clicked() || r.drag_started() {
                *selected = Some(s.id);
                touched = true;
            }
            if r.dragged() {
                let d = r.drag_delta();
                corner[0] += d.x / frame.width();
                corner[1] += d.y / frame.height();
                touched = true;
            }
            let active = r.hovered() || r.dragged();
            ui.painter().circle(p, if active { 7.0 } else { 5.0 }, if active { Color32::WHITE } else { base }, Stroke::new(1.5, Color32::BLACK));
        }
        if labels {
            ui.painter().text(pts[0] + egui::vec2(10.0, 8.0), egui::Align2::LEFT_TOP, &s.name, egui::FontId::proportional(12.0), Color32::WHITE);
        }
    }
    touched
}

fn mapping(ui: &mut egui::Ui, st: &mut State) {
    let aspect = st.aspect();
    let tex = st.texture;
    let mut edit_on_output = st.output.lock().unwrap().edit_mapping;
    let State { comp, selected_surface, output, .. } = st;
    ui.horizontal(|ui| {
        ui.toggle_value(&mut comp.mapping.enabled, label(icon::PROJECTOR_SCREEN, "Mapping on"))
            .on_hover_text("When on, the output shows only the surfaces below");
        ui.toggle_value(&mut comp.mapping.test_pattern, label(icon::GRID_FOUR, "Alignment grid"));
        if ui.button(label(icon::PLUS, "Surface")).clicked() {
            let id = comp.new_id();
            comp.mapping.add_surface(id);
            *selected_surface = Some(id);
            comp.mapping.enabled = true;
        }
        if ui.checkbox(&mut edit_on_output, "Edit on output (E)").on_hover_text("Show draggable handles in the output window").changed() {
            output.lock().unwrap().edit_mapping = edit_on_output;
        }
    });
    ui.weak("Drag corners to pin a surface onto the real object. Drag the centre to move it. Overlap feathered edges to blend projectors.");

    let avail = ui.available_size();
    let h = (avail.y * 0.5).min(avail.x / aspect).max(120.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail.x, h), Sense::hover());
    let frame = fit_rect(rect.shrink(4.0), aspect);
    ui.painter().rect_filled(frame, 0.0, Color32::BLACK);
    if let Some(t) = tex {
        paint_texture(ui.painter(), t, frame);
    }
    ui.painter().rect_stroke(frame, 0.0, Stroke::new(1.0, Color32::from_gray(70)), StrokeKind::Outside);
    mapping_handles(ui, frame, &mut comp.mapping, selected_surface, true);

    ui.add_space(4.0);
    let mut remove = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 180.0).max(80.0);
        if comp.mapping.surfaces.is_empty() {
            ui.weak("No surfaces yet. Add one, then drag its corners.");
        }
        for (i, s) in comp.mapping.surfaces.iter_mut().enumerate() {
            let is_selected = *selected_surface == Some(s.id);
            let frame = card_frame(ui, is_selected).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.toggle_value(&mut s.enabled, RichText::new("ON").small().strong());
                    ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(140.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::TRASH).on_hover_text("Delete surface").clicked() {
                            remove = Some(i);
                        }
                        if ui.small_button(icon::CORNERS_OUT).on_hover_text("Reset corners to the full output").clicked() {
                            s.corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                        }
                    });
                });
                ui.add(Slider::new(&mut s.opacity, 0.0..=1.0).text("Opacity"));
                ui.add(Slider::new(&mut s.feather, 0.0..=0.5).text("Soft edge"));
                ui.collapsing("Input region (part of the master shown)", |ui| {
                    ui.add(Slider::new(&mut s.input[0], 0.0..=1.0).text("X"));
                    ui.add(Slider::new(&mut s.input[1], 0.0..=1.0).text("Y"));
                    ui.add(Slider::new(&mut s.input[2], 0.05..=1.0).text("Width"));
                    ui.add(Slider::new(&mut s.input[3], 0.05..=1.0).text("Height"));
                });
            });
            if frame.response.interact(Sense::click()).clicked() {
                *selected_surface = Some(s.id);
            }
            ui.add_space(4.0);
        }
    });
    if let Some(i) = remove {
        let removed = comp.mapping.surfaces.remove(i);
        if *selected_surface == Some(removed.id) {
            *selected_surface = None;
        }
    }
}

fn control(ui: &mut egui::Ui, st: &mut State) {
    use crate::control::{all_targets, target_label};
    let State { control, comp, learn_pick, .. } = st;
    ui.label(RichText::new("MIDI").strong());
    if control.midi_ports.is_empty() {
        ui.weak("No MIDI inputs found. Plug in a controller and press Reconnect.");
    } else {
        ui.weak(format!("Listening on: {}", control.midi_ports.join(", ")));
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("OSC").strong());
        ui.add(egui::DragValue::new(&mut control.osc_port).range(1024..=65535).prefix("port "));
        if ui.button(label(icon::ARROWS_CLOCKWISE, "Reconnect")).clicked() {
            control.reconnect();
        }
    });
    ui.weak(&control.osc_status);
    ui.weak("TouchOSC: add a UDP connection to this address and port; Phosphor also appears in its Zeroconf list.");
    ui.separator();

    ui.label(RichText::new("Learn").strong());
    ui.horizontal(|ui| {
        let targets = all_targets(comp);
        let current = learn_pick.as_ref().map(|t| target_label(comp, t)).unwrap_or_else(|| "Pick what to control…".into());
        ComboBox::from_id_salt("learn_target").width(260.0).selected_text(current).show_ui(ui, |ui| {
            for (t, name) in targets {
                if ui.selectable_label(learn_pick.as_ref() == Some(&t), name).clicked() {
                    *learn_pick = Some(t);
                }
            }
        });
        let can_learn = learn_pick.is_some() && control.learning.is_none();
        if ui.add_enabled(can_learn, egui::Button::new(label(icon::BROADCAST, "Learn"))).clicked() {
            control.learning = learn_pick.clone();
        }
        if control.learning.is_some() && ui.button("Cancel").clicked() {
            control.learning = None;
        }
    });
    if let Some(t) = &control.learning {
        ui.colored_label(ACCENT, format!("Move a knob or send an OSC message to bind → {}", target_label(comp, t)));
    }
    match &control.last {
        Some(m) => ui.monospace(format!("last: {}  {:.3}", m.source.label(), m.value)),
        None => ui.weak("No controller messages received yet."),
    };
    ui.separator();

    ui.label(RichText::new("Mappings").strong());
    let mut remove = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if comp.control_maps.is_empty() {
            ui.weak("None yet. Pick a target above, press Learn, then move a control.");
        }
        for (i, m) in comp.control_maps.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button(icon::TRASH).clicked() {
                    remove = Some(i);
                }
                ui.monospace(m.source.label());
                ui.label("→");
                ui.label(target_label(comp, &m.target));
            });
        }
    });
    if let Some(i) = remove {
        comp.control_maps.remove(i);
    }
}

fn stream(ui: &mut egui::Ui, st: &mut State) {
    use crate::stream::Destination;
    let now = ui.ctx().input(|i| i.time);
    let (cw, ch) = (st.comp.width, st.comp.height);
    let State { stream, notice, .. } = st;
    let running = stream.running();
    ui.add_enabled_ui(!running, |ui| {
        let s = &mut stream.settings;
        ui.horizontal(|ui| {
            ui.label("Destination");
            ComboBox::from_id_salt("stream_dest").width(160.0).selected_text(s.destination.name()).show_ui(ui, |ui| {
                for d in Destination::ALL {
                    ui.selectable_value(&mut s.destination, d, d.name());
                }
            });
        });
        match s.destination {
            Destination::YouTube | Destination::Twitch => {
                ui.horizontal(|ui| {
                    ui.label("Stream key");
                    ui.add(egui::TextEdit::singleline(&mut s.stream_key).password(true).desired_width(260.0));
                });
            }
            Destination::CustomRtmp => {
                ui.horizontal(|ui| {
                    ui.label("RTMP URL");
                    ui.add(egui::TextEdit::singleline(&mut s.rtmp_url).desired_width(260.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Stream key");
                    ui.add(egui::TextEdit::singleline(&mut s.stream_key).password(true).desired_width(260.0));
                });
            }
            Destination::Srt => {
                ui.horizontal(|ui| {
                    ui.label("SRT URL");
                    ui.add(egui::TextEdit::singleline(&mut s.srt_url).desired_width(300.0));
                });
            }
            Destination::File => {
                ui.horizontal(|ui| {
                    ui.label("File");
                    ui.add(egui::TextEdit::singleline(&mut s.file_path).hint_text("…/recording.mp4 or .mov (ProRes)").desired_width(260.0));
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("Choose…").clicked()
                        && let Some(p) = rfd::FileDialog::new().add_filter("Video", &["mp4", "mov"]).set_file_name("Phosphor recording.mp4").save_file()
                    {
                        s.file_path = p.to_string_lossy().to_string();
                    }
                });
            }
        }
        ui.horizontal(|ui| {
            ui.label("Size");
            let current = if s.height == 0 { format!("Canvas ({cw}×{ch})") } else { format!("{}p", s.height) };
            ComboBox::from_id_salt("stream_size").selected_text(current).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.height, 0, format!("Canvas ({cw}×{ch})"));
                for h in [720, 1080, 1440, 2160] {
                    ui.selectable_value(&mut s.height, h, format!("{h}p"));
                }
            });
            ui.label("fps");
            ComboBox::from_id_salt("stream_fps").width(60.0).selected_text(s.fps.to_string()).show_ui(ui, |ui| {
                for f in [24, 25, 30, 50, 60] {
                    ui.selectable_value(&mut s.fps, f, f.to_string());
                }
            });
        });
        ui.add(Slider::new(&mut s.kbps, 1000..=30000).text("Bitrate (kbps)").logarithmic(true));
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if running {
            if ui.button(label(icon::STOP, "Stop")).clicked() {
                stream.stop();
            }
            ui.colored_label(ACCENT, format!("● LIVE {:02}:{:02}", (stream.elapsed(now) / 60.0) as u32, stream.elapsed(now) as u32 % 60));
        } else if ui.button(label(icon::BROADCAST, "Start")).clicked() {
            match stream.start(cw, ch, now) {
                Ok(()) => *notice = None,
                Err(e) => *notice = Some(e),
            }
        }
    });
    let (status, error, sent, dropped, log) = stream.status();
    if !status.is_empty() {
        ui.colored_label(if error { WARN } else { Color32::GRAY }, status);
    }
    if running || sent > 0 {
        ui.monospace(format!("frames sent {sent} · dropped {dropped}"));
    }
    for line in log {
        ui.colored_label(WARN, line);
    }
    if let Some(msg) = notice {
        ui.colored_label(WARN, msg.as_str());
    }
    ui.add_space(6.0);
    ui.weak("Hardware H.264 via VideoToolbox. No audio yet. OBS users: keep using OBS with a window/display capture for now; Syphon/NDI output is next.");
}

const RESOLUTIONS: &[(u32, u32, &str)] = &[
    (1280, 720, "1280×720 (720p)"),
    (1920, 1080, "1920×1080 (1080p)"),
    (2560, 1440, "2560×1440"),
    (3840, 2160, "3840×2160 (4K)"),
];

fn output(ui: &mut egui::Ui, st: &mut State) {
    egui::Grid::new("output_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Canvas");
        let current = format!("{}×{}", st.comp.width, st.comp.height);
        ComboBox::from_id_salt("resolution").selected_text(current).show_ui(ui, |ui| {
            for (w, h, label) in RESOLUTIONS {
                if ui.selectable_label(st.comp.width == *w && st.comp.height == *h, *label).clicked() {
                    st.comp.width = *w;
                    st.comp.height = *h;
                }
            }
        });
        ui.end_row();

        ui.label("Master");
        ui.add(Slider::new(&mut st.comp.master_fade, 0.0..=1.0).text("fade"));
        ui.end_row();

        ui.label("Output window");
        let text = if st.output_open() { "Close" } else { "Open" };
        if ui.button(text).clicked() {
            st.toggle_output();
        }
        ui.end_row();
    });
    ui.weak("In the output window: F = fullscreen, E = mapping handles, Esc = exit. Drag it to a projector first.");
    ui.separator();

    let s = st.stats.latest;
    egui::Grid::new("stats_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        let row = |ui: &mut egui::Ui, k: &str, v: String| {
            ui.label(k);
            ui.monospace(v);
            ui.end_row();
        };
        row(ui, "Frame rate", format!("{:.1} fps ({:.2} ms)", s.fps, s.frame_ms));
        row(ui, "Engine CPU / frame", format!("{:.3} ms", s.render_ms));
        row(ui, "Process CPU", format!("{:.1}% of one core", s.cpu_percent));
        row(ui, "Memory", format!("{:.0} MB", s.memory_mb));
        row(ui, "GPU passes / frame", s.passes.to_string());
    });
}

/// Save/Open project (native: file dialogs; browser: later via account sync).
pub fn project_buttons(ui: &mut egui::Ui, st: &mut State) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if ui.button(label(icon::FLOPPY_DISK, "Save")).clicked()
            && let Some(path) =
                rfd::FileDialog::new().add_filter("Phosphor project", &["phosphor"]).set_file_name("Untitled.phosphor").save_file()
            && let Err(e) = std::fs::write(&path, st.comp.to_json())
        {
            st.notice = Some(format!("Save failed: {e}"));
        }
        if ui.button(label(icon::FOLDER_OPEN, "Open")).clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Phosphor project", &["phosphor"]).pick_file()
            && let Ok(json) = std::fs::read_to_string(&path)
        {
            match phosphor_engine::Composition::from_json(&json) {
                Ok(comp) => st.load_composition(comp),
                Err(e) => st.notice = Some(format!("Open failed: {e}")),
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (ui, st);
}
