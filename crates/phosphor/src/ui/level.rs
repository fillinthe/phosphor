//! The one slider for levels and parameters: a plain filled bar with its name and value written inside.
//! Click or drag anywhere on it to set the value, double-click to reset; right-click for Learn, Modulate,
//! Stop the motion, Reset and typing an exact value. When a modulation moves it, a bright line shows where it
//! really is and a lit `~` opens the modulation. Used for opacity, volume, master fade, the Mixer faders and
//! every effect / generator parameter.

use super::*;

pub(crate) struct Level<'a> {
    value: &'a mut f32,
    min: f32,
    max: f32,
    default: f32,
    text: String,
    percent: bool,
    modulation: Option<&'a mut Option<Modulation>>,
    target: Option<ControlTarget>,
    width: Option<f32>,
    vertical: Option<f32>,
    tip: Option<&'a str>,
    log: bool,
}

impl<'a> Level<'a> {
    /// A 0..1 value shown as a percentage.
    pub fn percent(value: &'a mut f32, text: impl Into<String>) -> Self {
        Self { value, min: 0.0, max: 1.0, default: 1.0, text: text.into(), percent: true, modulation: None, target: None, width: None, vertical: None, tip: None, log: false }
    }

    /// Any range, shown as a number.
    pub fn range(value: &'a mut f32, min: f32, max: f32, text: impl Into<String>) -> Self {
        // A range must run low to high.
        let (min, max) = (min.min(max), max.max(min));
        Self { value, min, max, default: min, text: text.into(), percent: false, modulation: None, target: None, width: None, vertical: None, tip: None, log: false }
    }

    /// Up to this value as a percentage (volume goes to 200%).
    pub fn max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    pub fn default(mut self, d: f32) -> Self {
        self.default = d;
        self
    }

    pub fn modulation(mut self, m: &'a mut Option<Modulation>) -> Self {
        self.modulation = Some(m);
        self
    }

    pub fn target(mut self, t: ControlTarget) -> Self {
        self.target = Some(t);
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    /// A vertical fader this tall (Mixer).
    pub fn vertical(mut self, height: f32) -> Self {
        self.vertical = Some(height);
        self
    }

    /// Logarithmic (speed, scale): the middle of the bar is the geometric middle of the range. Needs min > 0.
    pub fn log(mut self) -> Self {
        self.log = self.min > 0.0;
        self
    }

    pub fn tip(mut self, tip: &'a str) -> Self {
        self.tip = Some(tip);
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let Level { value, min, max, default, text, percent, modulation, target, width, vertical, tip, log } = self;
        // Where a value sits along the bar (0..1) and back.
        let to_t = |v: f32| if log { ((v / min).ln() / (max / min).ln()).clamp(0.0, 1.0) } else { ((v - min) / (max - min).max(f32::EPSILON)).clamp(0.0, 1.0) };
        let from_t = |t: f32| if log { min * (max / min).powf(t) } else { min + t * (max - min) };
        let span = (max - min).max(f32::EPSILON);
        let moving = modulation.as_ref().and_then(|m| **m);
        let lit_mod = moving.is_some();
        let size = match vertical {
            Some(h) => egui::vec2(22.0, h),
            None => egui::vec2(width.unwrap_or(ui.available_width() - if lit_mod { 22.0 } else { 0.0 }).max(40.0), ui.spacing().interact_size.y.max(18.0)),
        };
        let (rect, mut r) = ui.allocate_exact_size(size, Sense::click_and_drag());

        // Primary button only: right-click opens the menu without moving the value.
        let primary = ui.input(|i| i.pointer.primary_down() || i.pointer.primary_released());
        if primary
            && (r.dragged() || r.clicked())
            && let Some(p) = r.interact_pointer_pos()
        {
            let t = match vertical {
                Some(_) => (rect.bottom() - p.y) / rect.height(),
                None => (p.x - rect.left()) / rect.width(),
            };
            let v = from_t(t.clamp(0.0, 1.0));
            if v != *value {
                *value = v;
                r.mark_changed();
            }
        }
        if r.double_clicked() {
            *value = default;
            r.mark_changed();
        }

        // Paint: track, fill, the real (modulated) position, then name and value.
        let ui_vis = ui.visuals();
        let p = ui.painter();
        let t = to_t(*value);
        let hovered = r.hovered() || r.dragged();
        p.rect_filled(rect, 4.0, ui_vis.extreme_bg_color);
        let mut fill = rect;
        match vertical {
            Some(_) => fill.min.y = rect.bottom() - t * rect.height(),
            None => fill.max.x = rect.left() + t * rect.width(),
        }
        let base = accent();
        p.rect_filled(fill, 4.0, base.gamma_multiply(if hovered { 0.75 } else { 0.55 }));
        if (percent || log) && max > 1.0 && min < 1.0 {
            // A mark at 100% / 1× on bars that go past it (volume, speed, scale).
            let u = to_t(1.0);
            let x = rect.left() + u * rect.width();
            p.line_segment([egui::pos2(x, rect.top() + 2.0), egui::pos2(x, rect.bottom() - 2.0)], Stroke::new(1.0, ui_vis.weak_text_color()));
        }
        if let Some(m) = moving {
            let live = phosphor_engine::control::modulate(*value, min, max, Some(&m), &mod_inputs(ui));
            let lt = to_t(live);
            let stroke = Stroke::new(2.0, ui_vis.strong_text_color());
            match vertical {
                Some(_) => {
                    let y = rect.bottom() - lt * rect.height();
                    p.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], stroke);
                }
                None => {
                    let x = rect.left() + lt * rect.width();
                    p.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], stroke);
                }
            }
        }
        if hovered {
            p.rect_stroke(rect, 4.0, Stroke::new(1.0, base), StrokeKind::Inside);
        }
        let font = egui::FontId::proportional(11.0);
        let color = ui_vis.strong_text_color();
        let shown = format_value(*value, min, max, percent);
        match vertical {
            Some(_) => {
                p.text(rect.center_bottom() - egui::vec2(0.0, 3.0), egui::Align2::CENTER_BOTTOM, &shown, egui::FontId::proportional(9.5), color);
            }
            None => {
                let pad = 6.0;
                let value_g = p.layout_no_wrap(shown.clone(), font.clone(), color);
                let room = (rect.width() - value_g.size().x - pad * 3.0).max(0.0);
                if !text.is_empty() && room > 16.0 {
                    let mut job = egui::text::LayoutJob::simple_singleline(text.clone(), font.clone(), color);
                    job.wrap = egui::text::TextWrapping::truncate_at_width(room);
                    let g = p.layout_job(job);
                    p.galley(egui::pos2(rect.left() + pad, rect.center().y - g.size().y / 2.0), g, color);
                }
                p.galley(egui::pos2(rect.right() - pad - value_g.size().x, rect.center().y - value_g.size().y / 2.0), value_g, color);
            }
        }
        if hovered {
            ui.ctx().set_cursor_icon(if vertical.is_some() { egui::CursorIcon::ResizeVertical } else { egui::CursorIcon::ResizeHorizontal });
        }

        if let Some(t) = &target {
            mapped_badge(ui, &r, t);
        }
        let hover = match tip {
            Some(tip) => format!("{tip}\nDouble-click to reset · right-click for more"),
            None => format!("{text}\nDouble-click to reset · right-click for more"),
        };
        let r = r.on_hover_text(hover.trim_start());

        // Right-click: Learn, Modulate, Stop the motion, Reset, and an exact value.
        let mut m = modulation;
        let before = *value;
        let mut exact = |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                ui.label("Value");
                if percent {
                    let mut pct = *value * 100.0;
                    if ui.add(egui::DragValue::new(&mut pct).range(min * 100.0..=max * 100.0).speed(0.5).fixed_decimals(0).suffix("%")).changed() {
                        *value = pct / 100.0;
                    }
                } else {
                    let decimals = if max - min >= 20.0 { 0 } else { 2 };
                    ui.add(egui::DragValue::new(&mut *value).range(min..=max).speed(span / 200.0).fixed_decimals(decimals));
                }
            });
        };
        if control_menu_with(&r, target.as_ref(), m.as_deref_mut(), Some(&mut exact)) {
            *value = default;
        }
        let mut r = r;
        if *value != before {
            r.mark_changed();
        }
        // A lit ~ only while something moves it (the right-click menu adds one otherwise).
        if let Some(m) = m
            && m.is_some()
        {
            mod_button(ui, m);
        }
        r
    }
}

/// How a level shows its value: a percentage, a whole number for wide ranges, else two decimals.
fn format_value(v: f32, min: f32, max: f32, percent: bool) -> String {
    if percent {
        format!("{:.0}%", v * 100.0)
    } else if max - min >= 20.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}
