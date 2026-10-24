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
        let go = big_action_button(ui, Act::Go, format!("{} GO", icon::PLAY))
            .on_hover_text("Next cue · right-click to map a pad, key or OSC");
        map_menu(ui, &go, ControlTarget::App(AppControl::CueGo));
        if go.clicked() {
            st.cue_next(now);
        }
        let back = crate::ui::command(ui, label(icon::SKIP_BACK, "Back")).on_hover_text("Previous cue");
        map_menu(ui, &back, ControlTarget::App(AppControl::CueBack));
        if back.clicked() {
            st.cue_back(now);
        }
        let random = crate::ui::command(ui, label(icon::SHUFFLE, "Random")).on_hover_text("A random cue (not the one on now)");
        map_menu(ui, &random, ControlTarget::App(AppControl::CueRandom));
        if random.clicked() {
            st.cue_random(now);
        }
        if crate::ui::command(ui, label(icon::RECORD, "Record cue")).on_hover_text("Save how every layer looks now (sources, on/off, opacity, blend, values, effects) as the next cue").clicked() {
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
                let (pause, start) = (label(icon::PAUSE, "Pause"), label(icon::PLAY, "Start"));
                let text = if clock.running() { pause.clone() } else { start.clone() };
                if steady_button(ui, text, &[&pause, &start]).clicked() {
                    clock.toggle(now);
                }
                if crate::ui::command(ui, icon::ARROW_COUNTER_CLOCKWISE).on_hover_text("Back to 00:00:00").clicked() {
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
    crate::ui::toolbar(ui, |ui| {
        if let Some((next, secs)) = st.next_cue_in(now) {
            let name = st.comp.cues.cues.get(next).map_or(String::new(), |c| c.name.clone());
            ui.label(RichText::new(format!("{} next: {} \"{name}\" in {}", icon::HOURGLASS, next + 1, format_tc(secs))).color(accent()));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        #[cfg(not(target_arch = "wasm32"))]
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::ui::small_command(ui, label(icon::UPLOAD_SIMPLE, "Save CSV")).on_hover_text("Save the cue names and timings as a table (spreadsheet)").clicked() {
                st.dialogs.save_file(crate::dialogs::Purpose::ExportCues, ("CSV", &["csv"]), "Cues.csv");
            }
            if crate::ui::small_command(ui, label(icon::DOWNLOAD_SIMPLE, "Read CSV")).on_hover_text("Read names and timings from a table into the cues, in order (the looks stay as recorded)").clicked() {
                st.dialogs.pick_file(crate::dialogs::Purpose::ImportCues, Some(("CSV", &["csv", "txt"])));
            }
        });
    });
    // Installations: a cue when the project opens, and one when nobody's touched anything for a while.
    let names: Vec<(u64, String)> = st.comp.cues.cues.iter().map(|c| (c.id, c.name.clone())).collect();
    let cue_pick = |ui: &mut egui::Ui, salt: &str, id: &mut u64| {
        let shown = names.iter().find(|(i, _)| i == id).map_or("none".to_string(), |(_, n)| n.clone());
        ComboBox::from_id_salt(salt).width(110.0).selected_text(shown).show_ui(ui, |ui| {
            if crate::ui::pick_item(ui, *id == 0, "none").clicked() {
                *id = 0;
            }
            for (cid, n) in &names {
                if crate::ui::pick_item(ui, id == cid, n).clicked() {
                    *id = *cid;
                }
            }
        });
    };
    crate::ui::field(ui, "When the project opens", |ui| {
        cue_pick(ui, "open_cue", &mut st.comp.cues.open_cue);
        ui.separator();
        let mut idle = st.comp.cues.idle_minutes > 0.0;
        if toggle(ui, &mut idle, "When nobody's touched anything for").on_hover_text("Installations: no mouse, keys or controller for a while, go to this cue (once, until someone's back)").changed() {
            st.comp.cues.idle_minutes = if idle { 10.0 } else { 0.0 };
        }
        if idle {
            guarded!(ui, st.comp.cues.idle_minutes, egui::DragValue::new(&mut st.comp.cues.idle_minutes).range(0.5..=600.0).speed(0.2).suffix(" min"));
            cue_pick(ui, "idle_cue", &mut st.comp.cues.idle_cue);
        }
    });
    #[cfg(not(target_arch = "wasm32"))]
    crate::ui::field(ui, "Calendar", |ui| {
        if st.comp.cues.calendar.is_empty() {
            if crate::ui::small_command(ui, label(icon::CALENDAR, "Choose a calendar (.ics)…")).on_hover_text("Events in a calendar (exported from Calendar, Google, Outlook) start the cue with the same name, on time; repeating events too").clicked() {
                st.dialogs.pick_file(crate::dialogs::Purpose::CueCalendar, Some(("Calendar", &["ics"])));
            }
        } else {
            let name = std::path::Path::new(&st.comp.cues.calendar).file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
            hint(ui, format!("{} (events start the cues named like them)", name));
            if crate::ui::small_command(ui, icon::X).on_hover_text("No calendar").clicked() {
                st.comp.cues.calendar.clear();
            }
        }
    });
    hint(ui, "Cues remember every layer and every mapping slice (on/off, opacity, corners). GO fades to the next one; sources change through each layer's transition, slices glide into place.");
    ui.add_space(4.0);
    let mut fire = None;
    let mut update = None;
    let mut remove = None;
    let current = st.cue_run.current;
    // For "Changes": every layer (name, and how it is now, for adding it to a cue).
    let layer_now: Vec<(u64, String)> = st.comp.layers.iter().rev().map(|l| (l.id, l.name.clone())).collect();
    let mut add_layer: Option<(usize, u64)> = None;
    let progress = st.cue_run.fade.as_ref().map(|f| (f.cue, if f.secs > 0.0 { ((now as f32 - f.start) / f.secs).clamp(0.0, 1.0) } else { 1.0 }));
    // Group names in use, for each cue's Group menu.
    let mut group_names: Vec<String> = st.comp.cues.cues.iter().map(|c| c.group.clone()).filter(|g| !g.is_empty()).collect();
    group_names.dedup();
    crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
        let mut prev_group = String::new();
        for (i, cue) in st.comp.cues.cues.iter_mut().enumerate() {
            let is_current = current == Some(i);
            // A group starts here: its name, and Go (its first cue).
            if !cue.group.is_empty() && cue.group != prev_group {
                crate::ui::toolbar(ui, |ui| {
                    crate::ui::heading(ui, format!("{} {}", icon::FOLDER_OPEN, cue.group));
                    if crate::ui::small_command(ui, label(icon::PLAY, "Go")).on_hover_text("Start this group (its first cue)").clicked() {
                        fire = Some(i);
                    }
                });
            }
            prev_group.clone_from(&cue.group);
            card_frame(ui, is_current).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{}", i + 1)).monospace().strong().color(if is_current { accent() } else { Color32::GRAY }));
                    if crate::ui::small_command(ui, icon::PLAY).on_hover_text("Go to this cue").clicked() {
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
                    let group_title = if cue.group.is_empty() { "Group".to_string() } else { cue.group.clone() };
                    ui.menu_button(label(icon::FOLDER, &group_title), |ui| {
                        hint(ui, "Cues with the same group name, one after another, are a group (a song, a scene): it gets a heading and a Go button.");
                        crate::ui::text_field(ui, &mut cue.group, "group name (empty = none)", Some(180.0));
                        for g in &group_names {
                            if crate::ui::pick_item(ui, cue.group == *g, g).clicked() {
                                cue.group.clone_from(g);
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button(label(icon::FUNNEL, "Changes"), |ui| {
                        ui.set_min_width(240.0);
                        hint(ui, "What going to this cue changes. Leave sources off to keep the media playing and only glide the numbers.");
                        toggle(ui, &mut cue.sets_sources, "Sources (clips, generators)");
                        toggle(ui, &mut cue.sets_master, "Master fade");
                        toggle(ui, &mut cue.sets_slices, "Mapping slices");
                        ui.separator();
                        ui.label(RichText::new("Layers").small());
                        for (id, name) in &layer_now {
                            let mut on = cue.layers.iter().any(|c| c.id == *id);
                            if toggle(ui, &mut on, name).on_hover_text("On: the cue sets this layer (as it is now, when you switch it on)").changed() {
                                if on {
                                    add_layer = Some((i, *id));
                                } else {
                                    cue.layers.retain(|c| c.id != *id);
                                }
                            }
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::ui::small_command(ui, icon::TRASH).on_hover_text("Delete cue").clicked() {
                            remove = Some(i);
                        }
                        if crate::ui::small_command(ui, icon::ARROWS_CLOCKWISE).on_hover_text("Re-record: save how everything looks now into this cue").clicked() {
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
    if let Some((i, id)) = add_layer
        && let Some(l) = st.comp.layers.iter().find(|l| l.id == id)
    {
        let snap = phosphor_engine::CueLayer::of(l);
        if let Some(c) = st.comp.cues.cues.get_mut(i) {
            c.layers.push(snap);
        }
    }
    if let Some(i) = fire {
        st.go_cue(i, now);
    }
    if let Some(i) = update {
        let fresh = st.comp.record_cue(String::new());
        if let Some(c) = st.comp.cues.cues.get_mut(i) {
            // The layers it already sets (a cue narrowed to some layers stays that way).
            let keep: Vec<u64> = c.layers.iter().map(|l| l.id).collect();
            c.layers = fresh.layers.into_iter().filter(|l| keep.contains(&l.id)).collect();
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
