//! Cues panel: record looks as cues, step through them with GO (or a controller), let them follow on by
//! themselves, or fire them on timecode (show clock or MIDI timecode).

use phosphor_engine::{AppControl, TimecodeSource};

use super::*;
use crate::app::format_tc;

/// "every day at 18:00" for installations: the time, and which days.
fn daily_editor(ui: &mut egui::Ui, cue: &mut phosphor_engine::Cue) {
    let mut on = cue.daily.is_some();
    if toggle(ui, &mut on, label(icon::CALENDAR, "daily")).on_hover_text("Fire every day at a time on this computer's clock (installations: on at 18:00, off at 02:00)").changed() {
        cue.daily = on.then_some(18 * 3600);
    }
    let Some(t) = &mut cue.daily else { return };
    let mut minutes = (*t / 60) as f64;
    if guarded!(ui, minutes, egui::DragValue::new(&mut minutes).range(0.0..=1439.0).speed(1.0).custom_formatter(|v, _| format!("{:02}:{:02}", v as u32 / 60, v as u32 % 60)).custom_parser(|s| {
        let (h, m) = s.trim().split_once(':')?;
        Some((h.trim().parse::<u32>().ok()?.min(23) * 60 + m.trim().parse::<u32>().ok()?.min(59)) as f64)
    }))
    .changed()
    {
        *t = minutes as u32 * 60;
    }
    // Days: all lit = every day (stored as 0); click one to leave it out or put it back.
    for (bit, day) in ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"].iter().enumerate() {
        let all = if cue.days == 0 { 0x7f } else { cue.days };
        let mut on = all & (1 << bit) != 0;
        if toggle(ui, &mut on, *day).on_hover_text("The days it fires on").changed() {
            let days = if on { all | (1 << bit) } else { all & !(1 << bit) };
            // None left would never fire: back to every day.
            cue.days = if days == 0x7f || days == 0 { 0 } else { days };
        }
    }
}

pub(crate) fn cues(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    ui.horizontal_wrapped(|ui| {
        let go = ui.add(egui::Button::new(RichText::new(format!("{} GO", icon::PLAY)).strong().size(16.0)).min_size(egui::vec2(80.0, 30.0)).fill(accent().gamma_multiply(0.35)))
            .on_hover_text("Next cue · right-click to map a pad, key or OSC");
        map_menu(ui, &go, ControlTarget::App(AppControl::CueGo));
        if go.clicked() {
            st.cue_next(now);
        }
        let back = ui.button(label(icon::SKIP_BACK, "Back")).on_hover_text("Previous cue");
        map_menu(ui, &back, ControlTarget::App(AppControl::CueBack));
        if back.clicked() {
            st.cue_back(now);
        }
        let random = ui.button(label(icon::SHUFFLE, "Random")).on_hover_text("A random cue (not the one on now)");
        map_menu(ui, &random, ControlTarget::App(AppControl::CueRandom));
        if random.clicked() {
            st.cue_random(now);
        }
        if ui.button(label(icon::RECORD, "Record cue")).on_hover_text("Save how every layer looks now (sources, on/off, opacity, blend, values, effects) as the next cue").clicked() {
            let n = st.comp.cues.cues.len() + 1;
            let cue = st.comp.record_cue(format!("Cue {n}"));
            let at = st.cue_run.current.map_or(st.comp.cues.cues.len(), |c| c + 1);
            st.comp.cues.cues.insert(at.min(st.comp.cues.cues.len()), cue);
            st.cue_run.current = Some(at.min(st.comp.cues.cues.len() - 1));
        }
        ui.separator();
        choice_combo(ui, "cue_timecode", &mut st.comp.cues.timecode, 130.0, false);
        match st.comp.cues.timecode {
            TimecodeSource::Off => {}
            TimecodeSource::ShowClock => {
                let clock = &mut st.cue_run.clock;
                let text = if clock.running() { label(icon::PAUSE, "Pause") } else { label(icon::PLAY, "Start") };
                if ui.button(text).clicked() {
                    clock.toggle(now);
                }
                if ui.button(icon::ARROW_COUNTER_CLOCKWISE).on_hover_text("Back to 00:00:00").clicked() {
                    clock.reset(now);
                }
            }
            TimecodeSource::Midi | TimecodeSource::Ltc => {}
        }
        match st.timecode(now) {
            Some(t) => {
                ui.label(RichText::new(format_tc(t)).monospace().strong().color(accent()));
            }
            None if st.comp.cues.timecode == TimecodeSource::Midi => {
                hint(ui, "waiting for MIDI timecode…");
            }
            None if st.comp.cues.timecode == TimecodeSource::Ltc => {
                hint(ui, "waiting for LTC on the audio input (pick the interface in the Audio panel)…");
            }
            None => {}
        }
    });
    ui.horizontal(|ui| {
        if let Some((next, secs)) = st.next_cue_in(now) {
            let name = st.comp.cues.cues.get(next).map_or(String::new(), |c| c.name.clone());
            ui.label(RichText::new(format!("{} next: {} \"{name}\" in {}", icon::HOURGLASS, next + 1, format_tc(secs))).color(accent()));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        #[cfg(not(target_arch = "wasm32"))]
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button(label(icon::UPLOAD_SIMPLE, "Save CSV")).on_hover_text("Save the cue names and timings as a table (spreadsheet)").clicked() {
                st.dialogs.save_file(crate::dialogs::Purpose::ExportCues, ("CSV", &["csv"]), "Cues.csv");
            }
            if ui.small_button(label(icon::DOWNLOAD_SIMPLE, "Read CSV")).on_hover_text("Read names and timings from a table into the cues, in order (the looks stay as recorded)").clicked() {
                st.dialogs.pick_file(crate::dialogs::Purpose::ImportCues, Some(("CSV", &["csv", "txt"])));
            }
        });
    });
    hint(ui, "Cues remember every layer and every mapping slice (on/off, opacity, corners). GO fades to the next one; sources change through each layer's transition, slices glide into place.");
    ui.add_space(4.0);
    let mut fire = None;
    let mut update = None;
    let mut remove = None;
    let current = st.cue_run.current;
    let progress = st.cue_run.fade.as_ref().map(|f| (f.cue, if f.secs > 0.0 { ((now as f32 - f.start) / f.secs).clamp(0.0, 1.0) } else { 1.0 }));
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (i, cue) in st.comp.cues.cues.iter_mut().enumerate() {
            let is_current = current == Some(i);
            card_frame(ui, is_current).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{}", i + 1)).monospace().strong().color(if is_current { accent() } else { Color32::GRAY }));
                    if ui.small_button(icon::PLAY).on_hover_text("Go to this cue").clicked() {
                        fire = Some(i);
                    }
                    ui.add(egui::TextEdit::singleline(&mut cue.name).desired_width(140.0));
                    guarded!(ui, cue.fade, egui::DragValue::new(&mut cue.fade).range(0.0..=60.0).speed(0.05).prefix("fade ").suffix(" s"));
                    let mut follows = cue.follow > 0.0;
                    if toggle(ui, &mut follows, "then next").on_hover_text("Go to the next cue by itself after this many seconds").changed() {
                        cue.follow = if follows { 5.0 } else { 0.0 };
                    }
                    if follows {
                        guarded!(ui, cue.follow, egui::DragValue::new(&mut cue.follow).range(0.1..=3600.0).speed(0.1).prefix("after ").suffix(" s"));
                    }
                    let mut timed = cue.at.is_some();
                    if toggle(ui, &mut timed, label(icon::CLOCK, "at")).on_hover_text("Fire at a timecode (choose the timecode at the top)").changed() {
                        cue.at = timed.then_some(0.0);
                    }
                    if let Some(at) = &mut cue.at {
                        let mut secs = *at as f64;
                        if guarded!(ui, secs, egui::DragValue::new(&mut secs).range(0.0..=86_400.0).speed(0.1).custom_formatter(|v, _| format_tc(v)).custom_parser(parse_tc)).changed() {
                            *at = secs as f32;
                        }
                    }
                    daily_editor(ui, cue);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::TRASH).on_hover_text("Delete cue").clicked() {
                            remove = Some(i);
                        }
                        if ui.small_button(icon::ARROWS_CLOCKWISE).on_hover_text("Re-record: save how everything looks now into this cue").clicked() {
                            update = Some(i);
                        }
                        hint(ui, if cue.slices.is_empty() { format!("{} layers", cue.layers.len()) } else { format!("{} layers · {} slices", cue.layers.len(), cue.slices.len()) });
                    });
                });
                if let Some((c, p)) = progress
                    && c == i
                    && p < 1.0
                {
                    ui.add(crate::ui::meter(p).desired_height(4.0));
                }
            });
        }
        if st.comp.cues.cues.is_empty() {
            hint(ui, "Set up a look, press Record cue; change things, record again. Then GO steps through them.");
        }
    });
    if let Some(i) = fire {
        st.go_cue(i, now);
    }
    if let Some(i) = update {
        let fresh = st.comp.record_cue(String::new());
        if let Some(c) = st.comp.cues.cues.get_mut(i) {
            c.layers = fresh.layers;
            c.master_fade = fresh.master_fade;
        }
    }
    if let Some(i) = remove {
        st.comp.cues.cues.remove(i);
        st.cue_run.current = st.cue_run.current.and_then(|c| if c == i { None } else if c > i { Some(c - 1) } else { Some(c) });
    }
}

/// "1:02:03.5", "2:03" or plain seconds → seconds.
fn parse_tc(text: &str) -> Option<f64> {
    text.trim().split(':').try_fold(0.0, |acc, part| part.trim().parse::<f64>().ok().map(|v| acc * 60.0 + v))
}
