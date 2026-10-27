//! Any button can be mapped: each kit button (commands, toggles, action buttons) is named by its panel, its label
//! and which one of that label it is in the panel (`Composition` keeps that name in `ControlTarget::Button`). In Map
//! mode it gets the click-to-learn layer like every other control; a pad / key / OSC press then makes it act as if
//! clicked the next time it's drawn (so it works while the button is on screen).

use std::collections::HashMap;

use eframe::egui;
use phosphor_engine::ControlTarget;

/// The panel being drawn (its `Tab` name), set by the dock around each panel.
const PANEL: &str = "phosphor_press_panel";
/// (panel, label) → how many drawn so far this frame.
const SEEN: &str = "phosphor_press_seen";
/// Mapped presses waiting for their button: key → when (seconds).
const PRESSED: &str = "phosphor_press_pressed";
/// Mapped values waiting for their slider: key → (0..1, when).
const SLID: &str = "phosphor_press_slid";
/// A press waits this long for its button to be drawn.
const WAIT_SECS: f64 = 1.0;
/// The right-clicked button's Learn menu: (its own menu's id, what it maps, where, which layer).
const LEARN_MENU: &str = "phosphor_press_learn_menu";

type LearnMenu = (egui::Id, ControlTarget, egui::Pos2, egui::LayerId);

/// Called by the dock around each panel (None after it).
pub(crate) fn set_panel(ctx: &egui::Context, panel: Option<(&str, &str)>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(PANEL), panel.map(|(k, t)| (k.to_string(), t.to_string()))));
}

/// Start of a frame: buttons are counted afresh, presses nobody drew in time are dropped.
pub(crate) fn begin_frame(ctx: &egui::Context, now: f64) {
    ctx.data_mut(|d| {
        d.remove_temp::<HashMap<(String, String), u32>>(egui::Id::new(SEEN));
        d.get_temp_mut_or_default::<HashMap<u64, f64>>(egui::Id::new(PRESSED)).retain(|_, t| now - *t < WAIT_SECS);
        d.get_temp_mut_or_default::<HashMap<u64, (f32, f64)>>(egui::Id::new(SLID)).retain(|_, (_, t)| now - *t < WAIT_SECS);
    });
}

/// A mapped knob / OSC value (0..1) arrived for this slider (`ControlTarget::Slider`); the newest wins.
pub(crate) fn slide(ctx: &egui::Context, key: u64, v: f32, now: f64) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<HashMap<u64, (f32, f64)>>(egui::Id::new(SLID)).insert(key, (v, now)));
    ctx.request_repaint();
}

/// The value waiting for slider `key`, if any (taken).
pub(crate) fn take_slide(ctx: &egui::Context, key: u64) -> Option<f32> {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<HashMap<u64, (f32, f64)>>(egui::Id::new(SLID)).remove(&key)).map(|(v, _)| v)
}

/// The stable key and shown name of the next control of `kind` labelled `label` in the panel being drawn (counted per
/// frame, so the second "Amount" in a panel is "#2"). Buttons and sliders share it.
pub(crate) fn slot(ui: &egui::Ui, kind: &str, label: &str) -> (u64, String) {
    let ctx = ui.ctx();
    let (panel, title) = ctx.data(|d| d.get_temp::<Option<(String, String)>>(egui::Id::new(PANEL))).flatten().unwrap_or_default();
    // Buttons keep their old keys (no kind in them): mappings saved before still find them.
    let counted = if kind.is_empty() { label.to_string() } else { format!("{kind}:{label}") };
    let n = ctx.data_mut(|d| {
        let seen = d.get_temp_mut_or_default::<HashMap<(String, String), u32>>(egui::Id::new(SEEN));
        let c = seen.entry((panel.clone(), counted.clone())).or_insert(0);
        *c += 1;
        *c - 1
    });
    (key_of(&panel, &counted, n), name_of(&title, label, n))
}

/// A mapped press arrived for this button.
pub(crate) fn press(ctx: &egui::Context, key: u64, now: f64) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<HashMap<u64, f64>>(egui::Id::new(PRESSED)).insert(key, now));
    ctx.request_repaint();
}

/// The stable key of the `n`th button labelled `label` in `panel`.
pub(crate) fn key_of(panel: &str, label: &str, n: u32) -> u64 {
    // (Saved in projects: a hash that never changes.)
    phosphor_engine::stable_hash(&[panel.as_bytes(), label.as_bytes(), &n.to_le_bytes()])
}

/// What people see for it in mapping lists: "Clips › Random clip" ("#2" for the second one).
pub(crate) fn name_of(panel_title: &str, label: &str, n: u32) -> String {
    let label = label.trim();
    let place = if panel_title.is_empty() { String::new() } else { format!("{panel_title} › ") };
    if n == 0 { format!("{place}{label}") } else { format!("{place}{label} #{}", n + 1) }
}

/// Makes a kit button mappable: Map mode layer, mapped look, and a waiting press turns into a click. Call right
/// after adding the button and before looking at `clicked()`.
pub(crate) fn mappable(ui: &egui::Ui, resp: &mut egui::Response, label: &str) {
    let label = label.trim();
    if label.is_empty() || !ui.is_visible() {
        return;
    }
    let (key, name) = slot(ui, "", label);
    let ctx = ui.ctx();
    if resp.enabled() && ctx.data_mut(|d| d.get_temp_mut_or_default::<HashMap<u64, f64>>(egui::Id::new(PRESSED)).remove(&key)).is_some() {
        resp.flags.insert(egui::response::Flags::FAKE_PRIMARY_CLICKED);
    }
    let target = ControlTarget::Button { key, name };
    crate::ui::mapped_badge(ui, resp, &target);
    if resp.secondary_clicked() {
        let at = resp.interact_pointer_pos().unwrap_or(resp.rect.left_bottom());
        let menu: LearnMenu = (egui::Popup::default_response_id(resp), target, at, resp.layer_id);
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(LEARN_MENU), Some(menu)));
    }
}

/// End of a frame: a right-clicked button without a right-click menu of its own gets Learn / map (buttons that have
/// one, like toggles with Map, keep theirs: it opened during the frame).
pub(crate) fn learn_menu(ctx: &egui::Context) {
    let id = egui::Id::new(LEARN_MENU);
    let Some((own, target, at, layer)) = ctx.data(|d| d.get_temp::<Option<LearnMenu>>(id)).flatten() else { return };
    let popup = egui::Id::new("phosphor_press_learn_popup");
    let fresh = own != egui::Id::NULL;
    if fresh {
        if egui::Popup::is_id_open(ctx, own) {
            ctx.data_mut(|d| d.remove_temp::<Option<LearnMenu>>(id));
            return;
        }
        egui::Popup::open_id(ctx, popup);
        ctx.data_mut(|d| d.insert_temp(id, Some((egui::Id::NULL, target.clone(), at, layer))));
    }
    let shown = egui::Popup::new(popup, ctx.clone(), at, layer).kind(egui::PopupKind::Menu).open_memory(None).show(|ui| crate::ui::learn_items(ui, &target));
    if shown.is_none() {
        ctx.data_mut(|d| d.remove_temp::<Option<LearnMenu>>(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_stable_and_tell_buttons_apart() {
        assert_eq!(key_of("Clips", "Random clip", 0), key_of("Clips", "Random clip", 0), "same every run");
        assert_ne!(key_of("Clips", "Random clip", 0), key_of("Clips", "Random clip", 1));
        assert_ne!(key_of("Clips", "Stop", 0), key_of("Layers", "Stop", 0));
        assert_eq!(name_of("Clips", "Stop", 1), "Clips › Stop #2");
        assert_eq!(name_of("", "GO", 0), "GO");
    }

    /// A mapped press makes the button report a click the next time it's drawn, once.
    #[test]
    fn a_press_clicks_the_button_once() {
        let ctx = egui::Context::default();
        let mut clicks = 0;
        for frame in 0..3 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                begin_frame(ui.ctx(), frame as f64 * 0.1);
                set_panel(ui.ctx(), Some(("Clips", "Clips")));
                if frame == 1 {
                    press(ui.ctx(), key_of("Clips", "Stop", 0), 0.1);
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    if crate::ui::command(ui, "Stop").clicked() {
                        clicks += 1;
                    }
                });
            });
            out.textures_delta.clear();
        }
        assert_eq!(clicks, 1);
    }

    /// A mapped knob moves a settings slider that has no control of its own (once it's drawn), and buttons keep
    /// the keys they had before sliders shared the counting.
    #[test]
    fn a_mapped_value_moves_a_plain_slider() {
        let ctx = egui::Context::default();
        let mut soft = 0.0f32;
        let key = key_of("Mapping", "slider:Soft edge", 0);
        for frame in 0..3 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                begin_frame(ui.ctx(), frame as f64 * 0.1);
                set_panel(ui.ctx(), Some(("Mapping", "Mapping")));
                if frame == 1 {
                    slide(ui.ctx(), key, 1.0, 0.1);
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    crate::ui::Level::range(&mut soft, 0.0, 0.5, "Soft edge").show(ui);
                    assert_eq!(slot(ui, "", "Stop").0, key_of("Mapping", "Stop", 0), "button keys unchanged");
                });
            });
            out.textures_delta.clear();
        }
        assert_eq!(soft, 0.5, "moved to the top of its range");
    }

    /// Right-clicking a plain button opens Learn / map for it; a button with its own right-click menu keeps that one.
    #[test]
    fn right_click_learns_a_button_without_a_menu() {
        fn right_click(own_menu: bool) -> bool {
            let ctx = egui::Context::default();
            let at = egui::pos2(30.0, 20.0);
            let mut learn_open = false;
            for frame in 0..4 {
                let mut input = egui::RawInput::default();
                input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0)));
                input.events.push(egui::Event::PointerMoved(at));
                if frame == 1 || frame == 2 {
                    input.events.push(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Secondary, pressed: frame == 1, modifiers: Default::default() });
                }
                let mut out = ctx.run_ui(input, |ui| {
                    begin_frame(ui.ctx(), frame as f64 * 0.1);
                    set_panel(ui.ctx(), Some(("Clips", "Clips")));
                    egui::CentralPanel::default().show(ui, |ui| {
                        let r = crate::ui::command(ui, "Stop all the clips");
                        if own_menu {
                            crate::ui::RightMenu::right_menu(&r, |ui| ui.label("its own"));
                        }
                    });
                    learn_menu(ui.ctx());
                    learn_open = egui::Popup::is_id_open(ui.ctx(), egui::Id::new("phosphor_press_learn_popup"));
                });
                out.textures_delta.clear();
            }
            learn_open
        }
        assert!(right_click(false), "plain button: Learn menu");
        assert!(!right_click(true), "button with its own menu: not ours");
    }
}
