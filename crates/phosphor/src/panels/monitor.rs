//! Preview windows (VDMX's Preview plugin): as many as you like, each showing the output window, the mix, one
//! layer, one group / comp or one output after its mapping. Open one from the Preview bar, View › New Preview
//! Window, or any layer / group / output's right-click menu (`request_preview`).

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitorSource {
    /// Exactly what the output window shows (after master fade and mapping).
    Output,
    /// The mix after master effects, before mapping.
    Mix,
    /// One layer after its effects, before blending.
    Layer(u64),
    /// One group (comp) after its effects, before blending.
    Group(u64),
    /// One output after its mapping (what that projector shows).
    OneOutput(u64),
}

pub const PREVIEW_REQUEST: &str = "phosphor_preview_request";

/// Opens a new preview window showing `source` (from any panel; the app opens it after this frame).
pub(crate) fn request_preview(ui: &egui::Ui, source: MonitorSource) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(PREVIEW_REQUEST), source));
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MonitorCfg {
    pub source: MonitorSource,
    pub grid: bool,
    /// Click / drag on the picture = X/Y data (two Hand modulators any control can follow), like VDMX's preview.
    #[serde(default)]
    pub xy: bool,
}

impl Default for MonitorCfg {
    fn default() -> Self {
        Self { source: MonitorSource::Mix, grid: false, xy: false }
    }
}

pub fn monitor_source_name(st: &State, n: u8) -> String {
    match st.monitors.get(&n).map(|c| c.source).unwrap_or(MonitorSource::Mix) {
        MonitorSource::Output => "Output".into(),
        MonitorSource::Mix => "Mix".into(),
        MonitorSource::Layer(id) => crate::ui::layer_name(&st.comp.layers, id),
        MonitorSource::Group(id) => st.comp.group(id).map_or("(removed group)".into(), |g| g.name.clone()),
        MonitorSource::OneOutput(id) => st.comp.outputs.iter().find(|o| o.id == id).map_or("(removed output)".into(), |o| o.name.clone()),
    }
}

pub(crate) fn monitor(ui: &mut egui::Ui, st: &mut State, n: u8) {
    let mut cfg = st.monitors.get(&n).copied().unwrap_or_default();
    let canvas = (st.comp.width, st.comp.height);
    let aspect = match cfg.source {
        MonitorSource::OneOutput(id) => st.comp.outputs.iter().find(|o| o.id == id).map_or(st.aspect(), |o| {
            let (w, h) = o.size(canvas);
            w as f32 / h.max(1) as f32
        }),
        _ => st.aspect(),
    };
    ui.horizontal(|ui| {
        let current = monitor_source_name(st, n);
        ComboBox::from_id_salt(("monitor_src", n)).width(180.0).selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut cfg.source, MonitorSource::Output, "Output (what the projector sees)");
            ui.selectable_value(&mut cfg.source, MonitorSource::Mix, "Mix (before mapping)");
            ui.separator();
            crate::ui::layer_items(ui, &st.comp.layers, &mut cfg.source, MonitorSource::Layer);
            if !st.comp.groups.is_empty() {
                ui.separator();
                for g in &st.comp.groups {
                    ui.selectable_value(&mut cfg.source, MonitorSource::Group(g.id), format!("Group: {}", g.name));
                }
            }
            ui.separator();
            for o in &st.comp.outputs {
                ui.selectable_value(&mut cfg.source, MonitorSource::OneOutput(o.id), format!("Output: {} (mapped)", o.name));
            }
        });
        toggle(ui, &mut cfg.grid, label(icon::GRID_FOUR, "Guides")).on_hover_text("Thirds and centre guides");
        toggle(ui, &mut cfg.xy, label(icon::CURSOR_CLICK, "XY")).on_hover_text(format!("Click or drag on the picture to set two modulators, \"Preview {n} X\" and \"Preview {n} Y\": any slider can follow them (right-click › Modulate)"));
        hint(ui, format!("Drag the tab anywhere, or right-click it {} Detach for its own window", icon::ARROW_RIGHT));
    });
    st.monitors.insert(n, cfg);
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 4.0, Color32::from_gray(6));
    let frame = fit_rect(rect.shrink(4.0), aspect);
    let tex = match cfg.source {
        MonitorSource::Output => st.texture,
        _ => {
            let ppp = ui.ctx().pixels_per_point();
            let w = ((frame.width() * ppp) as u32).clamp(64, st.comp.width);
            let h = ((w as f32 / aspect) as u32).clamp(16, 4096);
            let src = match cfg.source {
                MonitorSource::Layer(id) | MonitorSource::Group(id) | MonitorSource::OneOutput(id) => id,
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
        p.circle_stroke(frame.center(), 6.0, Stroke::new(1.0, accent()));
    }
    if cfg.xy {
        let r = ui.interact(frame, egui::Id::new(("monitor_xy", n)), Sense::click_and_drag());
        if (r.dragged() || r.clicked())
            && let Some(p) = r.interact_pointer_pos()
        {
            let x = ((p.x - frame.left()) / frame.width()).clamp(0.0, 1.0);
            let y = (1.0 - (p.y - frame.top()) / frame.height()).clamp(0.0, 1.0);
            set_hand(st, &format!("Preview {n} X"), x);
            set_hand(st, &format!("Preview {n} Y"), y);
        }
        // Where the pointer last was: a crosshair from the two modulators.
        let get = |name: &str| st.comp.modulators.iter().find(|m| m.name == name).map(|m| m.value);
        if let (Some(x), Some(y)) = (get(&format!("Preview {n} X")), get(&format!("Preview {n} Y"))) {
            let at = egui::pos2(frame.left() + x * frame.width(), frame.bottom() - y * frame.height());
            let stroke = Stroke::new(1.0, accent());
            ui.painter().line_segment([egui::pos2(at.x, frame.top()), egui::pos2(at.x, frame.bottom())], stroke);
            ui.painter().line_segment([egui::pos2(frame.left(), at.y), egui::pos2(frame.right(), at.y)], stroke);
            ui.painter().circle_stroke(at, 6.0, Stroke::new(2.0, accent()));
        }
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }
    ui.allocate_rect(rect, Sense::hover());
}

// Mixer: one vertical fader strip per layer, plus master.

/// Sets (or first makes) a Hand modulator called `name` to `v`: X/Y from a preview window.
fn set_hand(st: &mut State, name: &str, v: f32) {
    if let Some(m) = st.comp.modulators.iter_mut().find(|m| m.name == name) {
        m.value = v;
        return;
    }
    if st.comp.modulators.len() >= phosphor_engine::control::MAX_SHARED {
        return;
    }
    let id = st.comp.new_id();
    let signal = phosphor_engine::Modulation { source: phosphor_engine::ModSource::Hand, ..Default::default() };
    st.comp.modulators.push(phosphor_engine::Modulator { id, name: name.to_string(), signal, value: v, ..Default::default() });
}
