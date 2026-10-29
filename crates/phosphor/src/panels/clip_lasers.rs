//! The clip grid's laser rows: under the layers, a row per laser projector with a look in each cell. Click a cell to
//! switch the laser to it; right-click to put a look there or clear it; a column's launch switches every laser that
//! has a look in it (`Lasers::trigger_column`, from `Composition::trigger_column`).

use phosphor_engine::Composition;

use super::*;

/// The laser rows (nothing without lasers and looks). `head_w`: the row header's width.
pub(crate) fn laser_rows(ui: &mut egui::Ui, grid: &super::clip_grid::Grid, comp: &mut Composition, head_w: f32) {
    if comp.lasers.projectors.is_empty() || comp.lasers.looks.is_empty() {
        return;
    }
    let looks: Vec<(u64, String)> = comp.lasers.looks.iter().map(|l| (l.id, l.name.clone())).collect();
    ui.label(RichText::new(format!("{} Lasers", icon::PROJECTOR_SCREEN)).strong().color(crate::theme::Hue::Output.color()))
        .on_hover_text("A row per laser: click a look to switch to it, right-click a cell to put a look there; launching a column switches the lasers too");
    for p in &mut comp.lasers.projectors {
        ui.horizontal(|ui| {
            // The header: its name and the look it shows now.
            let (head, _) = ui.allocate_exact_size(egui::vec2(head_w, grid.cell.y), Sense::hover());
            let painter = ui.painter_at(head);
            painter.rect_filled(head, 4.0, ui.visuals().faint_bg_color);
            painter.text(head.left_top() + egui::vec2(8.0, 8.0), egui::Align2::LEFT_TOP, &p.name, egui::FontId::proportional(13.0), ui.visuals().text_color());
            let now = looks.iter().find(|(id, _)| *id == p.look).or(looks.first()).map_or("", |l| l.1.as_str());
            painter.text(head.left_top() + egui::vec2(8.0, 28.0), egui::Align2::LEFT_TOP, now, egui::FontId::proportional(11.0), Color32::GRAY);
            for c in 0..grid.cols {
                let look = p.clips.get(c).copied().unwrap_or(0);
                let (rect, resp) = ui.allocate_exact_size(grid.cell, Sense::click());
                let empty = ui.visuals().extreme_bg_color;
                let name = looks.iter().find(|(id, _)| *id == look).map(|l| l.1.as_str());
                let active = look != 0 && p.look == look;
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 4.0, if active { crate::theme::tint(empty, 0.14) } else { empty });
                match name {
                    Some(name) => {
                        let colour = super::lasers::look_colour(&looks, look);
                        painter.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 5.0)), 2.0, colour);
                        painter.circle_stroke(rect.center() - egui::vec2(0.0, 6.0), (rect.height() * 0.22).min(18.0), Stroke::new(1.5, colour));
                        painter.text(rect.center_bottom() - egui::vec2(0.0, 4.0), egui::Align2::CENTER_BOTTOM, name, egui::FontId::proportional(11.0), ui.visuals().text_color());
                    }
                    None if look != 0 => {
                        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "(removed look)", egui::FontId::proportional(10.0), Color32::GRAY);
                    }
                    None => {}
                }
                if active {
                    painter.rect_stroke(rect, 4.0, Stroke::new(1.5, accent()), egui::StrokeKind::Inside);
                }
                if resp.clicked() && name.is_some() {
                    p.look = look;
                }
                let tip = if name.is_some() { "Click: the laser shows this look · right-click: another look or clear" } else { "Right-click: put a look here" };
                resp.on_hover_text(tip).context_menu(|ui| {
                    for (id, n) in &looks {
                        if crate::ui::pick_item(ui, *id == look, n).clicked() {
                            if p.clips.len() <= c {
                                p.clips.resize(c + 1, 0);
                            }
                            p.clips[c] = *id;
                            ui.close();
                        }
                    }
                    if look != 0 {
                        ui.separator();
                        if danger_button(ui, label(icon::TRASH, "Clear")).clicked() {
                            p.clips[c] = 0;
                            ui.close();
                        }
                    }
                });
            }
        });
    }
}
