//! A tool window that lives inside the main window until you pop it out: then it's its own OS window
//! (same as detached panels), with "Dock back" to bring it home. Report a Bug and Update use it.

use eframe::egui;
use egui_phosphor::regular as icon;

/// Draws `body` in a floating egui window, or in a native viewport while `detached` is set.
/// `open` goes false when the user closes it either way.
pub(crate) fn tool_window(ctx: &egui::Context, title: &str, open: &mut bool, detached: &mut bool, width: f32, body: impl FnOnce(&mut egui::Ui)) {
    if !*open {
        return;
    }
    let mut body = Some(body);
    if *detached {
        let id = egui::ViewportId::from_hash_of(("phosphor_tool", title));
        let builder = egui::ViewportBuilder::default().with_title(format!("Phosphor · {title}")).with_inner_size([width + 24.0, 420.0]);
        let mut dock_back = false;
        let mut close = false;
        ctx.show_viewport_immediate(id, builder, |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                close = true;
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(title).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::ui::small_command(ui, format!("{} Dock back", icon::ARROW_SQUARE_IN)).clicked() {
                            dock_back = true;
                        }
                    });
                });
                ui.separator();
                if let Some(body) = body.take() {
                    crate::ui::column_scroll(ui).show(ui, body);
                }
            });
        });
        if close {
            *open = false;
            *detached = false;
        }
        if dock_back {
            *detached = false;
        }
        return;
    }
    let mut still_open = true;
    crate::ui::dialog(ctx, title).open(&mut still_open).collapsible(false).resizable(true).default_width(width).show(ctx, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if crate::ui::small_command(ui, icon::ARROW_SQUARE_OUT).on_hover_text("Open in its own window (drag it to another screen)").clicked() {
                *detached = true;
            }
        });
        if let Some(body) = body.take() {
            body(ui);
        }
    });
    *open = still_open;
}
