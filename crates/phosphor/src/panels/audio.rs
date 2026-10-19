//! Movie sound volume, audio input, analysis and beat detector.

use super::*;

pub(crate) fn audio(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} Movie sound", icon::SPEAKER_SIMPLE_HIGH)).strong());
        let comp = &mut st.comp;
        let r = guarded!(ui, comp.volume, Slider::new(&mut comp.volume, 0.0..=2.0).show_value(false)).on_hover_text("Master volume for every movie's sound (each layer has its own in the Inspector)");
        value_bubble(ui, &r, format!("{:.0}%", comp.volume * 100.0));
        map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::MasterVolume));
    });
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
        if ui.toggle_value(&mut on, text).clicked() {
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
    let gain = guarded!(ui, audio.gain, Slider::new(&mut audio.gain, 0.1..=10.0).text("Gain").logarithmic(true));
    map_menu(ui, &gain, ControlTarget::App(phosphor_engine::AppControl::AudioGain));
    guarded!(ui, audio.envelope.attack, Slider::new(&mut audio.envelope.attack, 0.0..=0.5).text("Attack (s)"))
        .on_hover_text("How fast the levels rise: 0 = instantly (punchy), higher = smoother swells");
    guarded!(ui, audio.envelope.release, Slider::new(&mut audio.envelope.release, 0.02..=2.0).text("Release (s)"))
        .on_hover_text("How fast the levels fall after a hit: short = flickery, long = slow fades");
    ui.separator();
    for (name, v) in [
        ("Level", mod_inputs.audio_level),
        ("Low", mod_inputs.audio_low),
        ("Mid", mod_inputs.audio_mid),
        ("High", mod_inputs.audio_high),
    ] {
        ui.add(egui::ProgressBar::new(v).text(name).fill(accent().gamma_multiply(0.6)));
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
    let kick = guarded!(ui, audio.beats.sensitivity, Slider::new(&mut audio.beats.sensitivity, 1.1..=3.0).text("Kick threshold").fixed_decimals(2))
        .on_hover_text("Lower catches softer kicks; higher ignores everything but strong hits");
    map_menu(ui, &kick, ControlTarget::App(phosphor_engine::AppControl::KickThreshold));
    ui.horizontal(|ui| {
        let r = ui.toggle_value(follow_audio, label(icon::METRONOME, "Follow audio"))
            .on_hover_text("Lock Phosphor's tempo and beat to the music, so On beat / On bar launches land on the kick");
        map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::FollowAudio));
        hint(ui, "Also: Audio beat (kick) under any slider's wave button.");
    });
    ui.add_space(4.0);
    hint(ui, format!("Press {} next to any slider to drive it with these signals or an LFO.", icon::WAVE_SINE));
}
