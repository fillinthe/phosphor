//! Monitor panels.

use super::*;

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

pub(crate) fn monitor(ui: &mut egui::Ui, st: &mut State, n: u8) {
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
        hint(ui, "Drag the tab anywhere, or right-click it → Detach for its own window");
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

// Mixer: one vertical fader strip per layer, plus master.
