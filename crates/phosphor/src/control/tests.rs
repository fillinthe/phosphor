use super::*;
use phosphor_engine::{ControlMap, LaunchTiming};

fn send(hub: &mut ControlHub, comp: &mut Composition, source: ControlSource, value: f32) {
    hub.inject(comp, Incoming { source, value });
    hub.apply(comp, 0.0);
}

#[test]
fn app_controls_and_toggles() {
    let mut hub = ControlHub::new();
    let mut comp = Composition::default();
    let id = comp.add_layer().id;
    let osc = |a: &str| ControlSource::Osc { address: a.to_string() };

    // Built-in OSC addresses for app-wide controls.
    send(&mut hub, &mut comp, osc("/phosphor/launch/timing"), 0.5);
    assert_eq!(comp.launch.timing, LaunchTiming::Beat);
    send(&mut hub, &mut comp, osc("/phosphor/launch/fade"), 0.0);
    assert!(!comp.launch.fade);
    send(&mut hub, &mut comp, osc("/phosphor/tempo"), 0.5);
    assert_eq!(hub.triggers.pop(), Some(Deferred::App(AppControl::Tempo, 0.5)));
    send(&mut hub, &mut comp, osc(&format!("/phosphor/layer/{id}/speed")), 1.0);
    assert!((comp.layer_mut(id).unwrap().rate - 4.0).abs() < 1e-3);

    // A key mapped to an on/off control flips it on each press and ignores the release.
    let key = ControlSource::Key { key: "Q".into() };
    comp.control_maps.push(ControlMap::new(key.clone(), ControlTarget::LayerEnabled(id)));
    let before = comp.layer_mut(id).unwrap().enabled;
    send(&mut hub, &mut comp, key.clone(), 1.0);
    send(&mut hub, &mut comp, key.clone(), 0.0);
    assert_eq!(comp.layer_mut(id).unwrap().enabled, !before);
    send(&mut hub, &mut comp, key.clone(), 1.0);
    assert_eq!(comp.layer_mut(id).unwrap().enabled, before);

    // Learn: the next message binds to the chosen control.
    hub.learning = Some(ControlTarget::LayerOpacity(id));
    let cc = ControlSource::MidiCc { channel: 0, cc: 21 };
    send(&mut hub, &mut comp, cc.clone(), 0.25);
    assert!(hub.learning.is_none());
    assert!(comp.control_maps.iter().any(|m| m.source == cc && m.target == ControlTarget::LayerOpacity(id)));
    send(&mut hub, &mut comp, cc.clone(), 0.75);
    assert!((comp.layer_mut(id).unwrap().opacity - 0.75).abs() < 1e-3);

    // An OSC toggle (TouchOSC style 1/0) follows the value.
    comp.control_maps.push(ControlMap::new(osc("/t/bypass"), ControlTarget::LayerFxBypass(id)));
    send(&mut hub, &mut comp, osc("/t/bypass"), 1.0);
    assert!(comp.layer_mut(id).unwrap().fx_bypass);
    send(&mut hub, &mut comp, osc("/t/bypass"), 0.0);
    assert!(!comp.layer_mut(id).unwrap().fx_bypass);
}

#[test]
fn surface_widgets_drive_bindings() {
    use phosphor_engine::{Binding, ValueRange, WidgetKind};
    let mut hub = ControlHub::new();
    let mut comp = Composition::default();
    let a = comp.add_layer().id;
    let b = comp.add_layer().id;
    // A fader driving two layers: one straight 0.2–0.8, one inverted.
    let fader = comp.surface.add(WidgetKind::Fader);
    let w = comp.surface.widget_mut(fader).unwrap();
    w.bindings.push(Binding { target: ControlTarget::LayerOpacity(a), range: ValueRange { min: 0.2, max: 0.8, invert: false } });
    w.bindings.push(Binding { target: ControlTarget::LayerOpacity(b), range: ValueRange { min: 0.0, max: 1.0, invert: true } });
    // A MIDI knob mapped to the fader.
    let cc = ControlSource::MidiCc { channel: 0, cc: 7 };
    comp.control_maps.push(ControlMap::new(cc.clone(), ControlTarget::Surface(fader)));
    send(&mut hub, &mut comp, cc.clone(), 0.5);
    assert!((comp.surface.widget(fader).unwrap().value - 0.5).abs() < 1e-3);
    assert!((comp.layer_mut(a).unwrap().opacity - 0.5).abs() < 1e-3);
    assert!((comp.layer_mut(b).unwrap().opacity - 0.5).abs() < 1e-3);
    send(&mut hub, &mut comp, cc.clone(), 1.0);
    assert!((comp.layer_mut(a).unwrap().opacity - 0.8).abs() < 1e-3);
    assert!(comp.layer_mut(b).unwrap().opacity < 1e-3);

    // A key flips a surface toggle, which switches a layer.
    let tog = comp.surface.add(WidgetKind::Toggle);
    comp.surface.widget_mut(tog).unwrap().bindings.push(Binding { target: ControlTarget::LayerEnabled(a), range: phosphor_engine::ValueRange::default() });
    let key = ControlSource::Key { key: "T".into() };
    comp.control_maps.push(ControlMap::new(key.clone(), ControlTarget::Surface(tog)));
    send(&mut hub, &mut comp, key.clone(), 1.0);
    send(&mut hub, &mut comp, key.clone(), 0.0);
    assert_eq!(comp.surface.widget(tog).unwrap().value, 1.0);
    assert!(comp.layer_mut(a).unwrap().enabled);
    send(&mut hub, &mut comp, key.clone(), 1.0);
    assert!(!comp.layer_mut(a).unwrap().enabled);

    // Two widgets driving each other don't loop forever.
    let x = comp.surface.add(WidgetKind::Slider);
    let y = comp.surface.add(WidgetKind::Slider);
    comp.surface.widget_mut(x).unwrap().bindings.push(Binding { target: ControlTarget::Surface(y), range: phosphor_engine::ValueRange::default() });
    comp.surface.widget_mut(y).unwrap().bindings.push(Binding { target: ControlTarget::Surface(x), range: phosphor_engine::ValueRange::default() });
    let mut out = Vec::new();
    set_target(&mut comp, &ControlTarget::Surface(x), 0.3, &mut out);
    assert!((comp.surface.widget(y).unwrap().value - 0.3).abs() < 1e-3);

    // A switched-off mapping does nothing; Listen catches the next input.
    comp.control_maps.iter_mut().for_each(|m| m.enabled = false);
    send(&mut hub, &mut comp, cc.clone(), 0.0);
    assert!((comp.layer_mut(a).unwrap().opacity - 0.8).abs() < 1e-3);
    hub.listen_source = true;
    send(&mut hub, &mut comp, ControlSource::Osc { address: "/x".into() }, 0.4);
    assert_eq!(hub.heard, Some(ControlSource::Osc { address: "/x".into() }));
    assert!(!hub.listen_source);
}

/// Reading a control back gives what was set (feedback, surfaces and the remote rely on it).
#[test]
fn get_target_reads_back_what_set_target_wrote() {
    let mut comp = Composition::starter();
    let mut out = Vec::new();
    let mut checked = 0;
    for (t, name) in all_targets(&comp) {
        if get_target(&comp, &t).is_none() {
            continue;
        }
        for v in [0.0, 0.3, 1.0] {
            set_target(&mut comp, &t, v, &mut out);
            let back = get_target(&comp, &t).unwrap();
            let toggle = is_toggle(&comp, &t);
            let choice = matches!(t, ControlTarget::App(AppControl::LaunchTiming | AppControl::LaunchStyle | AppControl::SurfacePage) | ControlTarget::LayerDirection(_));
            let want = if toggle { if v > 0.5 { 1.0 } else { 0.0 } } else { v };
            if choice {
                let mut again = Vec::new();
                let before = comp.clone();
                set_target(&mut comp, &t, back, &mut again);
                assert_eq!(comp, before, "{name}: choice {v} read back as {back} picks another option");
            } else {
                assert!((back - want).abs() < 0.01, "{name}: set {v}, read back {back}");
            }
        }
        checked += 1;
    }
    assert!(checked > 10, "only {checked} targets checked");
}

/// A TouchOSC-style device that sent a built-in address gets changes made elsewhere, once each.
#[test]
fn osc_feedback_reaches_the_sender() {
    use std::net::UdpSocket;
    let mut hub = ControlHub::with_settings(ControlSettings { osc_port: 18_571, feedback: true, feedback_port: 18_572, soft_takeover: false, dmx_in: false, ..Default::default() });
    let device = UdpSocket::bind("127.0.0.1:18572").unwrap();
    device.set_read_timeout(Some(std::time::Duration::from_millis(500))).unwrap();
    let msg = rosc::encoder::encode(&rosc::OscPacket::Message(rosc::OscMessage { addr: "/phosphor/master".into(), args: vec![rosc::OscType::Float(0.8)] })).unwrap();
    device.send_to(&msg, "127.0.0.1:18571").unwrap();
    let mut comp = Composition::starter();
    for _ in 0..50 {
        hub.poll(&mut comp);
        if hub.last.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    hub.apply(&mut comp, 0.0);
    assert!((comp.master_fade - 0.8).abs() < 1e-3, "controller moved master");
    hub.feedback(&comp, 1.0); // first contact: everything once
    let mut buf = [0u8; 4096];
    while device.recv(&mut buf).is_ok() {}
    comp.master_fade = 0.25; // moved with the mouse
    hub.feedback(&comp, 2.0);
    let n = device.recv(&mut buf).expect("feedback for the master fader");
    let (_, packet) = rosc::decoder::decode_udp(&buf[..n]).unwrap();
    let rosc::OscPacket::Message(m) = packet else { panic!("bundle") };
    assert_eq!(m.addr, "/phosphor/master");
    assert_eq!(m.args, vec![rosc::OscType::Float(0.25)]);
    hub.feedback(&comp, 3.0); // unchanged: nothing more
    assert!(device.recv(&mut buf).is_err(), "unchanged values must not be resent");
}

/// A Control Surface fader with a Send OSC address reaches another app when it moves, once per change.
#[test]
fn surface_widgets_send_osc_to_another_app() {
    use std::net::UdpSocket;
    let app = UdpSocket::bind("127.0.0.1:18573").unwrap();
    app.set_read_timeout(Some(std::time::Duration::from_millis(500))).unwrap();
    let mut hub = ControlHub::with_settings(ControlSettings { osc_port: 18_574, feedback: false, feedback_port: 18_575, soft_takeover: false, dmx_in: false, ..Default::default() });
    let mut comp = Composition::starter();
    comp.surface.osc_host = "127.0.0.1".into();
    comp.surface.osc_port = 18_573;
    comp.surface.widgets.push(phosphor_engine::SurfaceWidget { id: 77, kind: phosphor_engine::WidgetKind::Fader, value: 0.4, osc_out: "/lights/dim".into(), ..Default::default() });
    hub.surface_osc(&comp);
    let mut buf = [0u8; 1024];
    let n = app.recv(&mut buf).expect("the fader's value");
    let (_, packet) = rosc::decoder::decode_udp(&buf[..n]).unwrap();
    let rosc::OscPacket::Message(m) = packet else { panic!("bundle") };
    assert_eq!(m.addr, "/lights/dim");
    assert_eq!(m.args, vec![rosc::OscType::Float(0.4)]);
    hub.surface_osc(&comp);
    assert!(app.recv(&mut buf).is_err(), "unchanged: not sent again");
    comp.surface.widgets.last_mut().unwrap().value = 0.9; // a phone moved it
    hub.surface_osc(&comp);
    let n = app.recv(&mut buf).expect("the new value");
    let (_, packet) = rosc::decoder::decode_udp(&buf[..n]).unwrap();
    let rosc::OscPacket::Message(m) = packet else { panic!("bundle") };
    assert_eq!(m.args, vec![rosc::OscType::Float(0.9)]);
}

/// A virtual MIDI device on this Mac sends 128 BPM clock with a Start; the hub's MIDI clock locks to it.
#[cfg(target_os = "macos")]
#[test]
fn locks_to_midi_clock_from_a_real_midi_port() {
    use midir::os::unix::VirtualOutput;
    let Ok(out) = midir::MidiOutput::new("phosphor-test") else { return };
    let Ok(mut port) = out.create_virtual("Phosphor Test Clock") else { return };
    let mut hub = ControlHub::with_settings(ControlSettings { osc_port: 18_581, feedback: false, feedback_port: 18_582, soft_takeover: false, dmx_in: false, ..Default::default() });
    assert!(hub.midi_ports.iter().any(|p| p.contains("Phosphor Test Clock")), "virtual port not opened: {:?}", hub.midi_ports);
    let start = std::time::Instant::now();
    let tick = std::time::Duration::from_secs_f64(60.0 / 128.0 / 24.0);
    port.send(&[0xFA]).unwrap();
    for i in 0..60u32 {
        port.send(&[0xF8]).unwrap();
        std::thread::sleep((start + tick * (i + 1)).saturating_duration_since(std::time::Instant::now()));
    }
    std::thread::sleep(std::time::Duration::from_millis(20));
    let now = 100.0;
    hub.poll_clock(now);
    let bpm = hub.midi_clock.bpm().expect("tempo from the ticks");
    assert!((bpm - 128.0).abs() < 2.0, "bpm {bpm}");
    assert!(hub.midi_clock.active(now));
    let (_, beats) = hub.midi_clock.position().expect("running after Start");
    assert!((beats - 59.0 / 24.0).abs() < 0.01, "beats {beats}");
}

#[test]
fn groups_answer_layer_mix_targets() {
    let mut hub = ControlHub::new();
    let mut comp = Composition::default();
    let a = comp.add_layer().id;
    let b = comp.add_layer().id;
    let g = comp.create_group(&[a, b]).unwrap();
    let osc = |a: String| ControlSource::Osc { address: a };

    send(&mut hub, &mut comp, osc(format!("/phosphor/layer/{g}/opacity")), 0.25);
    assert!((comp.group(g).unwrap().opacity - 0.25).abs() < 1e-6);
    assert_eq!(get_target(&comp, &ControlTarget::LayerOpacity(g)), Some(0.25));
    send(&mut hub, &mut comp, osc(format!("/phosphor/layer/{g}/on")), 0.0);
    assert!(!comp.group(g).unwrap().enabled);
    // Members are untouched, and the group is listed as a target.
    assert!(comp.layers.iter().all(|l| l.opacity == 1.0 && l.enabled));
    assert!(all_targets(&comp).iter().any(|(t, _)| *t == ControlTarget::LayerFxBypass(g)));
}

#[test]
fn colour_widget_sends_red_green_blue() {
    let mut comp = Composition::default();
    let a = comp.add_layer().id;
    let b = comp.add_layer().id;
    let id = comp.surface.add(phosphor_engine::WidgetKind::Color);
    let w = comp.surface.widget_mut(id).unwrap();
    w.bindings.push(phosphor_engine::Binding { target: ControlTarget::LayerOpacity(a), range: phosphor_engine::ValueRange::default() });
    w.bindings_y.push(phosphor_engine::Binding { target: ControlTarget::LayerOpacity(b), range: phosphor_engine::ValueRange::default() });
    let mut out = Vec::new();
    set_target(&mut comp, &ControlTarget::SurfaceY(id), 1.0, &mut out);
    // Hue 0, full saturation and brightness = pure red.
    set_target(&mut comp, &ControlTarget::Surface(id), 0.0, &mut out);
    assert_eq!((comp.layer_mut(a).unwrap().opacity, comp.layer_mut(b).unwrap().opacity), (1.0, 0.0));
    // A third of the way round = green.
    set_target(&mut comp, &ControlTarget::Surface(id), 1.0 / 3.0, &mut out);
    assert!(comp.layer_mut(a).unwrap().opacity < 0.01 && comp.layer_mut(b).unwrap().opacity > 0.99);
}

#[test]
fn a_knob_sets_a_hand_modulator_and_osc_has_an_address() {
    let mut comp = Composition::default();
    comp.modulators.push(phosphor_engine::Modulator { id: 7, name: "Knob".into(), signal: phosphor_engine::Modulation { source: phosphor_engine::ModSource::Hand, ..Default::default() }, chain: None, value: 0.0, looper: Default::default(), sequence: None });
    let mut out = Vec::new();
    set_target(&mut comp, &ControlTarget::Modulator(7), 0.8, &mut out);
    assert_eq!(get_target(&comp, &ControlTarget::Modulator(7)), Some(0.8));
    assert!(all_targets(&comp).iter().any(|(t, name)| *t == ControlTarget::Modulator(7) && name.contains("Knob")));
    assert_eq!(default_osc_address(&ControlTarget::Modulator(7)), "/phosphor/modulator/7");
}

/// Soft takeover: a fader far from the value doesn't jump it; once it passes the value it follows; a mouse move
/// in between makes it catch up again.
#[test]
fn faders_take_over_softly() {
    let mut hub = ControlHub::with_settings(ControlSettings { osc_port: 18_576, feedback: false, feedback_port: 18_577, soft_takeover: true, dmx_in: false, ..Default::default() });
    let mut comp = Composition::starter();
    comp.master_fade = 0.5;
    let source = ControlSource::Osc { address: "/fader1".into() };
    comp.control_maps.push(phosphor_engine::ControlMap::new(source.clone(), ControlTarget::MasterFade));
    let mut send = |hub: &mut ControlHub, comp: &mut Composition, v: f32| {
        hub.fresh.insert(source.clone(), v);
        hub.apply(comp, 0.0);
    };
    send(&mut hub, &mut comp, 0.9);
    assert!((comp.master_fade - 0.5).abs() < 1e-6, "far away: no jump");
    send(&mut hub, &mut comp, 0.7);
    assert!((comp.master_fade - 0.5).abs() < 1e-6, "still above it");
    send(&mut hub, &mut comp, 0.45);
    assert!((comp.master_fade - 0.45).abs() < 1e-6, "passed the value: it follows");
    send(&mut hub, &mut comp, 0.3);
    assert!((comp.master_fade - 0.3).abs() < 1e-6);
    comp.master_fade = 0.8; // the mouse moved it
    send(&mut hub, &mut comp, 0.35);
    assert!((comp.master_fade - 0.8).abs() < 1e-6, "after a mouse move it has to catch up again");
    send(&mut hub, &mut comp, 0.79);
    assert!((comp.master_fade - 0.79).abs() < 1e-6, "near the value: caught");
}

/// The monitor keeps what came in (newest last), capped, and stops listing while paused.
#[test]
fn the_monitor_lists_incoming_messages() {
    let mut hub = ControlHub::new();
    let mut comp = Composition::starter();
    let cc = ControlSource::MidiCc { channel: 0, cc: 7 };
    hub.inject(&mut comp, Incoming { source: cc.clone(), value: 0.25 });
    hub.inject(&mut comp, Incoming { source: ControlSource::Osc { address: "/1/fader1".into() }, value: 0.5 });
    assert_eq!(hub.recent.len(), 2);
    assert_eq!(hub.recent.back().unwrap().1, ControlSource::Osc { address: "/1/fader1".into() });
    hub.monitor_paused = true;
    hub.inject(&mut comp, Incoming { source: cc.clone(), value: 0.75 });
    assert_eq!(hub.recent.len(), 2, "paused: not listed");
    assert_eq!(hub.last.as_ref().unwrap().value, 0.75, "but still received");
    hub.monitor_paused = false;
    for i in 0..300 {
        hub.inject(&mut comp, Incoming { source: cc.clone(), value: i as f32 / 300.0 });
    }
    assert_eq!(hub.recent.len(), super::MONITOR_LEN);
}

/// A smoothed mapping glides to the new value over its time instead of jumping.
#[test]
fn smoothed_mappings_glide() {
    let mut hub = ControlHub::new();
    let mut comp = Composition::starter();
    comp.master_fade = 0.0;
    let source = ControlSource::MidiCc { channel: 0, cc: 9 };
    let mut map = phosphor_engine::ControlMap::new(source.clone(), ControlTarget::MasterFade);
    map.fx.push(phosphor_engine::ValueFx::Smooth(1.0));
    comp.control_maps.push(map);
    hub.inject(&mut comp, Incoming { source, value: 1.0 });
    hub.apply(&mut comp, 10.0);
    assert!(comp.master_fade < 0.05, "starts where it was");
    hub.apply(&mut comp, 10.5);
    assert!(comp.master_fade > 0.5 && comp.master_fade < 0.9, "half way through: {}", comp.master_fade);
    hub.apply(&mut comp, 11.2);
    assert!((comp.master_fade - 1.0).abs() < 1e-6, "arrived");
}

#[test]
fn slices_are_controls() {
    let mut comp = Composition::default();
    let out = comp.add_output();
    let sid = comp.new_id();
    comp.outputs.iter_mut().find(|o| o.id == out).unwrap().mapping.add_surface(sid);
    let mut deferred = Vec::new();
    set_target(&mut comp, &ControlTarget::SliceOpacity(sid), 0.25, &mut deferred);
    assert_eq!(get_target(&comp, &ControlTarget::SliceOpacity(sid)), Some(0.25));
    set_target(&mut comp, &ControlTarget::SliceEnabled(sid), 0.0, &mut deferred);
    assert_eq!(get_target(&comp, &ControlTarget::SliceEnabled(sid)), Some(0.0), "0 = off");
    set_target(&mut comp, &ControlTarget::SliceEnabled(sid), FLIP, &mut deferred);
    assert_eq!(get_target(&comp, &ControlTarget::SliceEnabled(sid)), Some(1.0), "a button press flips it");
    assert_eq!(default_osc_address(&ControlTarget::SliceOpacity(sid)), format!("/phosphor/slice/{sid}/opacity"));
    assert!(all_targets(&comp).iter().any(|(t, _)| *t == ControlTarget::SliceEnabled(sid)));
    comp.control_prefs.set_locked(&ControlTarget::SliceOpacity(sid), true);
    set_target(&mut comp, &ControlTarget::SliceOpacity(sid), 1.0, &mut deferred);
    assert_eq!(comp.slice(sid).unwrap().opacity, 0.25, "locked");
}

#[test]
fn a_slice_group_is_one_control() {
    let mut comp = Composition::default();
    let out = comp.add_output();
    let (a, b, g) = (comp.new_id(), comp.new_id(), comp.new_id());
    let mapping = &mut comp.outputs.iter_mut().find(|o| o.id == out).unwrap().mapping;
    mapping.add_surface(a);
    mapping.add_surface(b);
    mapping.group_slices(g, &[a, b]);
    let mut deferred = Vec::new();
    set_target(&mut comp, &ControlTarget::SliceOpacity(g), 0.4, &mut deferred);
    assert_eq!(comp.slice(a).unwrap().opacity, 0.4);
    assert_eq!(comp.slice(b).unwrap().opacity, 0.4);
    comp.slice_mut(b).unwrap().enabled = false;
    set_target(&mut comp, &ControlTarget::SliceEnabled(g), FLIP, &mut deferred);
    assert!(!comp.slice(a).unwrap().enabled && !comp.slice(b).unwrap().enabled, "flips from the first one, all follow");
    assert!(all_targets(&comp).iter().any(|(t, _)| *t == ControlTarget::SliceOpacity(g)));
}

#[test]
fn a_glide_moves_a_target_over_time() {
    let mut comp = Composition::default();
    let mut hub = ControlHub::new();
    let xf = ControlTarget::App(AppControl::Crossfader);
    comp.crossfader = 0.0;
    hub.glide_to(xf.clone(), 0.0, 1.0, 10.0, 2.0);
    assert_eq!(hub.glide_target(&xf), Some(1.0));
    hub.apply(&mut comp, 11.0);
    assert!(comp.crossfader > 0.5 && comp.crossfader < 1.0, "half-way in time, eased past half: {}", comp.crossfader);
    hub.apply(&mut comp, 12.5);
    assert_eq!(comp.crossfader, 1.0);
    assert_eq!(hub.glide_target(&xf), None, "arrived");
}

#[test]
fn delayed_moves_land_later() {
    let mut hub = ControlHub::new();
    hub.soft_takeover = true;
    let mut comp = Composition::starter();
    comp.master_fade = 0.0;
    let source = ControlSource::MidiCc { channel: 0, cc: 10 };
    let mut map = phosphor_engine::ControlMap::new(source.clone(), ControlTarget::MasterFade);
    map.fx.push(phosphor_engine::ValueFx::Delay(0.5));
    comp.control_maps.push(map);
    hub.inject(&mut comp, Incoming { source, value: 0.8 });
    hub.apply(&mut comp, 10.0);
    assert_eq!(comp.master_fade, 0.0, "not yet");
    hub.apply(&mut comp, 10.4);
    assert_eq!(comp.master_fade, 0.0);
    hub.apply(&mut comp, 10.51);
    assert!((comp.master_fade - 0.8).abs() < 1e-6, "half a second later");
    // The fader keeps moving (soft takeover on): every move lands, late.
    hub.inject(&mut comp, Incoming { source: ControlSource::MidiCc { channel: 0, cc: 10 }, value: 0.9 });
    hub.apply(&mut comp, 10.6);
    hub.apply(&mut comp, 11.11);
    assert!((comp.master_fade - 0.9).abs() < 1e-6, "{}", comp.master_fade);
}

#[test]
fn slice_effects_are_controls_and_copies_get_their_own() {
    let mut comp = Composition::default();
    let out = comp.add_output();
    let sid = comp.new_id();
    comp.outputs.iter_mut().find(|o| o.id == out).unwrap().mapping.add_surface(sid);
    let fx = comp.new_effect("invert").unwrap();
    let fid = fx.id;
    comp.effects_mut(sid).expect("a slice has a chain").push(fx);
    let mut deferred = Vec::new();
    set_target(&mut comp, &ControlTarget::EffectMix(fid), 0.25, &mut deferred);
    assert_eq!(comp.effects(sid).unwrap()[0].mix, 0.25, "found through all_effects");
    assert!(all_targets(&comp).iter().any(|(t, _)| *t == ControlTarget::EffectMix(fid)));
    let copy = comp.duplicate_slice(sid).unwrap();
    let copied = comp.effects(copy).unwrap()[0].id;
    assert_ne!(copied, fid, "its own effect, not a second handle on the first");
    assert_eq!(comp.slice(copy).unwrap().name, "Surface 1 copy");
}

#[test]
fn corner_pin_corners_are_controls() {
    let mut comp = Composition::default();
    let id = comp.add_layer().id;
    let t = ControlTarget::LayerCorner { layer: id, point: 5 };
    assert!(!all_targets(&comp).iter().any(|(x, _)| *x == t), "only while pinned");
    comp.layer_mut(id).unwrap().transform.corners = Some([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    let mut deferred = Vec::new();
    set_target(&mut comp, &t, 0.7, &mut deferred);
    assert_eq!(comp.layer_mut(id).unwrap().transform.corners.unwrap()[2], [1.0, 0.7], "bottom-right y");
    assert_eq!(get_target(&comp, &t), Some(0.7));
    assert_eq!(default_osc_address(&t), format!("/phosphor/layer/{id}/corner/3/y"));
}

/// Lights are controls: a moving head's pan / tilt / dimmer / motion size and any light's brightness.
#[test]
fn lights_are_controls() {
    let mut comp = Composition::default();
    comp.dmx.fixtures.push(phosphor_engine::Fixture { id: 5, name: "Head".into(), shape: phosphor_engine::FixtureShape::MovingHead, ..Default::default() });
    comp.dmx.fixtures.push(phosphor_engine::Fixture { id: 6, name: "Strip".into(), ..Default::default() });
    let pan = ControlTarget::Fixture { fixture: 5, param: 0 };
    let mut deferred = Vec::new();
    set_target(&mut comp, &pan, 0.25, &mut deferred);
    assert_eq!(comp.dmx.fixtures[0].pan, 0.25);
    assert_eq!(get_target(&comp, &pan), Some(0.25));
    assert_eq!(default_osc_address(&ControlTarget::Fixture { fixture: 5, param: 3 }), "/phosphor/light/5/motion_size");
    let all = all_targets(&comp);
    assert_eq!(all.iter().filter(|(t, _)| matches!(t, ControlTarget::Fixture { fixture: 5, .. })).count(), 5);
    assert_eq!(all.iter().filter(|(t, _)| matches!(t, ControlTarget::Fixture { fixture: 6, .. })).count(), 1, "a strip: brightness only");
    comp.control_prefs.set_locked(&pan, true);
    set_target(&mut comp, &pan, 0.9, &mut deferred);
    assert_eq!(comp.dmx.fixtures[0].pan, 0.25, "locked");
}

/// A moving head's extra channels (gobo, zoom…) go out after its layout and are controls too.
#[test]
fn extra_light_channels() {
    let mut comp = Composition::default();
    let mut f = phosphor_engine::Fixture { id: 5, name: "Head".into(), shape: phosphor_engine::FixtureShape::MovingHead, universe: 0, channel: 1, ..Default::default() };
    f.extra = vec![phosphor_engine::ExtraChannel { name: "Gobo".into(), value: 0.0 }, phosphor_engine::ExtraChannel { name: "Zoom".into(), value: 0.0 }];
    comp.dmx.fixtures.push(f);
    let zoom = ControlTarget::Fixture { fixture: 5, param: 6 };
    set_target(&mut comp, &zoom, 0.5, &mut Vec::new());
    assert_eq!(get_target(&comp, &zoom), Some(0.5));
    assert!(all_targets(&comp).iter().any(|(t, n)| *t == zoom && n == "Light Head · Zoom"));
    assert_eq!(default_osc_address(&zoom), "/phosphor/light/5/extra/2");
    let mut universes = std::collections::BTreeMap::new();
    comp.dmx.fixtures[0].write(&[[255, 0, 0, 255]], &mut universes, 0.0);
    let u = universes[&0];
    // Rgb8 = 8 channels, then Gobo (0) and Zoom (128); nothing after.
    assert_eq!((u[8], u[9], u[10]), (0, 128, 0));
    assert_eq!(comp.dmx.fixtures[0].next_free(), (0, 11));
}

#[test]
fn soft_takeover_waits_or_glides() {
    let source = ControlSource::MidiCc { channel: 0, cc: 11 };
    let setup = || {
        let mut comp = Composition::starter();
        comp.master_fade = 0.2;
        comp.control_maps.push(phosphor_engine::ControlMap::new(source.clone(), ControlTarget::MasterFade));
        comp
    };
    // Waiting (the default): a fader far away does nothing until it comes within the distance.
    let mut hub = ControlHub::new();
    hub.soft_takeover = true;
    let mut comp = setup();
    hub.inject(&mut comp, Incoming { source: source.clone(), value: 0.9 });
    hub.apply(&mut comp, 1.0);
    assert_eq!(comp.master_fade, 0.2);
    hub.pickup_near = 0.1;
    hub.inject(&mut comp, Incoming { source: source.clone(), value: 0.28 });
    hub.apply(&mut comp, 1.1);
    assert!((comp.master_fade - 0.28).abs() < 1e-6, "caught within 10%: {}", comp.master_fade);
    // Glide: the value travels to the fader and then follows it.
    let mut hub = ControlHub::new();
    hub.soft_takeover = true;
    hub.takeover_glide = true;
    let mut comp = setup();
    hub.inject(&mut comp, Incoming { source: source.clone(), value: 0.9 });
    hub.apply(&mut comp, 2.0);
    hub.apply(&mut comp, 2.15);
    assert!(comp.master_fade > 0.3 && comp.master_fade < 0.9, "on its way: {}", comp.master_fade);
    hub.apply(&mut comp, 2.5);
    assert!((comp.master_fade - 0.9).abs() < 1e-6, "arrived: {}", comp.master_fade);
    hub.inject(&mut comp, Incoming { source, value: 0.85 });
    hub.apply(&mut comp, 2.6);
    assert!((comp.master_fade - 0.85).abs() < 1e-6, "follows the fader once there: {}", comp.master_fade);
}

#[test]
fn fall_holds_peaks() {
    let mut hub = ControlHub::new();
    hub.soft_takeover = true;
    let mut comp = Composition::starter();
    comp.master_fade = 0.0;
    let source = ControlSource::MidiCc { channel: 0, cc: 12 };
    let mut map = phosphor_engine::ControlMap::new(source.clone(), ControlTarget::MasterFade);
    map.fx.push(phosphor_engine::ValueFx::Fall(1.0));
    comp.control_maps.push(map);
    hub.inject(&mut comp, Incoming { source: source.clone(), value: 0.9 });
    hub.apply(&mut comp, 1.0);
    assert!((comp.master_fade - 0.9).abs() < 1e-6, "a rise lands at once (and skips soft takeover)");
    hub.inject(&mut comp, Incoming { source: source.clone(), value: 0.1 });
    hub.apply(&mut comp, 1.0);
    hub.apply(&mut comp, 1.3);
    assert!(comp.master_fade > 0.2 && comp.master_fade < 0.9, "falling: {}", comp.master_fade);
    hub.inject(&mut comp, Incoming { source: source.clone(), value: 0.1 });
    hub.apply(&mut comp, 1.6);
    hub.apply(&mut comp, 2.1);
    assert!((comp.master_fade - 0.1).abs() < 1e-6, "down after the fall time (repeats didn't restart it): {}", comp.master_fade);
}

#[test]
fn endless_knobs_and_dead_bands() {
    let mut hub = ControlHub::new();
    hub.soft_takeover = true;
    let mut comp = Composition::starter();
    comp.master_fade = 0.5;
    let knob = ControlSource::MidiCc { channel: 0, cc: 13 };
    let mut map = phosphor_engine::ControlMap::new(knob.clone(), ControlTarget::MasterFade);
    map.fx.push(phosphor_engine::ValueFx::Endless(0.01));
    comp.control_maps.push(map);
    // Three clicks up (CC value 3), then two down (126).
    hub.inject(&mut comp, Incoming { source: knob.clone(), value: 3.0 / 127.0 });
    hub.apply(&mut comp, 1.0);
    assert!((comp.master_fade - 0.53).abs() < 1e-5, "{}", comp.master_fade);
    hub.inject(&mut comp, Incoming { source: knob, value: 126.0 / 127.0 });
    hub.apply(&mut comp, 1.1);
    assert!((comp.master_fade - 0.51).abs() < 1e-5, "{}", comp.master_fade);

    let fader = ControlSource::MidiCc { channel: 0, cc: 14 };
    let mut comp = Composition::starter();
    comp.crossfader = 0.0;
    let mut map = phosphor_engine::ControlMap::new(fader.clone(), ControlTarget::App(phosphor_engine::AppControl::Crossfader));
    map.fx.push(phosphor_engine::ValueFx::DeadBand(0.05));
    comp.control_maps.push(map);
    let mut hub = ControlHub::new();
    hub.soft_takeover = false;
    for (t, v, want) in [(1.0, 0.5, 0.5), (1.1, 0.52, 0.5), (1.2, 0.48, 0.5), (1.3, 0.56, 0.56)] {
        hub.inject(&mut comp, Incoming { source: fader.clone(), value: v });
        hub.apply(&mut comp, t);
        assert!((comp.crossfader - want).abs() < 1e-6, "{v} → {} (wanted {want})", comp.crossfader);
    }
}

#[test]
fn osc_wildcards_match_like_the_spec() {
    use super::osc_match as m;
    assert!(m("/fader/*", "/fader/12") && !m("/fader/*", "/fader/1/x"), "* stays inside one part");
    assert!(m("/btn/?", "/btn/3") && !m("/btn/?", "/btn/33"));
    assert!(m("/btn/[1-4]", "/btn/2") && !m("/btn/[1-4]", "/btn/5") && m("/btn/[!1-4]", "/btn/5"));
    assert!(m("/{deck,mixer}/x", "/mixer/x") && !m("/{deck,mixer}/x", "/fx/x"));
    assert!(m("/a*z", "/abcz") && m("/a*z", "/az") && !m("/a*z", "/abc"));
}

/// A mapping with a wildcard address catches matching messages; a second OSC port also listens.
#[test]
fn wildcard_mappings_and_extra_osc_ports() {
    use std::net::UdpSocket;
    let mut hub = ControlHub::with_settings(ControlSettings { osc_port: 18_591, osc_extra_ports: vec![18_592], feedback: false, soft_takeover: false, ..Default::default() });
    assert!(hub.osc_status.contains("18592"), "{}", hub.osc_status);
    let mut comp = Composition::starter();
    comp.control_maps.push(phosphor_engine::ControlMap::new(ControlSource::Osc { address: "/deck/*/level".into() }, ControlTarget::MasterFade));
    let send = UdpSocket::bind("127.0.0.1:0").unwrap();
    let msg = rosc::encoder::encode(&rosc::OscPacket::Message(rosc::OscMessage { addr: "/deck/2/level".into(), args: vec![rosc::OscType::Float(0.35)] })).unwrap();
    send.send_to(&msg, "127.0.0.1:18592").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while (comp.master_fade - 0.35).abs() > 1e-6 && std::time::Instant::now() < deadline {
        hub.poll(&mut comp);
        hub.apply(&mut comp, 0.0);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!((comp.master_fade - 0.35).abs() < 1e-6, "{}", comp.master_fade);
}

#[test]
fn a_fader_covers_only_a_limited_slider() {
    let mut hub = ControlHub::new();
    hub.soft_takeover = false;
    let mut comp = Composition::starter();
    let source = ControlSource::MidiCc { channel: 0, cc: 15 };
    comp.control_maps.push(phosphor_engine::ControlMap::new(source.clone(), ControlTarget::MasterFade));
    comp.control_prefs.set_limit(&ControlTarget::MasterFade, true, Some(0.5));
    for (t, v, want) in [(1.0, 0.0, 0.5), (1.1, 1.0, 1.0), (1.2, 0.5, 0.75)] {
        hub.inject(&mut comp, Incoming { source: source.clone(), value: v });
        hub.apply(&mut comp, t);
        assert!((comp.master_fade - want).abs() < 1e-5, "{v} → {} (wanted {want})", comp.master_fade);
    }
}

#[test]
fn osc_words_reach_text_layers() {
    let mut hub = ControlHub::new();
    let mut comp = Composition::starter();
    let id = comp.add_layer().id;
    comp.layer_mut(id).unwrap().source = phosphor_engine::SourceSpec::Text(crate::text::default_style());
    let text = |c: &Composition| match &c.layers.iter().find(|l| l.id == id).unwrap().source {
        phosphor_engine::SourceSpec::Text(t) => t.text.clone(),
        _ => String::new(),
    };
    hub.inject_text(&format!("/phosphor/layer/{id}/text"), "Hello");
    hub.apply(&mut comp, 1.0);
    assert_eq!(text(&comp), "Hello", "built-in address");
    comp.control_maps.push(phosphor_engine::ControlMap::new(ControlSource::Osc { address: "/lyrics/*".into() }, ControlTarget::LayerText(id)));
    hub.inject_text("/lyrics/line", "Second line");
    hub.apply(&mut comp, 1.1);
    assert_eq!(text(&comp), "Second line", "a mapped (wildcard) address");
}

/// A control with a send goes out over OSC when it changes, once per change.
#[test]
fn controls_send_their_value_out() {
    use std::net::UdpSocket;
    let app = UdpSocket::bind("127.0.0.1:18593").unwrap();
    app.set_read_timeout(Some(std::time::Duration::from_millis(500))).unwrap();
    let mut hub = ControlHub::with_settings(ControlSettings { osc_port: 18_594, feedback: false, soft_takeover: false, ..Default::default() });
    let mut comp = Composition::starter();
    comp.surface.osc_host = "127.0.0.1".into();
    comp.surface.osc_port = 18_593;
    comp.master_fade = 0.25;
    comp.control_sends.push(phosphor_engine::ControlSend { target: ControlTarget::MasterFade, to: phosphor_engine::SendTo::Osc { address: "/lights/master".into() } });
    let mut buf = [0u8; 1024];
    let mut read = || {
        let n = app.recv(&mut buf).ok()?;
        match rosc::decoder::decode_udp(&buf[..n]).ok()?.1 {
            rosc::OscPacket::Message(m) => Some((m.addr, m.args)),
            _ => None,
        }
    };
    hub.sends(&comp);
    assert_eq!(read(), Some(("/lights/master".into(), vec![rosc::OscType::Float(0.25)])));
    hub.sends(&comp);
    assert_eq!(read(), None, "unchanged: not again");
    comp.master_fade = 0.9;
    hub.sends(&comp);
    assert_eq!(read(), Some(("/lights/master".into(), vec![rosc::OscType::Float(0.9)])));
}

/// A Choices widget picks one of a pick-one control's options (here a layer's four play directions).
#[test]
fn choices_widgets_pick_options() {
    let mut comp = Composition::starter();
    let layer = comp.add_layer().id;
    let id = comp.surface.add(phosphor_engine::WidgetKind::Choices);
    let w = comp.surface.widget_mut(id).unwrap();
    w.items = ["Fwd", "Rev", "Ping", "Rnd"].map(String::from).to_vec();
    w.bindings.push(phosphor_engine::Binding { target: ControlTarget::LayerDirection(layer), range: phosphor_engine::ValueRange::default() });
    let pick = w.choice_value(3);
    let mut out = Vec::new();
    set_target(&mut comp, &ControlTarget::Surface(id), pick, &mut out);
    assert_eq!(comp.layers.iter().find(|l| l.id == layer).unwrap().direction, phosphor_engine::PlayDirection::Random);
    // A fader on it: anywhere in the second quarter = the second choice.
    set_target(&mut comp, &ControlTarget::Surface(id), 0.3, &mut out);
    assert_eq!(comp.layers.iter().find(|l| l.id == layer).unwrap().direction, phosphor_engine::PlayDirection::Reverse);
    assert_eq!(comp.surface.widget(id).unwrap().picked(), 1);
}
