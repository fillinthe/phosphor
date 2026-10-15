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
    comp.surface.widget_mut(tog).unwrap().bindings.push(Binding { target: ControlTarget::LayerEnabled(a), range: ValueRange::default() });
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
    comp.surface.widget_mut(x).unwrap().bindings.push(Binding { target: ControlTarget::Surface(y), range: ValueRange::default() });
    comp.surface.widget_mut(y).unwrap().bindings.push(Binding { target: ControlTarget::Surface(x), range: ValueRange::default() });
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
            let choice = matches!(t, ControlTarget::App(AppControl::LaunchTiming | AppControl::SurfacePage) | ControlTarget::LayerDirection(_));
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
    let mut hub = ControlHub::with_settings(ControlSettings { osc_port: 18_571, feedback: true, feedback_port: 18_572 });
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
