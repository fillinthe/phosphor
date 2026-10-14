//! Dockable panels. Drag a tab to split or stack it with another, or drag it out of the dock
//! area to float it in its own window.

use eframe::egui::{self, Color32, ComboBox, RichText, Sense, Slider, Stroke, StrokeKind};
use egui_phosphor::regular as icon;
use phosphor_engine::{BlendMode, ControlTarget, EffectState, FitMode, LayerState, MASTER_ID, Mapping, ModSource, Modulation, ParamMod, SourceSpec, TransitionKind, shaders};
use serde::{Deserialize, Serialize};

use crate::app::{State, Trigger};
use crate::theme::{ACCENT, WARN};

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
}

impl Tab {
    pub const ALL: [Tab; 15] = [
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

pub fn label(icon: &str, text: &str) -> String {
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
            ui.spacing_mut().slider_width = (ui.available_width() - 200.0).max(80.0);
            let r = ui.add(Slider::new(&mut st.comp.master_fade, 0.0..=1.0).show_value(false));
            mod_marker(ui, &r, st.comp.master_fade, 0.0, 1.0, st.comp.master_fade_mod.as_ref());
            mapped_badge(ui, &r, &ControlTarget::MasterFade);
            if control_menu(&r, Some(&ControlTarget::MasterFade), Some(&mut st.comp.master_fade_mod)) {
                st.comp.master_fade = 1.0;
            }
            ui.monospace(format!("{:3.0}%", st.comp.master_fade * 100.0));
            mod_button(ui, &mut st.comp.master_fade_mod);
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
    let resp = ui.interact(rect, egui::Id::new("preview_area"), Sense::click());
    resp.context_menu(|ui| {
        let open = st.output_open();
        if ui.button(label(if open { icon::STOP } else { icon::PLAY }, if open { "Close output" } else { "Open output" })).clicked() {
            st.toggle_output();
            ui.close();
        }
        ui.menu_button(label(icon::PROJECTOR_SCREEN, "Send output to"), |ui| {
            if st.displays.is_empty() {
                st.displays = crate::screens::displays();
            }
            for d in st.displays.clone() {
                if ui.button(d.label()).clicked() {
                    st.send_output_to(d);
                    ui.close();
                }
            }
        });
        if ui.button(label(icon::MONITOR, "New monitor")).clicked() {
            st.new_monitor = Some(MonitorSource::Mix);
            ui.close();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if ui.button(label(icon::CAMERA, "Save snapshot…")).clicked() {
            st.snapshot_requested = true;
            ui.close();
        }
        ui.separator();
        if ui.button(label(icon::BROADCAST, "Learn master fade")).clicked() {
            request(ui, LEARN_REQUEST, ControlTarget::MasterFade);
            ui.close();
        }
    });
    let s = st.stats.latest;
    let mut info = format!("{}×{}  {:.0} fps", st.comp.width, st.comp.height, s.fps);
    if st.comp.outputs.iter().any(|o| o.mapping.active()) {
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
    BrowseSources,
    BrowseEffects,
    Monitor,
    Up,
    Down,
    Duplicate,
    Remove,
    ClearSource,
}

fn layers(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        if ui.button(label(icon::PLUS, "Add Layer")).clicked() {
            st.add_layer();
        }
        ui.weak("Top of the list is the top of the stack · drag a thumbnail to reorder · right-click for more");
    });
    ui.add_space(2.0);
    let aspect = st.aspect();
    let output_tex = st.texture;
    let any_solo = st.comp.layers.iter().any(|l| l.enabled && l.solo);
    let State { comp, selected, media, layer_textures, .. } = st;
    let count = comp.layers.len();
    let mut action = None;
    let mut drop: Option<(u64, u64)> = None;
    let mut media_drop: Option<(usize, usize)> = None;
    let mut palette_drop: Option<(u64, crate::explorer::PaletteDrag)> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let (pick, dropped) = master_card(ui, output_tex, aspect, *selected == Some(MASTER_ID), &comp.master_effects, &mut comp.master_fx_bypass);
        if pick {
            *selected = Some(MASTER_ID);
        }
        if let Some(d) = dropped {
            palette_drop = Some((MASTER_ID, d));
        }
        ui.add_space(4.0);
        for idx in (0..count).rev() {
            let layer = &mut comp.layers[idx];
            let thumb = layer_textures.get(&layer.id).map(|(t, _)| *t);
            let err = media.errors.get(&layer.id).cloned();
            let is_selected = *selected == Some(layer.id);
            let muted = any_solo && !layer.solo;
            let out = layer_card(ui, layer, thumb, aspect, is_selected, muted, err.as_deref());
            if out.select {
                *selected = Some(layer.id);
            }
            if let Some(a) = out.action {
                action = Some((idx, a));
            }
            if let Some(from) = out.dropped {
                drop = Some((from, layer.id));
            }
            if let Some(m) = out.dropped_media {
                media_drop = Some((idx, m));
            }
            if let Some(d) = out.dropped_palette {
                palette_drop = Some((layer.id, d));
            }
            ui.add_space(4.0);
        }
    });
    if let Some((idx, m)) = media_drop
        && let Some(item) = comp.media_bin.get(m).cloned()
        && let Some(layer) = comp.layers.get_mut(idx)
    {
        layer.source = crate::media::spec_for_path(&item.path);
        layer.name = item.name;
    }
    if let Some((from, onto)) = drop
        && from != onto
        && let Some(a) = comp.layers.iter().position(|l| l.id == from)
        && let Some(b) = comp.layers.iter().position(|l| l.id == onto)
    {
        let moved = comp.layers.remove(a);
        comp.layers.insert(b, moved);
    }
    match action {
        Some((i, LayerAction::Up)) if i + 1 < count => comp.layers.swap(i, i + 1),
        Some((i, LayerAction::Down)) if i > 0 => comp.layers.swap(i, i - 1),
        Some((i, LayerAction::Duplicate)) => {
            let id = comp.layers[i].id;
            duplicate_layer(st, id);
        }
        Some((i, LayerAction::Remove)) => {
            let id = comp.layers[i].id;
            remove_layer(st, id);
        }
        Some((i, LayerAction::BrowseSources)) => {
            let id = comp.layers[i].id;
            st.fx_browser = Some(crate::explorer::FxBrowser::new(id, true, None));
        }
        Some((i, LayerAction::BrowseEffects)) => {
            let id = comp.layers[i].id;
            st.fx_browser = Some(crate::explorer::FxBrowser::new(id, false, None));
        }
        Some((i, LayerAction::Monitor)) => {
            let id = comp.layers[i].id;
            st.new_monitor = Some(MonitorSource::Layer(id));
        }
        Some((i, LayerAction::ClearSource)) => {
            comp.layers[i].source = SourceSpec::None;
        }
        _ => {}
    }
    if let Some((target, d)) = palette_drop {
        crate::explorer::apply_palette(st, d, Some(target));
    }
}

pub fn duplicate_layer(st: &mut State, id: u64) {
    let Some(i) = st.comp.layers.iter().position(|l| l.id == id) else { return };
    let mut copy = st.comp.layers[i].clone();
    copy.id = st.comp.new_id();
    copy.name = format!("{} copy", copy.name);
    for fx in &mut copy.effects {
        fx.id = st.comp.new_id();
    }
    st.selected = Some(copy.id);
    st.comp.layers.insert(i + 1, copy);
}

pub fn remove_layer(st: &mut State, id: u64) {
    let Some(i) = st.comp.layers.iter().position(|l| l.id == id) else { return };
    st.comp.layers.remove(i);
    if st.selected == Some(id) {
        st.selected = st.comp.layers.get(i.saturating_sub(1)).or(st.comp.layers.last()).map(|l| l.id);
    }
}

struct CardOut {
    select: bool,
    action: Option<LayerAction>,
    /// Id of a layer that was dropped onto this card.
    dropped: Option<u64>,
    /// Media bin index dropped onto this card.
    dropped_media: Option<usize>,
    /// Effect or generator dragged from the Effects explorer.
    dropped_palette: Option<crate::explorer::PaletteDrag>,
}

/// Drag payload from the media bin.
#[derive(Clone, Copy)]
pub struct MediaDrag(pub usize);

/// A click / right-click area *behind* a card, registered before the card's contents so its own
/// buttons, menus and sliders stay clickable (egui gives clicks to the widget registered last).
/// Uses the card's rect from the previous frame; `None` on the very first frame.
fn card_background(ui: &mut egui::Ui, id: egui::Id) -> Option<egui::Response> {
    let rect = ui.ctx().data(|d| d.get_temp::<egui::Rect>(id))?;
    Some(ui.interact(rect, id.with("bg"), Sense::click()))
}

fn remember_card(ui: &egui::Ui, id: egui::Id, rect: egui::Rect) {
    ui.ctx().data_mut(|d| d.insert_temp(id, rect));
}

fn card_frame(ui: &egui::Ui, is_selected: bool) -> egui::Frame {
    let stroke = if is_selected { Stroke::new(1.5, ACCENT) } else { ui.visuals().widgets.noninteractive.bg_stroke };
    let fill = if is_selected { Color32::from_rgb(24, 34, 30) } else { ui.visuals().faint_bg_color };
    egui::Frame::new().fill(fill).stroke(stroke).corner_radius(6).inner_margin(egui::Margin::same(8))
}

/// The pinned card for the whole mix: shows the output and the master effect count.
fn master_card(
    ui: &mut egui::Ui,
    tex: Option<egui::TextureId>,
    aspect: f32,
    is_selected: bool,
    effects: &[EffectState],
    bypass: &mut bool,
) -> (bool, Option<crate::explorer::PaletteDrag>) {
    let mut select = false;
    let card_id = egui::Id::new("master_card");
    let bg = card_background(ui, card_id);
    let frame = card_frame(ui, is_selected).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if thumbnail(ui, tex, egui::vec2(112.0, 112.0 / aspect), true).clicked() {
                select = true;
            }
            ui.vertical(|ui| {
                if ui.selectable_label(is_selected, RichText::new("MASTER").strong().size(12.5).color(ACCENT)).clicked() {
                    select = true;
                }
                ui.weak("Effects here run over all layers");
                let fx = effects.iter().filter(|e| e.enabled).count();
                if fx > 0 {
                    let mut active = !*bypass;
                    let color = if active { ACCENT } else { Color32::GRAY };
                    if ui.toggle_value(&mut active, RichText::new(format!("{fx} fx")).small().color(color)).on_hover_text("Click to bypass the master effects").changed() {
                        *bypass = !active;
                    }
                }
            });
        });
    });
    remember_card(ui, card_id, frame.response.rect);
    if bg.is_some_and(|b| b.clicked()) {
        select = true;
    }
    // Only effects make sense on MASTER.
    let dropped = frame.response.dnd_release_payload::<crate::explorer::PaletteDrag>().map(|p| *p).filter(|d| d.effect.is_some());
    if frame.response.dnd_hover_payload::<crate::explorer::PaletteDrag>().is_some_and(|d| d.effect.is_some()) {
        ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
    }
    (select, dropped)
}

fn layer_card(
    ui: &mut egui::Ui,
    layer: &mut LayerState,
    thumb: Option<egui::TextureId>,
    aspect: f32,
    is_selected: bool,
    muted: bool,
    err: Option<&str>,
) -> CardOut {
    let mut select = false;
    let mut action = None;
    let card_id = egui::Id::new(("layer_card", layer.id));
    let bg = card_background(ui, card_id);
    let frame = card_frame(ui, is_selected).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let r = thumbnail(ui, thumb, egui::vec2(112.0, 112.0 / aspect), layer.enabled && !muted)
                .on_hover_text("Click to select · double-click to toggle · drag to reorder");
            r.dnd_set_drag_payload(layer.id);
            if r.clicked() {
                select = true;
            }
            if r.double_clicked() {
                layer.enabled = !layer.enabled;
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let on = ui.toggle_value(&mut layer.enabled, RichText::new("ON").small().strong()).on_hover_text("Layer on/off (Space) · right-click to Learn");
                    mapped_badge(ui, &on, &ControlTarget::LayerEnabled(layer.id));
                    on.context_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(layer.id)));
                    let solo_color = if layer.solo { WARN } else { Color32::GRAY };
                    ui.toggle_value(&mut layer.solo, RichText::new("S").small().strong().color(solo_color))
                        .on_hover_text("Solo: only soloed layers reach the mix");
                    let name = RichText::new(&layer.name).strong().size(12.5);
                    let name = if muted { name.color(Color32::GRAY) } else { name };
                    if ui.selectable_label(is_selected, name).clicked() {
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
                if source_picker(ui, layer) {
                    action = Some(LayerAction::BrowseSources);
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width = (ui.available_width() - 100.0).max(60.0);
                    let r = ui.add(Slider::new(&mut layer.opacity, 0.0..=1.0).show_value(false)).on_hover_text("Opacity");
                    mod_marker(ui, &r, layer.opacity, 0.0, 1.0, layer.opacity_mod.as_ref());
                    mapped_badge(ui, &r, &ControlTarget::LayerOpacity(layer.id));
                    if control_menu(&r, Some(&ControlTarget::LayerOpacity(layer.id)), Some(&mut layer.opacity_mod)) {
                        layer.opacity = 1.0;
                    }
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
                        let mut active = !layer.fx_bypass;
                        let color = if active { ACCENT } else { Color32::GRAY };
                        if ui.toggle_value(&mut active, RichText::new(format!("{fx} fx")).small().color(color)).on_hover_text("Click to bypass this layer's effects").changed() {
                            layer.fx_bypass = !active;
                        }
                    }
                });
            });
        });
        if let Some(err) = err {
            ui.colored_label(WARN, err);
        }
    });
    remember_card(ui, card_id, frame.response.rect);
    let resp = bg.unwrap_or_else(|| frame.response.clone());
    if resp.clicked() {
        select = true;
    }
    resp.context_menu(|ui| {
        if ui.button(label(icon::SPARKLE, "Browse sources…")).clicked() {
            action = Some(LayerAction::BrowseSources);
            ui.close();
        }
        if ui.button(label(icon::MAGIC_WAND, "Add effect…")).clicked() {
            action = Some(LayerAction::BrowseEffects);
            ui.close();
        }
        if ui.button(label(icon::MONITOR, "Show in a new monitor")).clicked() {
            action = Some(LayerAction::Monitor);
            ui.close();
        }
        ui.separator();
        learn_items(ui, &ControlTarget::LayerEnabled(layer.id));
        if ui.button(label(icon::BROADCAST, "Learn opacity")).clicked() {
            request(ui, LEARN_REQUEST, ControlTarget::LayerOpacity(layer.id));
            ui.close();
        }
        ui.separator();
        if ui.button(label(icon::COPY, "Duplicate (⌘D)")).clicked() {
            action = Some(LayerAction::Duplicate);
            ui.close();
        }
        if ui.button(label(icon::ARROW_UP, "Move up")).clicked() {
            action = Some(LayerAction::Up);
            ui.close();
        }
        if ui.button(label(icon::ARROW_DOWN, "Move down")).clicked() {
            action = Some(LayerAction::Down);
            ui.close();
        }
        if ui.button("Clear source").clicked() {
            action = Some(LayerAction::ClearSource);
            ui.close();
        }
        ui.separator();
        if ui.button(label(icon::TRASH, "Delete (⌫)")).clicked() {
            action = Some(LayerAction::Remove);
            ui.close();
        }
    });
    // Drop target for a dragged layer or a media bin item.
    let dropped = frame.response.dnd_release_payload::<u64>().map(|p| *p);
    let dropped_media = frame.response.dnd_release_payload::<MediaDrag>().map(|p| p.0);
    let dropped_palette = frame.response.dnd_release_payload::<crate::explorer::PaletteDrag>().map(|p| *p);
    let hovering = frame.response.dnd_hover_payload::<u64>().is_some_and(|p| *p != layer.id)
        || frame.response.dnd_hover_payload::<MediaDrag>().is_some()
        || frame.response.dnd_hover_payload::<crate::explorer::PaletteDrag>().is_some();
    if hovering {
        ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
    }
    CardOut { select, action, dropped, dropped_media, dropped_palette }
}

/// Returns true when the user asks for the source browser.
fn source_picker(ui: &mut egui::Ui, layer: &mut LayerState) -> bool {
    let mut browse = false;
    ui.horizontal(|ui| {
        if ui.button(icon::SPARKLE).on_hover_text("Browse sources with previews").clicked() {
            browse = true;
        }
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
    browse
}

fn param_slider(ui: &mut egui::Ui, values: &mut Vec<f32>, mods: &mut Vec<ParamMod>, i: usize, p: &shaders::Param, target: Option<ControlTarget>) {
    if values.len() <= i {
        values.resize(i + 1, p.default);
    }
    ui.horizontal(|ui| {
        let r = ui.add(Slider::new(&mut values[i], p.min..=p.max).text(p.name)).on_hover_text("Double-click to reset · right-click for Learn / Modulate");
        let slot = mods.iter().position(|m| m.index == i);
        mod_marker(ui, &r, values[i], p.min, p.max, slot.map(|k| &mods[k].m));
        if let Some(t) = &target {
            mapped_badge(ui, &r, t);
        }
        let mut m = slot.map(|k| mods[k].m);
        if r.double_clicked() | control_menu(&r, target.as_ref(), Some(&mut m)) {
            values[i] = p.default;
        }
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

pub const LEARN_REQUEST: &str = "phosphor_learn_request";
pub const UNLEARN_REQUEST: &str = "phosphor_unlearn_request";
pub const MAPPED_TARGETS: &str = "phosphor_mapped_targets";

fn request(ui: &egui::Ui, key: &str, target: ControlTarget) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(key), target));
}

fn is_mapped(ui: &egui::Ui, target: &ControlTarget) -> bool {
    ui.ctx()
        .data(|d| d.get_temp::<std::sync::Arc<Vec<ControlTarget>>>(egui::Id::new(MAPPED_TARGETS)))
        .is_some_and(|v| v.contains(target))
}

/// Learn / Forget entries for any control's right-click menu.
fn learn_items(ui: &mut egui::Ui, target: &ControlTarget) {
    if ui.button(label(icon::BROADCAST, "Learn MIDI / OSC / key")).on_hover_text("Then move a knob, send OSC, or press a key").clicked() {
        request(ui, LEARN_REQUEST, target.clone());
        ui.close();
    }
    if is_mapped(ui, target) && ui.button(label(icon::X, "Forget mapping")).clicked() {
        request(ui, UNLEARN_REQUEST, target.clone());
        ui.close();
    }
}

/// Small badge on a control that has a MIDI/OSC/key mapping.
fn mapped_badge(ui: &egui::Ui, resp: &egui::Response, target: &ControlTarget) {
    if is_mapped(ui, target) {
        let p = resp.rect.left_top() + egui::vec2(1.0, 1.0);
        ui.painter().circle_filled(p, 3.0, Color32::from_rgb(255, 196, 70));
    }
}

/// The standard right-click menu for a slider: Learn, Modulate, Reset. Returns true on Reset.
fn control_menu(resp: &egui::Response, target: Option<&ControlTarget>, m: Option<&mut Option<Modulation>>) -> bool {
    let mut reset = false;
    resp.context_menu(|ui| {
        if let Some(t) = target {
            learn_items(ui, t);
        }
        if let Some(m) = m {
            ui.menu_button(label(icon::WAVE_SINE, "Modulate…"), |ui| modulation_editor(ui, m));
        }
        if ui.button(label(icon::ARROW_COUNTER_CLOCKWISE, "Reset to default")).clicked() {
            reset = true;
            ui.close();
        }
    });
    reset
}

fn mod_inputs(ui: &egui::Ui) -> phosphor_engine::ModInputs {
    ui.ctx().data(|d| d.get_temp(egui::Id::new("phosphor_mod_inputs"))).unwrap_or_default()
}

/// Draws where a modulated slider actually is right now (a green tick on the rail).
fn mod_marker(ui: &egui::Ui, slider: &egui::Response, base: f32, min: f32, max: f32, m: Option<&Modulation>) {
    let Some(m) = m else { return };
    let live = phosphor_engine::control::modulate(base, min, max, Some(m), &mod_inputs(ui));
    let t = if (max - min).abs() > f32::EPSILON { ((live - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
    let rail = ui.spacing().slider_width;
    let r = slider.rect;
    let pad = r.height() * 0.5;
    let x = r.left() + pad + t * (rail - 2.0 * pad).max(1.0);
    let y = r.center().y;
    ui.painter().line_segment([egui::pos2(x, y - 6.0), egui::pos2(x, y + 6.0)], Stroke::new(2.0, ACCENT));
    ui.painter().circle_filled(egui::pos2(x, y), 3.0, ACCENT);
}

const SYNC_CHOICES: &[(f32, &str)] =
    &[(0.25, "1/4 beat"), (0.5, "1/2 beat"), (1.0, "1 beat"), (2.0, "2 beats"), (4.0, "1 bar"), (8.0, "2 bars"), (16.0, "4 bars"), (32.0, "8 bars")];

fn modulation_editor(ui: &mut egui::Ui, m: &mut Option<Modulation>) {
    ui.set_min_width(320.0);
    let inputs = mod_inputs(ui);
    let current = m.map(|m| m.source);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Drive this control with").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.selectable_label(current.is_none(), "Off").clicked() {
                *m = None;
            }
        });
    });
    let mut chips = |ui: &mut egui::Ui, title: &str, filter: fn(ModSource) -> bool| {
        ui.label(RichText::new(title).small().color(Color32::GRAY));
        ui.horizontal_wrapped(|ui| {
            for src in ModSource::ALL.into_iter().filter(|s| filter(*s)) {
                if ui.selectable_label(current == Some(src), src.name()).clicked() {
                    *m = Some(Modulation { source: src, ..m.unwrap_or_default() });
                }
            }
        });
    };
    chips(ui, "AUDIO", |s| s.is_audio());
    chips(ui, "TIME & TEMPO", |s| !s.is_audio());
    let Some(mm) = m else { return };
    ui.separator();
    if mm.source.is_lfo() {
        ui.horizontal(|ui| {
            ui.checkbox(&mut mm.sync, "Sync to tempo");
            if mm.sync {
                let current = SYNC_CHOICES.iter().find(|(v, _)| (*v - mm.rate).abs() < 1e-3).map_or("custom", |(_, n)| *n);
                ComboBox::from_id_salt("sync_rate").width(100.0).selected_text(format!("every {current}")).show_ui(ui, |ui| {
                    for (v, name) in SYNC_CHOICES {
                        ui.selectable_value(&mut mm.rate, *v, *name);
                    }
                });
            } else {
                ui.add(Slider::new(&mut mm.rate, 0.01..=10.0).text("Hz").logarithmic(true));
            }
        });
    }
    if mm.source == ModSource::Steps {
        ui.label(RichText::new("Steps (drag to set)").small().color(Color32::GRAY));
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 56.0), Sense::click_and_drag());
        let w = rect.width() / 8.0;
        let cycles = if mm.sync { inputs.beats / mm.rate.max(1.0 / 16.0) } else { inputs.time * mm.rate };
        let playing = ((cycles.rem_euclid(1.0)) * 8.0) as usize;
        if let Some(pos) = resp.interact_pointer_pos() {
            let i = (((pos.x - rect.left()) / w) as usize).min(7);
            mm.steps[i] = (1.0 - (pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        }
        for (i, v) in mm.steps.iter().enumerate() {
            let x0 = rect.left() + i as f32 * w + 2.0;
            let bar = egui::Rect::from_min_max(egui::pos2(x0, rect.bottom() - v * rect.height()), egui::pos2(x0 + w - 4.0, rect.bottom()));
            ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x0 + w - 4.0, rect.bottom())), 2.0, Color32::from_gray(26));
            ui.painter().rect_filled(bar, 2.0, if i == playing { ACCENT } else { ACCENT.gamma_multiply(0.45) });
        }
    }
    ui.horizontal(|ui| {
        ui.selectable_value(&mut mm.range, false, "Add to slider").on_hover_text("The signal pushes the slider up (or down) by Amount");
        ui.selectable_value(&mut mm.range, true, "Sweep a range").on_hover_text("The signal moves the control between From and To; the slider is ignored");
        ui.checkbox(&mut mm.invert, "Invert");
    });
    if mm.range {
        ui.add(Slider::new(&mut mm.min, 0.0..=1.0).text("From"));
        ui.add(Slider::new(&mut mm.max, 0.0..=1.0).text("To"));
    } else {
        ui.add(Slider::new(&mut mm.amount, -1.0..=1.0).text("Amount"));
    }
    let v = phosphor_engine::control::source_value(mm, &inputs);
    ui.add(egui::ProgressBar::new(v).text(format!("live {v:.2}")).fill(ACCENT.gamma_multiply(0.6)));
}

pub enum FxAction {
    /// Copy the effect at this index right after itself (needs a fresh id from the composition).
    Duplicate(usize),
    Browse(Option<egui::Pos2>),
    CopyChain,
    PasteChain,
}

/// Drag payload for reordering effects (distinct from the layer payload).
#[derive(Clone, Copy)]
struct FxDrag(u64);

/// An effect chain editor: collapsible cards, drag to reorder, copy/paste whole chains.
fn effects_editor(ui: &mut egui::Ui, effects: &mut Vec<EffectState>, clipboard_len: usize) -> Option<FxAction> {
    let mut action: Option<FxAction> = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Effects").strong());
        let add = ui.button(label(icon::PLUS, "Add effect")).on_hover_text("Browse effects with previews");
        if add.clicked() {
            action = Some(FxAction::Browse(Some(add.rect.left_bottom())));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(clipboard_len > 0, egui::Button::new(label(icon::CLIPBOARD_TEXT, "Paste"))).on_hover_text("Append the copied chain").clicked() {
                action = Some(FxAction::PasteChain);
            }
            if ui.add_enabled(!effects.is_empty(), egui::Button::new(label(icon::COPY, "Copy"))).on_hover_text("Copy this whole chain").clicked() {
                action = Some(FxAction::CopyChain);
            }
        });
    });
    if effects.is_empty() {
        ui.weak("No effects yet. Effects run top to bottom; drag the handle to reorder.");
    }
    let mut remove = None;
    let mut swap = None;
    let mut duplicate: Option<usize> = None;
    let mut drop: Option<(u64, u64)> = None;
    let count = effects.len();
    for (i, fx) in effects.iter_mut().enumerate() {
        let Some(desc) = shaders::effect(&fx.effect) else { continue };
        let fill = if fx.enabled { ui.visuals().faint_bg_color } else { ui.visuals().panel_fill };
        let frame = egui::Frame::group(ui.style()).fill(fill).corner_radius(6).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let id = ui.make_persistent_id(("fx", fx.id));
            let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
            let header = state.show_header(ui, |ui| {
                let handle = ui.add(egui::Label::new(RichText::new(icon::DOTS_SIX_VERTICAL).color(Color32::GRAY)).sense(Sense::drag()));
                handle.dnd_set_drag_payload(FxDrag(fx.id));
                handle.on_hover_text("Drag to reorder");
                let on = ui.checkbox(&mut fx.enabled, RichText::new(desc.name).strong());
                mapped_badge(ui, &on, &ControlTarget::EffectEnabled(fx.id));
                on.context_menu(|ui| {
                    learn_items(ui, &ControlTarget::EffectEnabled(fx.id));
                    ui.separator();
                    if ui.button(label(icon::ARROW_COUNTER_CLOCKWISE, "Reset all controls")).clicked() {
                        fx.values = desc.defaults();
                        fx.mods.clear();
                        fx.mix = 1.0;
                        ui.close();
                    }
                    if ui.button(label(icon::COPY, "Duplicate")).clicked() {
                        duplicate = Some(i);
                        ui.close();
                    }
                    if ui.button(label(icon::ARROW_UP, "Earlier in the chain")).clicked() && i > 0 {
                        swap = Some((i, i - 1));
                        ui.close();
                    }
                    if ui.button(label(icon::ARROW_DOWN, "Later in the chain")).clicked() && i + 1 < count {
                        swap = Some((i, i + 1));
                        ui.close();
                    }
                    if ui.button(label(icon::TRASH, "Remove")).clicked() {
                        remove = Some(i);
                        ui.close();
                    }
                });
                if fx.mix < 0.999 {
                    ui.weak(format!("{:.0}%", fx.mix * 100.0));
                }
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
            header.body(|ui| {
                for (pi, p) in desc.params.iter().enumerate() {
                    param_slider(ui, &mut fx.values, &mut fx.mods, pi, p, Some(ControlTarget::EffectParam { effect: fx.id, index: pi }));
                }
                let mix = ui.add(Slider::new(&mut fx.mix, 0.0..=1.0).text("Mix"));
                mapped_badge(ui, &mix, &ControlTarget::EffectMix(fx.id));
                if control_menu(&mix, Some(&ControlTarget::EffectMix(fx.id)), None) {
                    fx.mix = 1.0;
                }
            });
        });
        if let Some(p) = frame.response.dnd_release_payload::<FxDrag>() {
            drop = Some((p.0, fx.id));
        }
        if frame.response.dnd_hover_payload::<FxDrag>().is_some_and(|p| p.0 != fx.id) {
            ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
        }
    }
    if let Some((from, onto)) = drop
        && from != onto
        && let Some(a) = effects.iter().position(|e| e.id == from)
        && let Some(b) = effects.iter().position(|e| e.id == onto)
    {
        let moved = effects.remove(a);
        effects.insert(b, moved);
    }
    if let Some((a, b)) = swap {
        effects.swap(a, b);
    }
    if let Some(i) = remove {
        effects.remove(i);
    }
    if let Some(i) = duplicate {
        action = Some(FxAction::Duplicate(i));
    }
    action
}

fn apply_fx_action(st: &mut State, target: u64, action: FxAction) {
    match action {
        FxAction::Browse(pos) => st.fx_browser = Some(crate::explorer::FxBrowser::new(target, false, pos)),
        FxAction::Duplicate(i) => {
            let id = st.comp.new_id();
            let chain = if target == MASTER_ID { Some(&mut st.comp.master_effects) } else { st.comp.layer_mut(target).map(|l| &mut l.effects) };
            if let Some(chain) = chain
                && let Some(fx) = chain.get(i).cloned()
            {
                chain.insert(i + 1, phosphor_engine::EffectState { id, ..fx });
            }
        }
        FxAction::CopyChain => {
            let chain = if target == MASTER_ID {
                st.comp.master_effects.clone()
            } else {
                st.comp.layers.iter().find(|l| l.id == target).map(|l| l.effects.clone()).unwrap_or_default()
            };
            st.fx_clipboard = chain;
        }
        FxAction::PasteChain => {
            for mut fx in st.fx_clipboard.clone() {
                fx.id = st.comp.new_id();
                if target == MASTER_ID {
                    st.comp.master_effects.push(fx);
                } else if let Some(l) = st.comp.layer_mut(target) {
                    l.effects.push(fx);
                }
            }
        }
    }
}

fn inspector(ui: &mut egui::Ui, st: &mut State) {
    let aspect = st.aspect();
    let Some(sel) = st.selected.filter(|id| *id == MASTER_ID || st.comp.layers.iter().any(|l| l.id == *id)) else {
        ui.weak("Select a layer (or MASTER) to edit its source and effects.");
        return;
    };
    let clipboard_len = st.fx_clipboard.len();
    let mut fx_action: Option<FxAction> = None;
    if sel == MASTER_ID {
        let tex = st.texture;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
            let w = ui.available_width().min(360.0);
            thumbnail(ui, tex, egui::vec2(w, w / aspect), true);
            ui.label(RichText::new("MASTER").strong().color(ACCENT));
            ui.weak("These effects run over the whole mix, after every layer has been blended.");
            ui.separator();
            fx_action = effects_editor(ui, &mut st.comp.master_effects, clipboard_len);
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
                    param_slider(ui, &mut layer.generator_values, &mut layer.generator_mods, i, p, Some(ControlTarget::GeneratorParam { layer: layer.id, index: i }));
                }
                ui.separator();
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Transition").strong());
                ComboBox::from_id_salt(("trans", layer.id)).width(120.0).selected_text(layer.transition_kind.name()).show_ui(ui, |ui| {
                    for k in TransitionKind::ALL {
                        ui.selectable_value(&mut layer.transition_kind, k, k.name());
                    }
                });
                ui.add(Slider::new(&mut layer.transition_secs, 0.0..=5.0).text("s").fixed_decimals(2));
            });
            ui.separator();
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
            fx_action = effects_editor(ui, &mut layer.effects, clipboard_len);
        });
    }
    if let Some(action) = fx_action {
        apply_fx_action(st, sel, action);
    }
}

pub fn add_effect_to(st: &mut State, target: u64, effect: &str) {
    let Some(fx) = st.comp.new_effect(effect) else { return };
    st.explorer.recent.retain(|r| r != effect);
    st.explorer.recent.insert(0, effect.to_string());
    st.explorer.recent.truncate(12);
    if target == MASTER_ID {
        st.comp.master_effects.push(fx);
    } else if let Some(l) = st.comp.layer_mut(target) {
        l.effects.push(fx);
    }
}

/// Adds every supported file under `dir` (two levels deep, capped) to the bin.
#[cfg(not(target_arch = "wasm32"))]
fn import_folder(comp: &mut phosphor_engine::Composition, dir: &std::path::Path, depth: u32, budget: &mut usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if *budget == 0 {
            return;
        }
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if depth > 0 && !name.starts_with('.') {
                import_folder(comp, &p, depth - 1, budget);
            }
        } else if crate::media::is_supported(&name) && comp.add_media(&name, &p.to_string_lossy()) {
            *budget -= 1;
        }
    }
}

fn media(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if ui.button(label(icon::FOLDER_OPEN, "Import files…")).clicked()
                && let Some(paths) = rfd::FileDialog::new().add_filter("Media", &crate::media::dialog_extensions()).pick_files()
            {
                for p in paths {
                    let path = p.to_string_lossy().to_string();
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
                    st.comp.add_media(&name, &path);
                }
            }
            if ui.button(label(icon::FOLDER_OPEN, "Import folder…")).on_hover_text("Adds every image/movie in the folder (and one level of subfolders)").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                let mut budget = 500;
                import_folder(&mut st.comp, &dir, 1, &mut budget);
            }
        }
        ui.weak("Click = put on selected layer · drag onto a layer · right-click to remove");
    });
    ui.add_space(4.0);
    let State { comp, thumbs, selected, .. } = st;
    let mut assign: Option<usize> = None;
    let mut new_layer_from: Option<usize> = None;
    let mut remove: Option<usize> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if comp.media_bin.is_empty() {
            ui.weak("Empty. Import files or a folder, or drop files onto the window.");
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for (i, item) in comp.media_bin.iter().enumerate() {
                let size = egui::vec2(128.0, 72.0 + 18.0);
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
                resp.dnd_set_drag_payload(MediaDrag(i));
                let thumb_rect = egui::Rect::from_min_size(rect.min, egui::vec2(128.0, 72.0));
                let p = ui.painter();
                p.rect_filled(thumb_rect, 4.0, Color32::from_gray(14));
                match thumbs.get(&item.path) {
                    Some(tex) => paint_texture(p, tex, thumb_rect),
                    None => {
                        let glyph = if crate::media::is_movie(&item.name) { icon::FILM_STRIP } else { icon::IMAGE };
                        let text = if thumbs.is_pending(&item.path) { "…" } else { glyph };
                        p.text(thumb_rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(22.0), Color32::GRAY);
                    }
                }
                let stroke = if resp.hovered() { Stroke::new(1.5, ACCENT) } else { Stroke::new(1.0, Color32::from_gray(50)) };
                p.rect_stroke(thumb_rect, 4.0, stroke, StrokeKind::Inside);
                let mut name = item.name.clone();
                if name.chars().count() > 18 {
                    name = format!("{}…", name.chars().take(17).collect::<String>());
                }
                p.text(egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, name, egui::FontId::proportional(11.0), Color32::LIGHT_GRAY);
                if resp.clicked() {
                    assign = Some(i);
                }
                resp.on_hover_text(&item.path).context_menu(|ui| {
                    if ui.button(label(icon::PLUS, "Put on a new layer")).clicked() {
                        new_layer_from = Some(i);
                        ui.close();
                    }
                    #[cfg(target_os = "macos")]
                    if ui.button(label(icon::FOLDER_OPEN, "Reveal in Finder")).clicked() {
                        let _ = std::process::Command::new("open").arg("-R").arg(&item.path).spawn();
                        ui.close();
                    }
                    if ui.button(label(icon::TRASH, "Remove from bin")).clicked() {
                        remove = Some(i);
                        ui.close();
                    }
                });
            }
        });
    });
    if let Some(i) = assign
        && let Some(item) = comp.media_bin.get(i).cloned()
    {
        let target = match *selected {
            Some(id) if id != MASTER_ID && comp.layers.iter().any(|l| l.id == id) => id,
            _ => {
                let id = comp.add_layer().id;
                *selected = Some(id);
                id
            }
        };
        if let Some(l) = comp.layer_mut(target) {
            l.source = crate::media::spec_for_path(&item.path);
            l.name = item.name;
        }
    }
    if let Some(i) = new_layer_from
        && let Some(item) = comp.media_bin.get(i).cloned()
    {
        let l = comp.add_layer();
        l.source = crate::media::spec_for_path(&item.path);
        l.name = item.name;
        *selected = Some(l.id);
    }
    if let Some(i) = remove {
        let item = comp.media_bin.remove(i);
        thumbs.forget(&item.path);
    }
}

/// Columns per bank page.
pub const CLIP_PAGE_SIZE: usize = 9;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quantize {
    #[default]
    Off,
    Beat,
    Bar,
}

impl Quantize {
    pub const ALL: [Quantize; 3] = [Quantize::Off, Quantize::Beat, Quantize::Bar];

    pub fn name(self) -> &'static str {
        match self {
            Quantize::Off => "Instant",
            Quantize::Beat => "On beat",
            Quantize::Bar => "On bar",
        }
    }
}

fn clips(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    let aspect = st.aspect();
    ui.horizontal(|ui| {
        if ui.button(label(icon::METRONOME, "Tap")).on_hover_text("Tap 3+ times to set the tempo").clicked() {
            st.clock.tap(now);
        }
        ui.add(egui::DragValue::new(&mut st.clock.bpm).range(40.0..=300.0).speed(0.5).suffix(" bpm"));
        if ui.button("Resync").on_hover_text("Beat 1 = now").clicked() {
            st.clock.resync(now);
        }
        // Beat indicator: four dots, the current one lit.
        let beat = st.clock.beat_in_bar(now);
        let phase = st.clock.phase(now);
        for i in 0..4 {
            let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
            let lit = i == beat;
            let color = if lit { ACCENT.gamma_multiply(1.0 - 0.6 * phase) } else { Color32::from_gray(60) };
            ui.painter().circle_filled(r.center(), if lit { 5.0 } else { 4.0 }, color);
        }
        ComboBox::from_id_salt("quantize").width(90.0).selected_text(st.quantize.name()).show_ui(ui, |ui| {
            for q in Quantize::ALL {
                ui.selectable_value(&mut st.quantize, q, q.name());
            }
        });
        if !st.pending_triggers.is_empty() {
            ui.colored_label(ACCENT, format!("{} queued", st.pending_triggers.len()));
        }
        ui.separator();
        ui.label(RichText::new("Bank").small());
        for (i, name) in ["A", "B", "C", "D"].iter().enumerate() {
            if ui.selectable_label(st.clip_page == i, *name).clicked() {
                st.clip_page = i;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(label(icon::PLUS, "Column")).clicked() {
                st.comp.clip_columns += 1;
            }
            ui.weak("Keys 1–9 trigger this bank's columns · drop media onto a cell · right-click a cell");
        });
    });
    ui.add_space(4.0);
    let cols = st.comp.clip_columns.max(1);
    let page_offset = st.clip_page * CLIP_PAGE_SIZE;
    let cell = egui::vec2(96.0, 96.0 / aspect + 16.0);
    let mut triggers: Vec<Trigger> = Vec::new();
    let mut set_cell: Option<(usize, usize, usize)> = None; // (layer idx, slot, media idx)
    let mut use_current: Option<(usize, usize)> = None;
    let mut clear: Option<(usize, usize)> = None;
    {
        let State { comp, thumbs, .. } = st;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            // Column headers trigger whole columns.
            ui.horizontal(|ui| {
                ui.allocate_exact_size(egui::vec2(110.0, 22.0), Sense::hover());
                for c in page_offset..page_offset + cols {
                    let head = ui.add_sized([cell.x, 22.0], egui::Button::new(format!("{} {}", icon::PLAY, c + 1))).on_hover_text("Trigger this column on every layer · right-click to Learn");
                    if head.clicked() {
                        triggers.push(Trigger::Column(c));
                    }
                    mapped_badge(ui, &head, &ControlTarget::TriggerColumn(c));
                    head.context_menu(|ui| learn_items(ui, &ControlTarget::TriggerColumn(c)));
                }
            });
            let n = comp.layers.len();
            for idx in (0..n).rev() {
                let layer = &mut comp.layers[idx];
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(110.0, cell.y), Sense::hover());
                    ui.painter().text(r.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, &layer.name, egui::FontId::proportional(13.0), Color32::LIGHT_GRAY);
                    for c in page_offset..page_offset + cols {
                        let clip = layer.clips.get(c);
                        let (rect, resp) = ui.allocate_exact_size(cell, Sense::click());
                        let active = clip.is_some_and(|k| !k.is_empty() && layer.active_clip == Some(k.id));
                        let thumb_rect = egui::Rect::from_min_size(rect.min, egui::vec2(cell.x, cell.x / aspect));
                        let p = ui.painter();
                        p.rect_filled(rect, 4.0, if active { Color32::from_rgb(24, 40, 32) } else { Color32::from_gray(18) });
                        match clip {
                            Some(k) if !k.is_empty() => {
                                let tex = match &k.source {
                                    SourceSpec::Image { path: Some(pth), .. } | SourceSpec::Movie { path: Some(pth), .. } => thumbs.get(pth),
                                    _ => None,
                                };
                                match tex {
                                    Some(t) => paint_texture(p, t, thumb_rect),
                                    None => {
                                        p.rect_filled(thumb_rect, 4.0, Color32::from_gray(30));
                                        p.text(thumb_rect.center(), egui::Align2::CENTER_CENTER, k.source.label(), egui::FontId::proportional(11.0), Color32::LIGHT_GRAY);
                                    }
                                }
                                let mut name = if k.name.is_empty() { k.source.label() } else { k.name.clone() };
                                if name.chars().count() > 14 {
                                    name = format!("{}…", name.chars().take(13).collect::<String>());
                                }
                                p.text(egui::pos2(rect.left() + 3.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, name, egui::FontId::proportional(10.5), Color32::LIGHT_GRAY);
                            }
                            _ => {
                                p.text(rect.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(18.0), Color32::from_gray(70));
                            }
                        }
                        let stroke = if active {
                            Stroke::new(2.0, ACCENT)
                        } else if resp.hovered() || resp.dnd_hover_payload::<MediaDrag>().is_some() {
                            Stroke::new(1.0, ACCENT.gamma_multiply(0.7))
                        } else {
                            Stroke::new(1.0, Color32::from_gray(45))
                        };
                        p.rect_stroke(rect, 4.0, stroke, StrokeKind::Inside);
                        if let Some(m) = resp.dnd_release_payload::<MediaDrag>() {
                            set_cell = Some((idx, c, m.0));
                        }
                        if resp.clicked() {
                            match clip {
                                Some(k) if !k.is_empty() => triggers.push(Trigger::Clip { layer: layer.id, slot: c }),
                                _ => use_current = Some((idx, c)),
                            }
                        }
                        mapped_badge(ui, &resp, &ControlTarget::TriggerClip { layer: layer.id, slot: c });
                        resp.context_menu(|ui| {
                            if clip.is_some_and(|k| !k.is_empty()) {
                                if ui.button(label(icon::PLAY, "Trigger")).clicked() {
                                    triggers.push(Trigger::Clip { layer: layer.id, slot: c });
                                    ui.close();
                                }
                                learn_items(ui, &ControlTarget::TriggerClip { layer: layer.id, slot: c });
                                ui.separator();
                            }
                            if ui.button("Store the layer's current source here").clicked() {
                                use_current = Some((idx, c));
                                ui.close();
                            }
                            if ui.button("Clear slot").clicked() {
                                clear = Some((idx, c));
                                ui.close();
                            }
                        });
                    }
                });
            }
            if n == 0 {
                ui.weak("Add layers first; each layer gets a row of clip slots.");
            }
        });
    }
    if let Some((idx, c, m)) = set_cell
        && let Some(item) = st.comp.media_bin.get(m).cloned()
        && let Some(layer) = st.comp.layers.get_mut(idx)
    {
        let id = layer.id;
        let slot = layer.clip_slot_mut(c);
        slot.source = crate::media::spec_for_path(&item.path);
        slot.name = item.name;
        slot.generator_values.clear();
        if slot.id == 0 {
            slot.id = id * 1000 + c as u64 + 1;
        }
    }
    if let Some((idx, c)) = use_current
        && let Some(layer) = st.comp.layers.get_mut(idx)
    {
        let (src, vals, name, id) = (layer.source.clone(), layer.generator_values.clone(), layer.name.clone(), layer.id);
        if src != SourceSpec::None {
            let slot = layer.clip_slot_mut(c);
            slot.source = src;
            slot.generator_values = vals;
            slot.name = name;
            slot.id = id * 1000 + c as u64 + 1;
            layer.active_clip = Some(slot.id);
        }
    }
    if let Some((idx, c)) = clear
        && let Some(layer) = st.comp.layers.get_mut(idx)
        && c < layer.clips.len()
    {
        layer.clips[c] = Default::default();
    }
    for t in triggers {
        st.trigger(t, now);
    }
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
            if ui.selectable_label(audio.is_network(), "Session viewers (network)").on_hover_text("Audio sent back by people watching your session link").clicked() {
                audio.use_network();
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
    st.comp.ensure_outputs();
    if !st.comp.outputs.iter().any(|o| o.id == st.mapping_output) {
        st.mapping_output = st.primary_output();
    }
    // Which output's surfaces are being edited.
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Output").strong());
        let outs: Vec<(u64, String)> = st.comp.outputs.iter().map(|o| (o.id, o.name.clone())).collect();
        for (id, name) in outs {
            ui.selectable_value(&mut st.mapping_output, id, name);
        }
        if ui.small_button(label(icon::PLUS, "Output")).on_hover_text("Add another output (projector / screen)").clicked() {
            st.mapping_output = st.comp.add_output();
        }
    });
    let oid = st.mapping_output;
    let tex = st.output_tex(oid);
    let mut edit_on_output = st.output.lock().unwrap().get(&oid).is_some_and(|o| o.edit_mapping);
    let canvas = (st.comp.width, st.comp.height);
    let mut add_surface = false;
    {
        let State { comp, output, .. } = st;
        let Some(o) = comp.outputs.iter_mut().find(|o| o.id == oid) else { return };
        ui.horizontal(|ui| {
            ui.toggle_value(&mut o.mapping.enabled, label(icon::PROJECTOR_SCREEN, "Mapping on"))
                .on_hover_text("When on, this output shows only its surfaces");
            ui.toggle_value(&mut o.mapping.test_pattern, label(icon::GRID_FOUR, "Alignment grid"));
            if ui.button(label(icon::PLUS, "Surface")).clicked() {
                add_surface = true;
            }
            if ui.checkbox(&mut edit_on_output, "Edit on output (E)").on_hover_text("Show draggable handles in this output's window").changed() {
                output.lock().unwrap().entry(oid).or_default().edit_mapping = edit_on_output;
            }
        });
    }
    if add_surface {
        let id = st.comp.new_id();
        if let Some(o) = st.comp.output_mut(oid) {
            o.mapping.add_surface(id);
            o.mapping.enabled = true;
        }
        st.selected_surface = Some(id);
    }
    ui.weak("Drag corners to pin a surface onto the real object. Drag the centre to move it. Overlap soft edges to blend projectors.");

    let State { comp, selected_surface, .. } = st;
    let Some(o) = comp.outputs.iter_mut().find(|o| o.id == oid) else { return };
    let (ow, oh) = o.size(canvas);
    let aspect = ow as f32 / oh.max(1) as f32;
    let avail = ui.available_size();
    let h = (avail.y * 0.5).min(avail.x / aspect).max(120.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail.x, h), Sense::hover());
    let frame = fit_rect(rect.shrink(4.0), aspect);
    ui.painter().rect_filled(frame, 0.0, Color32::BLACK);
    if let Some(t) = tex {
        paint_texture(ui.painter(), t, frame);
    }
    ui.painter().rect_stroke(frame, 0.0, Stroke::new(1.0, Color32::from_gray(70)), StrokeKind::Outside);
    mapping_handles(ui, frame, &mut o.mapping, selected_surface, true);

    ui.add_space(4.0);
    let mut remove = None;
    let mut duplicate = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 180.0).max(80.0);
        if o.mapping.surfaces.is_empty() {
            ui.weak("No surfaces yet. Add one, then drag its corners.");
        }
        for (i, s) in o.mapping.surfaces.iter_mut().enumerate() {
            let is_selected = *selected_surface == Some(s.id);
            let card_id = egui::Id::new(("surface_card", s.id));
            let bg = card_background(ui, card_id);
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
                ui.collapsing("Input region (part of the mix shown)", |ui| {
                    ui.add(Slider::new(&mut s.input[0], 0.0..=1.0).text("X"));
                    ui.add(Slider::new(&mut s.input[1], 0.0..=1.0).text("Y"));
                    ui.add(Slider::new(&mut s.input[2], 0.05..=1.0).text("Width"));
                    ui.add(Slider::new(&mut s.input[3], 0.05..=1.0).text("Height"));
                });
            });
            remember_card(ui, card_id, frame.response.rect);
            let resp = bg.unwrap_or_else(|| frame.response.clone());
            if resp.clicked() {
                *selected_surface = Some(s.id);
            }
            resp.context_menu(|ui| {
                if ui.button(label(icon::CORNERS_OUT, "Reset corners")).clicked() {
                    s.corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                    ui.close();
                }
                if ui.button(label(icon::COPY, "Duplicate")).clicked() {
                    duplicate = Some(i);
                    ui.close();
                }
                if ui.button(label(icon::TRASH, "Delete")).clicked() {
                    remove = Some(i);
                    ui.close();
                }
            });
            ui.add_space(4.0);
        }
    });
    if let Some(i) = remove {
        let removed = o.mapping.surfaces.remove(i);
        if *selected_surface == Some(removed.id) {
            *selected_surface = None;
        }
    }
    if let Some(i) = duplicate {
        let mut copy = o.mapping.surfaces[i].clone();
        copy.id = comp.next_id;
        comp.next_id += 1;
        copy.name = format!("{} copy", copy.name);
        for c in &mut copy.corners {
            c[0] += 0.03;
            c[1] += 0.03;
        }
        if let Some(o) = comp.outputs.iter_mut().find(|o| o.id == oid) {
            o.mapping.surfaces.insert(i + 1, copy);
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
    let State { stream, notice, stream_audio_devices, .. } = st;
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
        #[cfg(not(target_arch = "wasm32"))]
        ui.horizontal(|ui| {
            ui.label("Audio");
            let current = if s.audio_device < 0 {
                "None".to_string()
            } else {
                stream_audio_devices.iter().find(|(i, _)| *i == s.audio_device).map(|(_, n)| n.clone()).unwrap_or_else(|| format!("Device {}", s.audio_device))
            };
            ComboBox::from_id_salt("stream_audio").width(220.0).selected_text(current).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.audio_device, -1, "None");
                for (i, name) in stream_audio_devices.iter() {
                    ui.selectable_value(&mut s.audio_device, *i, name);
                }
            });
            if ui.button(icon::ARROWS_CLOCKWISE).on_hover_text("Scan audio devices (mic, interface, BlackHole loopback)").clicked() {
                *stream_audio_devices = crate::stream::list_audio_devices();
            }
        });
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
    ui.weak("Hardware H.264 via VideoToolbox. For the DJ's music in the stream, pick a loopback device (BlackHole) or your interface as Audio.");
}

fn network(ui: &mut egui::Ui, st: &mut State) {
    let State { net, notice, audio, .. } = st;
    ui.label(RichText::new("Session").strong());
    ui.weak("Anyone on your network opens the link and sees the output live; they can send their music back.");
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!net.running(), |ui| {
            ui.add(egui::DragValue::new(&mut net.port).range(1024..=65535).prefix("port "));
            ComboBox::from_id_salt("net_fps").width(60.0).selected_text(format!("{} fps", net.fps)).show_ui(ui, |ui| {
                for f in [10, 15, 20, 30] {
                    ui.selectable_value(&mut net.fps, f, format!("{f} fps"));
                }
            });
        });
        if net.running() {
            if ui.button(label(icon::STOP, "Stop session")).clicked() {
                net.stop();
            }
        } else if ui.button(label(icon::BROADCAST, "Start session")).clicked() {
            match net.start() {
                Ok(()) => *notice = None,
                Err(e) => *notice = Some(e),
            }
        }
    });
    if net.running() {
        let url = net.url();
        ui.horizontal(|ui| {
            ui.monospace(&url);
            if ui.button(icon::COPY).on_hover_text("Copy link").clicked() {
                ui.ctx().copy_text(url.clone());
            }
        });
        ui.separator();
        ui.label(RichText::new(format!("{} Phone / tablet remote", icon::DEVICE_MOBILE)).strong());
        ui.weak("Scan with your phone or iPad camera: faders, clips, tap tempo and the selected layer's controls, synced both ways.");
        let remote = net.remote_url();
        ui.horizontal(|ui| {
            qr_code(ui, &remote, 150.0);
            ui.vertical(|ui| {
                ui.monospace(&remote);
                if ui.button(icon::COPY).on_hover_text("Copy link").clicked() {
                    ui.ctx().copy_text(remote.clone());
                }
                ui.monospace(format!("remotes connected: {}", net.remotes.load(std::sync::atomic::Ordering::Relaxed)));
                ui.weak("Tip: Share ▸ Add to Home Screen makes it full-screen like an app.");
            });
        });
        ui.separator();
        ui.monospace(format!(
            "viewers {} · sending audio {}",
            net.viewers.load(std::sync::atomic::Ordering::Relaxed),
            net.audio_peers.load(std::sync::atomic::Ordering::Relaxed)
        ));
        if !audio.is_network() {
            if ui.button(label(icon::MICROPHONE, "React to viewers' audio")).clicked() {
                audio.use_network();
            }
        } else {
            ui.colored_label(ACCENT, "Audio analysis is listening to session viewers");
        }
        ui.add_space(4.0);
        ui.weak("Picture: MJPEG at up to 1280 px (works in any browser, Pi included). Mic sending needs a secure page: use http://localhost or add this origin to chrome://flags/#unsafely-treat-insecure-origin-as-secure on the viewer.");
    }
    if let Some(msg) = notice {
        ui.colored_label(WARN, msg.as_str());
    }
}

const RESOLUTIONS: &[(u32, u32, &str)] = &[
    (1280, 720, "1280×720 (720p)"),
    (1920, 1080, "1920×1080 (1080p)"),
    (2560, 1440, "2560×1440"),
    (3840, 2160, "3840×2160 (4K)"),
];

fn output(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Screens").strong());
        if ui.small_button(label(icon::ARROWS_CLOCKWISE, "Rescan")).clicked() {
            st.displays = crate::screens::displays();
        }
    });
    if st.displays.is_empty() {
        ui.weak("No displays found.");
    }
    for d in st.displays.clone() {
        ui.horizontal(|ui| {
            let icon_name = if d.main { icon::LAPTOP } else { icon::PROJECTOR_SCREEN };
            ui.label(format!("{icon_name} {}", d.label()));
            if d.main {
                ui.weak("(this screen)");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Send Output 1 here").on_hover_text("Opens the first output fullscreen on this display, no menu bar").clicked() {
                    st.send_output_to(d.clone());
                }
            });
        });
    }
    ui.separator();
    outputs_list(ui, st);
    ui.separator();
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

        ui.label("All output windows");
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

// -------------------------------------------------------------------------------------------
// Monitors: extra preview panels showing the output, the mix, or one layer.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitorSource {
    /// Exactly what the output window shows (after master fade and mapping).
    Output,
    /// The mix after master effects, before mapping.
    Mix,
    /// One layer after its effects, before blending.
    Layer(u64),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MonitorCfg {
    pub source: MonitorSource,
    pub grid: bool,
}

impl Default for MonitorCfg {
    fn default() -> Self {
        Self { source: MonitorSource::Mix, grid: false }
    }
}

pub fn monitor_source_name(st: &State, n: u8) -> String {
    match st.monitors.get(&n).map(|c| c.source).unwrap_or(MonitorSource::Mix) {
        MonitorSource::Output => "Output".into(),
        MonitorSource::Mix => "Mix".into(),
        MonitorSource::Layer(id) => st.comp.layers.iter().find(|l| l.id == id).map_or("(removed layer)".into(), |l| l.name.clone()),
    }
}

fn monitor(ui: &mut egui::Ui, st: &mut State, n: u8) {
    let aspect = st.aspect();
    let mut cfg = st.monitors.get(&n).copied().unwrap_or_default();
    ui.horizontal(|ui| {
        let current = monitor_source_name(st, n);
        ComboBox::from_id_salt(("monitor_src", n)).width(180.0).selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut cfg.source, MonitorSource::Output, "Output (what the projector sees)");
            ui.selectable_value(&mut cfg.source, MonitorSource::Mix, "Mix (before mapping)");
            ui.separator();
            for l in st.comp.layers.iter().rev() {
                ui.selectable_value(&mut cfg.source, MonitorSource::Layer(l.id), format!("Layer: {}", l.name));
            }
        });
        ui.toggle_value(&mut cfg.grid, icon::GRID_FOUR).on_hover_text("Thirds and centre guides");
        ui.weak("Drag the tab anywhere, or right-click it → Detach for its own window");
    });
    st.monitors.insert(n, cfg);
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 4.0, Color32::from_gray(6));
    let frame = fit_rect(rect.shrink(4.0), aspect);
    let tex = match cfg.source {
        MonitorSource::Output => st.texture,
        MonitorSource::Mix | MonitorSource::Layer(_) => {
            let ppp = ui.ctx().pixels_per_point();
            let w = ((frame.width() * ppp) as u32).clamp(64, st.comp.width);
            let h = ((w as f32 / aspect) as u32).max(16);
            let src = match cfg.source {
                MonitorSource::Layer(id) => id,
                _ => MASTER_ID,
            };
            st.monitor_wants.push((n, src, w, h));
            st.monitor_tex(n)
        }
    };
    match tex {
        Some(t) => paint_texture(ui.painter(), t, frame),
        None => {
            ui.painter().text(frame.center(), egui::Align2::CENTER_CENTER, "no picture (layer off?)", egui::FontId::proportional(13.0), Color32::GRAY);
        }
    }
    if cfg.grid {
        let p = ui.painter();
        let stroke = Stroke::new(1.0, Color32::from_white_alpha(60));
        for k in 1..3 {
            let x = frame.left() + frame.width() * k as f32 / 3.0;
            let y = frame.top() + frame.height() * k as f32 / 3.0;
            p.line_segment([egui::pos2(x, frame.top()), egui::pos2(x, frame.bottom())], stroke);
            p.line_segment([egui::pos2(frame.left(), y), egui::pos2(frame.right(), y)], stroke);
        }
        p.circle_stroke(frame.center(), 6.0, Stroke::new(1.0, ACCENT));
    }
    ui.allocate_rect(rect, Sense::hover());
}

// -------------------------------------------------------------------------------------------
// Mixer: one vertical fader strip per layer, plus master.

fn fader(ui: &mut egui::Ui, value: &mut f32, m: &mut Option<Modulation>, target: ControlTarget, height: f32) {
    ui.spacing_mut().slider_width = height;
    let r = ui.add(Slider::new(value, 0.0..=1.0).vertical().show_value(false));
    if let Some(mm) = m.as_ref() {
        let live = phosphor_engine::control::modulate(*value, 0.0, 1.0, Some(mm), &mod_inputs(ui));
        let rr = r.rect;
        let pad = rr.width() * 0.5;
        let y = rr.bottom() - pad - live * (height - 2.0 * pad).max(1.0);
        ui.painter().line_segment([egui::pos2(rr.left() - 2.0, y), egui::pos2(rr.right() + 2.0, y)], Stroke::new(2.0, ACCENT));
    }
    mapped_badge(ui, &r, &target);
    if control_menu(&r, Some(&target), Some(m)) {
        *value = 1.0;
    }
}

fn mixer(ui: &mut egui::Ui, st: &mut State) {
    let aspect = st.aspect();
    let height = (ui.available_height() - 150.0).clamp(80.0, 600.0);
    let State { comp, selected, layer_textures, texture, .. } = st;
    let any_solo = comp.layers.iter().any(|l| l.enabled && l.solo);
    egui::ScrollArea::horizontal().auto_shrink(false).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for layer in comp.layers.iter_mut() {
                let is_sel = *selected == Some(layer.id);
                let card_id = egui::Id::new(("mixer_strip", layer.id));
                let bg = card_background(ui, card_id);
                let frame = card_frame(ui, is_sel).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
                    ui.set_width(78.0);
                    ui.vertical_centered(|ui| {
                        let tex = layer_textures.get(&layer.id).map(|(t, _)| *t);
                        let muted = any_solo && !layer.solo;
                        if thumbnail(ui, tex, egui::vec2(72.0, 72.0 / aspect), layer.enabled && !muted).clicked() {
                            *selected = Some(layer.id);
                        }
                        let mut name = layer.name.clone();
                        if name.chars().count() > 11 {
                            name = format!("{}…", name.chars().take(10).collect::<String>());
                        }
                        ui.label(RichText::new(name).small().strong());
                        fader(ui, &mut layer.opacity, &mut layer.opacity_mod, ControlTarget::LayerOpacity(layer.id), height);
                        ui.monospace(format!("{:3.0}%", layer.opacity * 100.0));
                        ui.horizontal(|ui| {
                            let on = ui.toggle_value(&mut layer.enabled, RichText::new("ON").small());
                            on.context_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(layer.id)));
                            ui.toggle_value(&mut layer.solo, RichText::new("S").small());
                        });
                        ComboBox::from_id_salt(("mix_blend", layer.id)).width(70.0).selected_text(RichText::new(layer.blend.name()).small()).show_ui(ui, |ui| {
                            for mode in BlendMode::ALL {
                                ui.selectable_value(&mut layer.blend, mode, mode.name());
                            }
                        });
                    });
                });
                remember_card(ui, card_id, frame.response.rect);
                if bg.is_some_and(|b| b.clicked()) {
                    *selected = Some(layer.id);
                }
            }
            // Master strip.
            card_frame(ui, *selected == Some(MASTER_ID)).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
                ui.set_width(78.0);
                ui.vertical_centered(|ui| {
                    if thumbnail(ui, *texture, egui::vec2(72.0, 72.0 / aspect), true).clicked() {
                        *selected = Some(MASTER_ID);
                    }
                    ui.label(RichText::new("MASTER").small().strong().color(ACCENT));
                    fader(ui, &mut comp.master_fade, &mut comp.master_fade_mod, ControlTarget::MasterFade, height);
                    ui.monospace(format!("{:3.0}%", comp.master_fade * 100.0));
                });
            });
        });
    });
}

const OUTPUT_SIZES: &[(u32, u32, &str)] = &[
    (0, 0, "Same as canvas"),
    (1280, 720, "1280×720"),
    (1920, 1080, "1920×1080"),
    (1920, 1200, "1920×1200"),
    (2560, 1440, "2560×1440"),
    (3840, 2160, "3840×2160"),
    (1080, 1920, "1080×1920 (portrait)"),
];

/// Every output with its size, screen, window and Identify controls.
fn outputs_list(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Outputs").strong());
        if ui.small_button(label(icon::PLUS, "Add output")).clicked() {
            st.mapping_output = st.comp.add_output();
        }
        if ui.small_button(label(icon::HASH, "Identify")).on_hover_text("Show each output's number on its window").clicked() {
            st.identify_outputs(now);
        }
    });
    let canvas = (st.comp.width, st.comp.height);
    let mut remove = None;
    let count = st.comp.outputs.len();
    for i in 0..count {
        let id = st.comp.outputs[i].id;
        let open = st.output_window_open(id);
        let tex = st.output_tex(id);
        let mut toggle = false;
        let mut send: Option<crate::screens::Display> = None;
        let displays = st.displays.clone();
        let o = &mut st.comp.outputs[i];
        card_frame(ui, false).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(64.0, 36.0), Sense::hover());
                ui.painter().rect_filled(r, 3.0, Color32::BLACK);
                if let Some(t) = tex {
                    let (ow, oh) = o.size(canvas);
                    paint_texture(ui.painter(), t, fit_rect(r, ow as f32 / oh.max(1) as f32));
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}", i + 1)).strong().color(ACCENT));
                        ui.add(egui::TextEdit::singleline(&mut o.name).desired_width(110.0));
                        ui.toggle_value(&mut o.enabled, RichText::new("ON").small()).on_hover_text("Render this output");
                        if o.mapping.active() {
                            ui.label(RichText::new(format!("{} mapped", icon::PROJECTOR_SCREEN)).small().color(ACCENT));
                        }
                    });
                    ui.horizontal(|ui| {
                        let current = OUTPUT_SIZES.iter().find(|(w, h, _)| *w == o.width && *h == o.height).map_or(format!("{}×{}", o.width, o.height), |(_, _, n)| n.to_string());
                        ComboBox::from_id_salt(("out_size", id)).width(130.0).selected_text(current).show_ui(ui, |ui| {
                            for (w, h, name) in OUTPUT_SIZES {
                                if ui.selectable_label(o.width == *w && o.height == *h, *name).clicked() {
                                    o.width = *w;
                                    o.height = *h;
                                }
                            }
                        });
                        let text = if open { label(icon::STOP, "Window") } else { label(icon::PLAY, "Window") };
                        if ui.button(text).clicked() {
                            toggle = true;
                        }
                        ui.menu_button(label(icon::PROJECTOR_SCREEN, "Send to"), |ui| {
                            for d in &displays {
                                if ui.button(d.label()).clicked() {
                                    send = Some(d.clone());
                                    ui.close();
                                }
                            }
                            if displays.is_empty() {
                                ui.weak("No displays found");
                            }
                        });
                        if count > 1 && ui.small_button(icon::TRASH).on_hover_text("Remove output").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            });
        });
        if toggle {
            st.toggle_output_window(id);
        }
        if let Some(d) = send {
            st.send_output_id_to(id, d);
        }
    }
    if let Some(i) = remove {
        let id = st.comp.outputs.remove(i).id;
        st.output.lock().unwrap().remove(&id);
    }
}

/// Draws a QR code for `text` (no image needed: one filled square per dark module).
fn qr_code(ui: &mut egui::Ui, text: &str, size: f32) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Ok(code) = qrcode::QrCode::new(text.as_bytes()) else { return };
        let n = code.width();
        let colors = code.to_colors();
        let quiet = 2.0;
        let cell = size / (n as f32 + quiet * 2.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 4.0, Color32::WHITE);
        for (i, c) in colors.iter().enumerate() {
            if *c == qrcode::Color::Dark {
                let (x, y) = ((i % n) as f32, (i / n) as f32);
                let min = rect.min + egui::vec2((x + quiet) * cell, (y + quiet) * cell);
                p.rect_filled(egui::Rect::from_min_size(min, egui::vec2(cell + 0.5, cell + 0.5)), 0.0, Color32::BLACK);
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (ui, text, size);
}
