//! Movie sound volume, audio input, analysis and beat detector.

use super::*;

pub(crate) fn audio(ui: &mut egui::Ui, st: &mut State) {
    ui.label(RichText::new(format!("{} Sound out", icon::SPEAKER_SIMPLE_HIGH)).strong());
    sound_out(ui, &mut st.comp);
    ui.separator();
    let State { audio, notice, mod_inputs, clock, .. } = st;
    let follow_audio = &mut clock.follow_audio;
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
            if ui.selectable_label(audio.is_clips(), "Clip sound (what Phosphor plays)").on_hover_text("The sound of the movies playing in Phosphor: visuals react to the music you play from the grid, no cable or mic").clicked() {
                audio.use_clips();
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
        if toggle(ui, &mut on, text).clicked() {
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
    ui.horizontal(|ui| {
        Level::range(&mut audio.gain, 0.1, 10.0, "Input gain").log().default(1.0).target(ControlTarget::App(phosphor_engine::AppControl::AudioGain)).tip("How much the input is boosted before the visuals hear it").show(ui);
    });
    ui.horizontal(|ui| {
        Level::range(&mut audio.envelope.attack, 0.0, 0.5, "Attack (s)").default(0.0).tip("How fast the levels rise: 0 = instantly (punchy), higher = smoother swells").show(ui);
    });
    ui.horizontal(|ui| {
        Level::range(&mut audio.envelope.release, 0.02, 2.0, "Release (s)").default(0.15).tip("How fast the levels fall after a hit: short = flickery, long = slow fades").show(ui);
    });
    ui.separator();
    for (name, v) in [
        ("Level", mod_inputs.audio_level),
        ("Low", mod_inputs.audio_low),
        ("Mid", mod_inputs.audio_mid),
        ("High", mod_inputs.audio_high),
    ] {
        ui.add(crate::ui::meter(v).text(name).fill(accent().gamma_multiply(0.6)));
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
            p.rect_filled(r, 1.0, accent().gamma_multiply(0.4 + 0.6 * v));
        }
    }
    ui.add_space(6.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} Beat detector", icon::HEARTBEAT)).strong());
        let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), Sense::hover());
        let pulse = mod_inputs.audio_beat;
        ui.painter().circle_filled(r.center(), 4.0 + 5.0 * pulse, if pulse > 0.05 { Color32::from_rgb(255, 196, 70) } else { Color32::from_gray(50) });
        let an = audio.analysis();
        if an.bpm > 0.0 {
            ui.monospace(format!("{:.1} BPM", an.bpm));
            hint(ui, format!("{:.0}% steady", an.confidence * 100.0));
        } else {
            hint(ui, "listening for a beat…");
        }
    });
    ui.horizontal(|ui| {
        Level::range(&mut audio.beats.sensitivity, 1.1, 3.0, "Kick threshold").default(1.5).target(ControlTarget::App(phosphor_engine::AppControl::KickThreshold)).tip("Lower catches softer kicks; higher ignores everything but strong hits").show(ui);
    });
    ui.horizontal(|ui| {
        let r = toggle(ui, follow_audio, label(icon::METRONOME, "Follow audio"))
            .on_hover_text("Lock Phosphor's tempo and beat to the music, so On beat / On bar launches land on the kick");
        map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::FollowAudio));
        hint(ui, "Also: Audio beat (kick) under any slider's wave button.");
    });
    ui.add_space(4.0);
    hint(ui, format!("Press {} next to any slider to drive it with these signals or an LFO.", icon::WAVE_SINE));
}

/// Mute all, master volume and levelling for everything Phosphor plays (Audio panel).
fn sound_out(ui: &mut egui::Ui, comp: &mut phosphor_engine::Composition) {
    use phosphor_engine::AppControl;
    ui.horizontal(|ui| {
        let r = super::inspector::mute_button(ui, &mut comp.sound_muted, "all sound");
        map_menu(ui, &r, ControlTarget::App(AppControl::MuteSound));
        Level::percent(&mut comp.volume, "Master volume").max(2.0).target(ControlTarget::App(AppControl::MasterVolume)).tip("Every movie's sound (each layer has its own volume too)").show(ui);
    });
    if comp.sound_muted {
        hint(ui, "All sound is off. Streams and recordings started now have no audio; one already running keeps its audio device until it restarts.");
    }
    let r = crate::ui::toggle(ui, &mut comp.sound_auto_level, "Even out volume levels").on_hover_text("Each movie is brought towards the same loudness as it plays (up to 4× up or down; silence isn't boosted)");
    map_menu(ui, &r, ControlTarget::App(AppControl::AutoLevel));
    if comp.sound_auto_level {
        hint(ui, "Loud and quiet clips now play about equally loud; each layer's volume still works on top.");
    }
}
