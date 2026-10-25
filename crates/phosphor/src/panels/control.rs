//! MIDI/OSC settings and the mapping list.

use super::*;

pub(crate) fn control(ui: &mut egui::Ui, st: &mut State) {
    let control = &mut st.control;
    crate::ui::heading(ui, "MIDI");
    if control.midi_ports.is_empty() {
        hint(ui, "No MIDI inputs found. Plug in a controller and press Reconnect.");
    } else {
        hint(ui, format!("Listening on: {}", control.midi_ports.join(", ")));
    }
    crate::ui::toolbar(ui, |ui| {
        crate::ui::heading(ui, "OSC");
        guarded!(ui, control.osc_port, egui::DragValue::new(&mut control.osc_port).range(1024..=65535).prefix("port "));
        // More ports, typed as a list ("8000, 9001"); applied by Reconnect.
        let id = ui.id().with("osc_extra");
        let mut text = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| control.osc_extra_ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", "));
        if crate::ui::text_field(ui, &mut text, "also on ports…", Some(110.0)).on_hover_text("More ports to listen on, for apps that only send to their own port (e.g. 8000, 9001); press Reconnect").changed() {
            control.osc_extra_ports = text.split([',', ' ']).filter_map(|p| p.trim().parse::<u16>().ok()).filter(|p| *p >= 1024).collect();
        }
        ui.data_mut(|d| d.insert_temp(id, text));
        if crate::ui::command(ui, label(icon::ARROWS_CLOCKWISE, "Reconnect")).clicked() {
            control.reconnect();
        }
    });
    hint(ui, &control.osc_status);
    ui.horizontal_wrapped(|ui| {
        hint(ui, "TouchOSC: add a UDP connection to this address and port; Phosphor also appears in its Zeroconf list.");
        #[cfg(not(target_arch = "wasm32"))]
        if ui
            .button(label(icon::EXPORT, "Export TouchOSC layout…"))
            .on_hover_text("A ready-made TouchOSC layout from your Control Surface (or master, layers and columns if you haven't built one), already sending Phosphor's addresses")
            .clicked()
        {
            #[cfg(not(target_arch = "wasm32"))]
            st.dialogs.save_file(crate::dialogs::Purpose::TouchOsc, ("TouchOSC layout", &["tosc"]), "Phosphor.tosc");
        }
        if crate::ui::command(ui, label(icon::DOWNLOAD_SIMPLE, "Import TouchOSC layout…")).on_hover_text("Bring a TouchOSC layout in as Control Surface pages: same controls in the same places, each control's OSC address already mapped").clicked() {
            #[cfg(not(target_arch = "wasm32"))]
            st.dialogs.pick_file(crate::dialogs::Purpose::ImportTouchOsc, Some(("TouchOSC layout", &["tosc", "xml"])));
        }
    });
    let control = &mut st.control;
    crate::ui::toolbar(ui, |ui| {
        crate::ui::toggle(ui, &mut control.soft_takeover, "Soft takeover")
            .on_hover_text("A fader or knob only takes over once it reaches the value on screen (moves it past it), so the picture never jumps when the hardware was left somewhere else");
        #[cfg(not(target_arch = "wasm32"))]
        {
            let shown = if control.clock_out_port.is_empty() { "Off".to_string() } else { control.clock_out_port.clone() };
            crate::ui::dropdown(ui, "clock_out", 140.0).selected_text(format!("Clock out: {shown}")).show_ui(ui, |ui| {
                if crate::ui::pick_item(ui, control.clock_out_port.is_empty(), "Off").clicked() {
                    control.clock_out_port.clear();
                }
                for p in crate::control::clock_out::outputs() {
                    if crate::ui::pick_item(ui, control.clock_out_port == p, &p).clicked() {
                        control.clock_out_port = p;
                        control.clock_out_error = None;
                    }
                }
            })
            .response
            .on_hover_text("Send MIDI clock (24 a beat, with Start) to a MIDI output: drum machines, sequencers and other apps follow Phosphor's tempo");
            if let Some(e) = &control.clock_out_error {
                ui.label(RichText::new(format!("{} {e}", icon::WARNING)).small().color(Color32::from_rgb(230, 160, 60)));
            }
            let shown = if control.mtc_out_port.is_empty() { "Off".to_string() } else { control.mtc_out_port.clone() };
            crate::ui::dropdown(ui, "mtc_out", 140.0).selected_text(format!("Timecode out: {shown}")).show_ui(ui, |ui| {
                if crate::ui::pick_item(ui, control.mtc_out_port.is_empty(), "Off").clicked() {
                    control.mtc_out_port.clear();
                }
                for p in crate::control::clock_out::outputs() {
                    if crate::ui::pick_item(ui, control.mtc_out_port == p, &p).clicked() {
                        control.mtc_out_port = p;
                        control.mtc_out_error = None;
                    }
                }
                ui.separator();
                crate::ui::toggle(ui, &mut control.mtc_time_of_day, "Time of day").on_hover_text("Send the clock time instead of the show clock (Cues panel)");
                ui.separator();
                ui.label(RichText::new("LTC (timecode as sound)").small());
                let pairs = (crate::media::video::sound::output_channels() / 2).max(1) as u8;
                if crate::ui::pick_item(ui, control.ltc_out_pair.is_none(), "LTC off").clicked() {
                    control.ltc_out_pair = None;
                }
                for p in 0..pairs.max(control.ltc_out_pair.map_or(0, |p| p + 1)) {
                    let name = format!("LTC on outputs {}-{}", p as u32 * 2 + 1, p as u32 * 2 + 2);
                    if crate::ui::pick_item(ui, control.ltc_out_pair == Some(p), name).clicked() {
                        control.ltc_out_pair = Some(p);
                    }
                }
            })
            .response
            .on_hover_text("Send MIDI timecode (25 fps) to a MIDI output: the show clock, or the time of day. Lighting desks, DAWs and other apps follow it");
            if let Some(e) = &control.mtc_out_error {
                ui.label(RichText::new(format!("{} {e}", icon::WARNING)).small().color(Color32::from_rgb(230, 160, 60)));
            }
        }
        ui.menu_button(label(icon::SQUARES_FOUR, "Clip pad lights"), |ui| {
            ui.set_min_width(260.0);
            hint(ui, "Pads mapped to clips light up: one colour for an empty slot, one for a loaded clip, one for the clip playing. Grid controllers pick the colour by note velocity.");
            for (name, lights) in crate::control::CLIP_LIGHTS {
                if crate::ui::pick_item(ui, control.clip_lights == lights, name).clicked() {
                    control.clip_lights = lights;
                }
            }
            crate::ui::toolbar(ui, |ui| {
                for (k, what) in ["empty", "loaded", "playing"].iter().enumerate() {
                    guarded!(ui, control.clip_lights[k], egui::DragValue::new(&mut control.clip_lights[k]).range(0..=127).prefix(format!("{what} ")));
                }
            });
        });
        if control.soft_takeover {
            crate::ui::toggle(ui, &mut control.takeover_glide, "Glide")
                .on_hover_text("Instead of waiting for the fader to reach the value, the value glides to the fader (a smooth catch-up, no jump)");
            let mut pct = control.pickup_near * 100.0;
            if guarded!(ui, pct, egui::DragValue::new(&mut pct).range(0.5..=50.0).speed(0.1).max_decimals(1).prefix("within ").suffix("%"))
                .on_hover_text("How close a fader has to come to the value on screen to take over")
                .changed()
            {
                control.pickup_near = pct / 100.0;
            }
        }
        crate::ui::toggle(ui, &mut control.feedback, "Send feedback")
            .on_hover_text("Controllers follow Phosphor: values changed here (mouse, phone, other controllers) go back to TouchOSC faders and to MIDI controllers with motor faders or LED rings");
        ui.add_enabled_ui(control.feedback, |ui| {
            guarded!(ui, control.feedback_port, egui::DragValue::new(&mut control.feedback_port).range(1024..=65535).prefix("to port "))
                .on_hover_text("The port your OSC controller listens on (TouchOSC: the connection's receive port, 9000 by default)");
        });
    });
    #[cfg(not(target_arch = "wasm32"))]
    {
        let control = &mut st.control;
        crate::ui::toolbar(ui, |ui| {
            let mut on = control.dmx_in();
            if crate::ui::toggle(ui, &mut on, "DMX in")
                .on_hover_text("A lighting desk drives Phosphor: listen for Art-Net and sACN, then map any channel like a MIDI fader (right-click a control › Learn, move the desk's fader)")
                .changed()
            {
                control.set_dmx_in(on);
            }
            if on {
                hint(ui, control.dmx_status());
            }
        });
        // Elgato Stream Deck: keys and dials map like MIDI pads and knobs; mapped keys show their control's name.
        crate::ui::toolbar(ui, |ui| {
            let mut on = control.stream_deck_on();
            if crate::ui::toggle(ui, &mut on, "Stream Deck")
                .on_hover_text("Use an Elgato Stream Deck: right-click any control › Learn, press a key (or turn a Stream Deck + dial). Each mapped key shows the control's name, lit while it's on. Quit Elgato's own Stream Deck app first.")
                .changed()
            {
                control.set_stream_deck(on);
            }
            if on {
                hint(ui, control.stream_deck_status());
            }
        });
        // An Arduino board (StandardFirmata): its pins map like MIDI; PWM pins can be sent to (Send out).
        crate::ui::toolbar(ui, |ui| {
            ui.label("Arduino");
            let current = control.arduino_port();
            let mut pick = current.clone();
            crate::ui::dropdown(ui, "arduino_port", 200.0).selected_text(if current.is_empty() { "(none)".to_string() } else { current.clone() }).show_ui(ui, |ui| {
                crate::ui::pick_value(ui, &mut pick, String::new(), "(none)");
                for p in crate::serial::ports() {
                    crate::ui::pick_value(ui, &mut pick, p.clone(), p);
                }
            })
            .response
            .on_hover_text("A board running StandardFirmata (Arduino IDE › File › Examples › Firmata). Its analog inputs and pins then map like MIDI (right-click a control › Learn, turn the knob); Send out › Arduino drives its PWM pins.");
            if pick != current {
                control.set_arduino(&pick);
            }
            if !current.is_empty() {
                hint(ui, control.arduino_status());
            }
        });
    }
    monitor(ui, st);
    ui.separator();

    mappings_editor(ui, st);
}

/// The MIDI / OSC monitor: every message as it comes in (newest at the bottom), what it's mapped to, pause / clear.
fn monitor(ui: &mut egui::Ui, st: &mut State) {
    crate::ui::section(ui, "control_monitor", format!("{} Monitor: what your controllers send", icon::PULSE), true, |ui| {
        let comp = &st.comp;
        let control = &mut st.control;
        crate::ui::toolbar(ui, |ui| {
            let (resume, hold) = (label(icon::PLAY, "Resume"), label(icon::PAUSE, "Pause"));
            let pause = if control.monitor_paused { resume.clone() } else { hold.clone() };
            if steady_small_button(ui, pause, &[&resume, &hold]).clicked() {
                control.monitor_paused = !control.monitor_paused;
            }
            if crate::ui::small_command(ui, label(icon::TRASH, "Clear")).clicked() {
                control.recent.clear();
            }
            hint(ui, format!("{} messages", control.recent.len()));
        });
        if control.recent.is_empty() {
            hint(ui, "Nothing yet: move a knob, press a pad, or send OSC to this Mac.");
            return;
        }
        let now = web_time::Instant::now();
        crate::ui::column_scroll(ui).id_salt("monitor_list").max_height(180.0).stick_to_bottom(true).show(ui, |ui| {
            egui::Grid::new("monitor_grid").num_columns(4).spacing([12.0, 2.0]).striped(true).show(ui, |ui| {
                for (at, source, value) in &control.recent {
                    let ago = now.saturating_duration_since(*at).as_secs_f32();
                    ui.monospace(RichText::new(if ago < 60.0 { format!("{ago:4.1}s") } else { format!("{:3.0}m", ago / 60.0) }).small());
                    ui.monospace(RichText::new(source.label()).small());
                    ui.monospace(RichText::new(format!("{value:.3}")).small());
                    let mapped: Vec<String> = comp.control_maps.iter().filter(|m| m.source == *source).map(|m| crate::control::target_label(comp, &m.target)).collect();
                    if mapped.is_empty() {
                        ui.label(RichText::new("not mapped").small().color(Color32::GRAY));
                    } else {
                        ui.label(RichText::new(format!("{} {}", icon::ARROW_RIGHT, mapped.join(", "))).small().color(accent()));
                    }
                    ui.end_row();
                }
            });
        });
    });
}

/// Where a mapped control lives: the panel to open, the layer / group / comp to select (MASTER_ID = master), the
/// slice to select. None when it's gone.
fn where_is(comp: &phosphor_engine::Composition, target: &ControlTarget) -> Option<(Tab, Option<u64>, Option<u64>)> {
    use ControlTarget as T;
    let item = |id: u64| Some((Tab::Inspector, Some(id), None));
    match target {
        T::MasterFade => item(phosphor_engine::MASTER_ID),
        T::LayerOpacity(id) | T::LayerEnabled(id) | T::LayerSpeed(id) | T::LayerFxBypass(id) | T::LayerLoopIn(id) | T::LayerLoopOut(id) | T::LayerDirection(id) | T::LayerScrub(id) | T::LayerVolume(id) | T::LayerX(id) | T::LayerY(id) | T::LayerScale(id) | T::LayerRotation(id) => item(*id),
        T::GeneratorParam { layer, .. } | T::LayerJump { layer, .. } | T::LayerCorner { layer, .. } => item(*layer),
        T::LayerText(id) | T::LayerTextStep { layer: id, .. } => item(*id),
        T::TriggerClip { layer, .. } | T::LayerClipPick(layer) | T::LayerRandomClip(layer) => Some((Tab::Clips, Some(*layer), None)),
        T::TriggerColumn(_) => Some((Tab::Clips, None, None)),
        T::Menu(_) => None,
        T::EffectParam { effect, .. } | T::EffectMix(effect) | T::EffectEnabled(effect) => {
            let has = |chain: &[phosphor_engine::EffectState]| chain.iter().any(|e| e.id == *effect);
            if has(&comp.master_effects) {
                return item(phosphor_engine::MASTER_ID);
            }
            let owner = comp.layers.iter().find(|l| has(&l.effects)).map(|l| l.id).or_else(|| comp.groups.iter().find(|g| has(&g.effects)).map(|g| g.id)).or_else(|| comp.comps.iter().find(|c| has(&c.effects)).map(|c| c.id));
            if let Some(id) = owner {
                return item(id);
            }
            let slice = comp.outputs.iter().flat_map(|o| o.mapping.surfaces.iter()).find(|s| has(&s.effects))?;
            Some((Tab::Mapping, None, Some(slice.id)))
        }
        T::SliceOpacity(id) | T::SliceEnabled(id) => Some((Tab::Mapping, None, Some(*id))),
        T::Surface(_) | T::SurfaceY(_) | T::SurfaceBar { .. } => Some((Tab::Surface, None, None)),
        T::Modulator(_) => Some((Tab::Modulators, None, None)),
        T::Fixture { .. } => Some((Tab::Leds, None, None)),
        T::App(_) => None,
    }
}

/// Every controller mapping, editable: Listen for a new input, retype or re-listen an input, pick
/// the target, set the range, switch on/off, delete. A live meter shows each input as it moves.
fn mappings_editor(ui: &mut egui::Ui, st: &mut State) {
    use crate::app::ListenFor;
    let mut show: Option<(Tab, Option<u64>, Option<u64>)> = None;
    let State { control, comp, listen_for, map_draft, map_search, mapping_presets, mapping_preset_name, notice, selected, selected_surface, open_tab, .. } = st;
    // A heard input lands where the Listen was for.
    if let Some(src) = control.heard.take() {
        match listen_for.take() {
            Some(ListenFor::Replace(i)) => {
                if let Some(m) = comp.control_maps.get_mut(i) {
                    m.source = src;
                }
            }
            _ => *map_draft = Some((src, None)),
        }
    }
    let listening = control.listen_source;
    ui.horizontal(|ui| {
        crate::ui::heading(ui, "Mappings");
        hint(ui, format!("{}", comp.control_maps.len()));
        let picked = crate::ui::saves_menu(ui, &label(icon::FLOPPY_DISK, "Saved"), "Sets of mappings by name, e.g. one per controller (Load replaces this project's mappings; Undo brings them back).", mapping_presets, mapping_preset_name, "mapping set", || if comp.control_maps.is_empty() { String::new() } else { serde_json::to_string(&comp.control_maps).unwrap_or_default() });
        match picked {
            Some(crate::ui::SaveAction::Load(json)) => match serde_json::from_str::<Vec<phosphor_engine::ControlMap>>(&json) {
                Ok(maps) => comp.control_maps = maps,
                Err(e) => *notice = Some(format!("Couldn't load those mappings: {e}")),
            },
            Some(crate::ui::SaveAction::Error(e)) => *notice = Some(e),
            None => {}
        }
        if listening {
            ui.label(RichText::new(format!("{} Listening… touch a knob, pad or key, or send OSC", icon::EAR)).color(Color32::from_rgb(255, 205, 90)));
            ui.ctx().request_repaint();
            if crate::ui::command(ui, "Cancel").clicked() {
                control.listen_source = false;
                *listen_for = None;
            }
        } else if crate::ui::command(ui, label(icon::EAR, "Listen")).on_hover_text("Waits for the next input (MIDI knob/pad, OSC message or key), then lets you pick what it controls").clicked() {
            control.listen_source = true;
            *listen_for = Some(ListenFor::NewMapping);
        }
        if crate::ui::command(ui, label(icon::PLUS, "Add by hand")).clicked() {
            *map_draft = Some((phosphor_engine::ControlSource::MidiCc { channel: 0, cc: 1 }, None));
        }
        ui.add(egui::TextEdit::singleline(map_search).hint_text("Search mappings").desired_width(140.0));
    });
    let targets = crate::control::all_targets(comp);
    // The mapping being made: input heard (or typed) → pick a target → Add.
    let mut finish: Option<bool> = None;
    if let Some((src, target)) = map_draft.as_mut() {
        card_frame(ui, true).show(ui, |ui| {
            crate::ui::toolbar(ui, |ui| {
                ui.label(RichText::new("New").strong().color(accent()));
                source_editor(ui, "new_map_source", src);
                ui.label(icon::ARROW_RIGHT);
                if let Some(t) = target_picker(ui, "new_map_target", &targets, target.as_ref(), 220.0) {
                    *target = Some(t);
                }
                if crate::ui::command_if(ui, target.is_some(), "Add").clicked() {
                    finish = Some(true);
                }
                if crate::ui::small_command(ui, icon::X).clicked() {
                    finish = Some(false);
                }
            });
        });
    }
    if let Some(add) = finish {
        if add && let Some((src, Some(t))) = map_draft.take() {
            comp.control_maps.push(phosphor_engine::ControlMap::new(src, t));
        }
        *map_draft = None;
    }

    let last = control.last.as_ref().map(|m| (m.source.clone(), m.value));
    let q = map_search.to_lowercase();
    let mut remove = None;
    crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
        if comp.control_maps.is_empty() && map_draft.is_none() {
            hint(ui, format!("None yet. Press Listen and move a control, or right-click any slider or button {} Listen, or use MAP in the top bar.", icon::ARROW_RIGHT));
        }
        let places: Vec<_> = comp.control_maps.iter().map(|m| where_is(comp, &m.target)).collect();
        egui::Grid::new("mappings").num_columns(6).striped(true).spacing([6.0, 3.0]).show(ui, |ui| {
            for (i, m) in comp.control_maps.iter_mut().enumerate() {
                let name = targets.iter().find(|(t, _)| *t == m.target).map_or("(removed)", |(_, n)| n.as_str());
                if !q.is_empty() && !name.to_lowercase().contains(&q) && !m.source.label().to_lowercase().contains(&q) {
                    continue;
                }
                crate::ui::toggle(ui, &mut m.enabled, "").on_hover_text("On / off");
                crate::ui::toolbar(ui, |ui| {
                    source_editor(ui, ("map_source", i), &mut m.source);
                    let relisten = *listen_for == Some(ListenFor::Replace(i)) && control.listen_source;
                    if crate::ui::pick_item(ui, relisten, icon::EAR).on_hover_text("Listen again for this mapping's input").clicked() {
                        control.listen_source = true;
                        *listen_for = Some(ListenFor::Replace(i));
                    }
                });
                // Live meter: lights up and shows the value while this input moves.
                let live = last.as_ref().filter(|(s, _)| *s == m.source).map(|(_, v)| *v);
                let (r, _) = ui.allocate_exact_size(egui::vec2(36.0, 8.0), Sense::hover());
                ui.painter().rect_filled(r, 2.0, Color32::from_gray(35));
                if let Some(v) = live {
                    let mut fill = r;
                    fill.set_width(r.width() * v.clamp(0.0, 1.0));
                    ui.painter().rect_filled(fill, 2.0, accent());
                }
                crate::ui::toolbar(ui, |ui| {
                    if let Some(t) = target_picker(ui, ("map_target", i), &targets, Some(&m.target), 220.0) {
                        m.target = t;
                    }
                    let place = places.get(i).copied().flatten();
                    if crate::ui::small_command_if(ui, place.is_some(), icon::CROSSHAIR).on_hover_text("Show this control (selects its layer and opens its panel)").clicked() {
                        show = place;
                    }
                });
                crate::ui::toolbar(ui, |ui| {
                    range_editor(ui, &mut m.range);
                    value_fx_menu(ui, ("map_fx", i), &mut m.fx);
                });
                if crate::ui::small_command(ui, icon::TRASH).on_hover_text("Delete").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    });
    if let Some(i) = remove {
        comp.control_maps.remove(i);
        *listen_for = None;
    }
    if let Some((tab, item, slice)) = show {
        if item.is_some() {
            *selected = item;
        }
        if slice.is_some() {
            *selected_surface = slice;
        }
        *open_tab = Some(tab);
    }
}

/// The ~ menu on a mapping: effects its value goes through (smooth, steps, curve, formula), in order.
fn value_fx_menu(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, fx: &mut Vec<phosphor_engine::ValueFx>) {
    use phosphor_engine::ValueFx;
    let text = if fx.is_empty() { RichText::new(icon::WAVE_SINE) } else { RichText::new(format!("{} {}", icon::WAVE_SINE, fx.len())).color(accent()) };
    ui.menu_button(text, |ui| {
        ui.set_min_width(260.0);
        crate::ui::heading(ui, "On the way in");
        hint(ui, "The controller's value goes through these, top to bottom, before its range.");
        let mut remove = None;
        for (k, f) in fx.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                crate::ui::heading(ui, f.name());
                match f {
                    ValueFx::Smooth(s) => {
                        guarded!(ui, *s, egui::DragValue::new(s).range(0.02..=10.0).speed(0.01).suffix(" s")).on_hover_text("Glide to each new value over this long");
                    }
                    ValueFx::Steps(n) => {
                        guarded!(ui, *n, egui::DragValue::new(n).range(2..=64)).on_hover_text("Snap to this many steps");
                    }
                    ValueFx::Curve(c) => {
                        guarded!(ui, *c, egui::DragValue::new(c).range(0.1..=8.0).speed(0.02)).on_hover_text("Above 1: slow then fast; below 1: fast then slow");
                    }
                    ValueFx::Formula(f) => {
                        text_field(ui, f, "1 - $VAL", Some(140.0)).on_hover_text("$VAL is the value (0..1); + - * / %, pow, sin, cos (a turn per 1.0), abs, min, max, clamp, mix, fract, floor…");
                    }
                    ValueFx::Delay(s) => {
                        guarded!(ui, *s, egui::DragValue::new(s).range(0.02..=30.0).speed(0.01).suffix(" s")).on_hover_text("Each move lands this much later");
                    }
                    ValueFx::Threshold(t) => {
                        guarded!(ui, *t, egui::DragValue::new(t).range(0.0..=1.0).speed(0.005).max_decimals(2)).on_hover_text("On (full) at or above this, off below: a fader becomes a switch");
                    }
                    ValueFx::Clamp(lo, hi) => {
                        guarded!(ui, *lo, egui::DragValue::new(lo).range(0.0..=1.0).speed(0.005).max_decimals(2).prefix("from ")).on_hover_text("Lowest value it can send");
                        guarded!(ui, *hi, egui::DragValue::new(hi).range(0.0..=1.0).speed(0.005).max_decimals(2).prefix("to ")).on_hover_text("Highest value it can send");
                    }
                    ValueFx::Repeat(n) => {
                        guarded!(ui, *n, egui::DragValue::new(n).range(1..=32).suffix("×")).on_hover_text("The range repeats this many times along the fader");
                    }
                    ValueFx::Randomize(a) => {
                        guarded!(ui, *a, egui::DragValue::new(a).range(0.0..=1.0).speed(0.005).max_decimals(2)).on_hover_text("Up to this much random wobble on each value");
                    }
                    ValueFx::Fall(s) => {
                        guarded!(ui, *s, egui::DragValue::new(s).range(0.02..=10.0).speed(0.01).suffix(" s")).on_hover_text("Rises land at once; drops glide down over this long (peak hold)");
                    }
                    ValueFx::Endless(s) => {
                        guarded!(ui, *s, egui::DragValue::new(s).range(0.001..=0.25).speed(0.0005).max_decimals(3).prefix("per click ")).on_hover_text("For knobs that turn forever (relative MIDI: 1–63 up, 65–127 down): how far one click moves the value (0.01 = 1%)");
                    }
                    ValueFx::DeadBand(d) => {
                        guarded!(ui, *d, egui::DragValue::new(d).range(0.001..=0.25).speed(0.0005).max_decimals(3)).on_hover_text("Changes smaller than this are ignored (a jittery fader or sensor)");
                    }
                    ValueFx::Lookup(points) => {
                        // Draw the output: left = fader at 0, right = fader at full.
                        crate::ui::step_bars(ui, points, None, false, 40.0);
                    }
                }
                if crate::ui::small_command(ui, icon::X).clicked() {
                    remove = Some(k);
                }
            });
        }
        if let Some(k) = remove {
            fx.remove(k);
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let fresh = [
                ValueFx::Smooth(0.3),
                ValueFx::Steps(4),
                ValueFx::Curve(2.0),
                ValueFx::Formula("1 - $VAL".into()),
                ValueFx::Delay(0.5),
                ValueFx::Threshold(0.5),
                ValueFx::Clamp(0.0, 1.0),
                ValueFx::Repeat(2),
                ValueFx::Randomize(0.1),
                ValueFx::Fall(0.5),
                ValueFx::Lookup((0..9).map(|i| i as f32 / 8.0).collect()),
                ValueFx::Endless(0.01),
                ValueFx::DeadBand(0.02),
            ];
            for f in fresh {
                let name = f.name();
                if crate::ui::small_command(ui, label(icon::PLUS, name)).clicked() {
                    fx.push(f);
                }
            }
        });
    })
    .response
    .on_hover_text("Smooth, steps, curve or a formula on this mapping's value");
    let _ = id;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_controls_are_found() {
        let mut comp = phosphor_engine::Composition::starter();
        let layer = comp.add_layer().id;
        let fx = comp.new_effect("invert").unwrap();
        let fid = fx.id;
        comp.layers.iter_mut().find(|l| l.id == layer).unwrap().effects.push(fx);
        assert_eq!(where_is(&comp, &ControlTarget::EffectMix(fid)), Some((Tab::Inspector, Some(layer), None)));
        assert_eq!(where_is(&comp, &ControlTarget::LayerJump { layer, forward: true }), Some((Tab::Inspector, Some(layer), None)));
        assert_eq!(where_is(&comp, &ControlTarget::MasterFade).map(|p| p.1), Some(Some(phosphor_engine::MASTER_ID)));
        assert_eq!(where_is(&comp, &ControlTarget::EffectMix(999_999)), None, "gone");
    }
}
