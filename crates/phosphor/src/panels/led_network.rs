//! LED / DMX panel, the network row: which adapter Art-Net / sACN go out of, a warning when the controllers are on a
//! network no adapter is on, and Find PixLites (Advatek boxes: one strip per output, patched as the boxes say).

use std::net::Ipv4Addr;

use super::*;

pub(crate) fn network_row(ui: &mut egui::Ui, st: &mut State) {
    if matches!(st.comp.dmx.protocol, phosphor_engine::DmxProtocol::UsbPro | phosphor_engine::DmxProtocol::OpenDmx) {
        return;
    }
    let adapters = crate::net_adapters::cached();
    crate::ui::field(ui, "Send out of", |ui| {
        let d = &mut st.comp.dmx;
        let chosen: Option<Ipv4Addr> = d.adapter.trim().parse().ok();
        let shown = match chosen {
            None => crate::i18n::tr("Automatic"),
            Some(ip) => adapters.iter().find(|a| a.ip == ip).map_or_else(|| format!("{ip} ({})", crate::i18n::tr("not on this computer")), |a| a.label()),
        };
        crate::ui::dropdown(ui, "dmx_adapter", 200.0)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                crate::ui::pick_value(ui, &mut d.adapter, String::new(), "Automatic").on_hover_text("The system's default network (often the Wi-Fi)");
                for a in &adapters {
                    crate::ui::pick_value(ui, &mut d.adapter, a.ip.to_string(), a.label());
                }
            })
            .response
            .on_hover_text("The network adapter the lights are on (an Ethernet cable to the rig while the Wi-Fi stays on)");
        let searching = st.pixlites.busy();
        if crate::ui::command_if(ui, !searching, label(icon::MAGNIFYING_GLASS, "Find PixLites"))
            .on_hover_text("Asks Advatek PixLite controllers on every network for their setup (it only asks: nothing on them changes)")
            .clicked()
        {
            st.pixlites.start(adapters.clone());
        }
        if searching {
            ui.spinner();
        }
    });
    reach_hint(ui, st, &adapters);
    found_boxes(ui, st, &adapters);
}

/// Says so when what the lights are sent to isn't on the chosen adapter's network (or on any).
fn reach_hint(ui: &mut egui::Ui, st: &State, adapters: &[crate::net_adapters::Adapter]) {
    let d = &st.comp.dmx;
    let chosen: Option<Ipv4Addr> = d.adapter.trim().parse().ok();
    let mut controllers: Vec<Ipv4Addr> = st.pixlites.found.iter().flat_map(|f| f.boxes.iter().map(|b| b.ip)).collect();
    if let Ok(ip) = d.target.trim().parse::<Ipv4Addr>() {
        controllers.push(ip);
    }
    let Some(far) = controllers.into_iter().find(|ip| match chosen {
        Some(c) => adapters.iter().find(|a| a.ip == c).is_some_and(|a| !a.reaches(*ip)),
        None => !adapters.iter().any(|a| a.reaches(*ip)),
    }) else {
        return;
    };
    let net = format!("{}.{}.{}.x", far.octets()[0], far.octets()[1], far.octets()[2]);
    let on = adapters.iter().find(|a| a.reaches(far));
    let text = match on {
        Some(a) => format!("{} {far} is on {net}: choose {} to reach it", icon::WARNING, a.label()),
        None => format!("{} {far} is on {net}, and no adapter here is: give this computer an address on {net} (for example in Network settings, on the cable to the rig)", icon::WARNING),
    };
    ui.colored_label(crate::theme::WARN, text);
}

/// What Find PixLites found, and the button that makes their strips.
fn found_boxes(ui: &mut egui::Ui, st: &mut State, adapters: &[crate::net_adapters::Adapter]) {
    let Some(found) = st.pixlites.found.clone() else { return };
    if found.boxes.is_empty() && found.problems.is_empty() {
        hint(ui, "No PixLite answered: is the cable in, and the right adapter chosen?");
        return;
    }
    for (i, b) in found.boxes.iter().enumerate() {
        let used = || b.outputs.iter().filter(|o| o.pixels > 0);
        let first = used().map(|o| o.universe).min().unwrap_or(0);
        let last = used().map(|o| o.last_universe()).max().unwrap_or(first);
        let on = adapters.iter().find(|a| a.reaches(b.ip)).map_or_else(String::new, |a| format!(" \u{b7} {}", a.name));
        hint(ui, format!("{} {} \u{b7} {} \u{b7} {} {} \u{b7} {} outputs, {} px \u{b7} U{first}\u{2013}{last}{on}", icon::CHECK_CIRCLE, crate::advatek::label(b, i), b.ip, b.model, b.firmware, b.outputs.iter().filter(|o| o.pixels > 0).count(), b.pixels()));
    }
    for p in &found.problems {
        ui.colored_label(crate::theme::WARN, format!("{} {p}", icon::WARNING));
    }
    if found.boxes.is_empty() {
        return;
    }
    let strips: usize = found.boxes.iter().map(|b| b.outputs.iter().filter(|o| o.pixels > 0).count()).sum();
    let pixels: usize = found.boxes.iter().map(|b| b.pixels()).sum();
    crate::ui::toolbar(ui, |ui| {
        if crate::ui::command(ui, label(icon::PLUS, &format!("Add {strips} strips ({pixels} px)")))
            .on_hover_text("One LED strip per output, patched as the boxes are, as lines down the picture: drag them to where the strings really are. Strips from an earlier Add for the same boxes are replaced")
            .clicked()
        {
            let comp = &mut st.comp;
            let mut ids = Vec::new();
            for _ in 0..strips {
                ids.push(comp.new_id());
            }
            let mut next = ids.into_iter();
            let (n, px) = crate::advatek::import(&mut comp.dmx, &found.boxes, || next.next().unwrap_or(0));
            // The rig is on the network the boxes answered on: send out of it.
            if comp.dmx.adapter.is_empty()
                && let Some(a) = found.boxes.first().and_then(|b| adapters.iter().find(|a| a.reaches(b.ip)))
            {
                comp.dmx.adapter = a.ip.to_string();
            }
            st.notice = Some(format!("{n} strips, {px} pixels added from {} PixLite(s)", found.boxes.len()));
            st.pixlites.found = None;
        }
        if crate::ui::small_command(ui, icon::X).on_hover_text("Forget this search").clicked() {
            st.pixlites.found = None;
        }
    });
}

/// Flash / Chase buttons on a fixture's card: light just this one (all of it, or one pixel running along it).
pub(crate) fn identify_buttons(ui: &mut egui::Ui, identify: &mut crate::led_identify::Identify, fixture: u64) {
    use crate::led_identify::Mode;
    let showing = identify.showing();
    for (mode, name, tip) in [
        (Mode::Flash, "Flash", "Lights only this one, white, every other light dark: find it in the venue. Click again to stop"),
        (Mode::Chase, "Chase", "One pixel runs along it from its first pixel: see where it starts and which way it runs. Click again to stop"),
    ] {
        let mut on = showing == Some((fixture, mode));
        if crate::ui::icon_toggle(ui, &mut on, name).on_hover_text(tip).clicked() {
            identify.toggle(fixture, mode);
        }
    }
}
