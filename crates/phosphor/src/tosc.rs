//! TouchOSC layout export: a `.tosc` file (zlib-compressed XML, `lexml`) built from the Control Surface, one
//! TouchOSC page per surface page, every control already sending Phosphor's built-in OSC address. With no
//! surface yet, a starter page: master, each layer's opacity and the first eight clip columns.

use std::fmt::Write as _;
use std::io::Write as _;

use phosphor_engine::{Composition, ControlTarget, SurfaceWidget, WidgetKind};

use crate::control::default_osc_address;

/// Layout width in TouchOSC points (an iPad in landscape).
const WIDTH: f32 = 1024.0;
const HEIGHT: f32 = 768.0;
/// Space for TouchOSC's page tabs.
const TABS: f32 = 40.0;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn prop_s(x: &mut String, key: &str, v: &str) {
    let _ = write!(x, "<property type='s'><key>{key}</key><value>{}</value></property>", esc(v));
}

fn prop_i(x: &mut String, key: &str, v: i32) {
    let _ = write!(x, "<property type='i'><key>{key}</key><value>{v}</value></property>");
}

fn prop_frame(x: &mut String, [fx, fy, fw, fh]: [f32; 4]) {
    let _ = write!(x, "<property type='r'><key>frame</key><value><x>{fx:.0}</x><y>{fy:.0}</y><w>{fw:.0}</w><h>{fh:.0}</h></value></property>");
}

fn prop_color(x: &mut String, [r, g, b]: [u8; 3]) {
    let f = |c: u8| c as f32 / 255.0;
    let _ = write!(x, "<property type='c'><key>color</key><value><r>{:.3}</r><g>{:.3}</g><b>{:.3}</b><a>1</a></value></property>", f(r), f(g), f(b));
}

/// One OSC message: `address` with the control's `var` (x or y) as a float 0..1.
fn osc(x: &mut String, address: &str, var: &str) {
    let partial = |kind: &str, conv: &str, value: &str| format!("<partial><type>{kind}</type><conversion>{conv}</conversion><value>{}</value><scaleMin>0</scaleMin><scaleMax>1</scaleMax></partial>", esc(value));
    let _ = write!(
        x,
        "<osc><enabled>1</enabled><send>1</send><receive>1</receive><feedback>0</feedback><connections>00001</connections>\
         <triggers><trigger><var>{var}</var><condition>ANY</condition></trigger></triggers>\
         <path>{}</path><arguments>{}</arguments></osc>",
        partial("CONSTANT", "STRING", address),
        partial("VALUE", "FLOAT", var)
    );
}

/// A control node. `messages` are (address, var) pairs.
fn node(x: &mut String, kind: &str, name: &str, frame: [f32; 4], color: [u8; 3], extra: &[(&str, i32)], messages: &[(String, &str)]) {
    let _ = write!(x, "<node ID='{}' type='{kind}'><properties>", uuid());
    prop_s(x, "name", name);
    prop_frame(x, frame);
    prop_color(x, color);
    for (k, v) in extra {
        prop_i(x, k, *v);
    }
    if kind == "LABEL" {
        prop_s(x, "text", name);
    }
    x.push_str("</properties><values/><messages>");
    for (addr, var) in messages {
        osc(x, addr, var);
    }
    x.push_str("</messages></node>");
}

fn uuid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let a = t ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}", (a >> 32) as u32, (a >> 16) as u16, a as u16 & 0xfff, n as u16 & 0xfff, a.wrapping_mul(31) & 0xffff_ffff_ffff)
}

fn widget(x: &mut String, w: &SurfaceWidget, cell: f32) {
    let frame = [w.x * cell + 4.0, w.y * cell + 4.0, w.w * cell - 8.0, w.h * cell - 8.0];
    let main = default_osc_address(&ControlTarget::Surface(w.id));
    let y = default_osc_address(&ControlTarget::SurfaceY(w.id));
    match w.kind {
        // buttonType: 0 momentary, 2 toggle on press.
        WidgetKind::Button => node(x, "BUTTON", &w.label, frame, w.color, &[("buttonType", 0)], &[(main, "x")]),
        WidgetKind::Toggle => node(x, "BUTTON", &w.label, frame, w.color, &[("buttonType", 2)], &[(main, "x")]),
        WidgetKind::Fader => node(x, "FADER", &w.label, frame, w.color, &[], &[(main, "x")]),
        // orientation 1 = east (horizontal).
        WidgetKind::Slider => node(x, "FADER", &w.label, frame, w.color, &[("orientation", 1)], &[(main, "x")]),
        WidgetKind::XyPad | WidgetKind::Color => node(x, "XY", &w.label, frame, w.color, &[], &[(main, "x"), (y, "y")]),
        WidgetKind::Label => node(x, "LABEL", &w.label, frame, w.color, &[], &[]),
        // (TouchOSC's radio needs its own count; a horizontal fader picks the choices the same way.)
        WidgetKind::Choices => node(x, "FADER", &w.label, frame, w.color, &[("orientation", 1)], &[(main, "x")]),
    }
}

/// The starter page when there's no Control Surface: master, layer opacities, column buttons.
fn starter(x: &mut String, comp: &Composition) {
    let accent = [0, 200, 190];
    node(x, "FADER", "Master", [16.0, TABS + 16.0, 90.0, 520.0], [255, 255, 255], &[], &[(default_osc_address(&ControlTarget::MasterFade), "x")]);
    let n = comp.layers.len().clamp(1, 8) as f32;
    let width = ((WIDTH - 140.0) / n).min(110.0);
    for (i, l) in comp.layers.iter().rev().take(8).enumerate() {
        let fx = 130.0 + i as f32 * width;
        node(x, "FADER", &l.name, [fx, TABS + 16.0, width - 12.0, 520.0], accent, &[], &[(default_osc_address(&ControlTarget::LayerOpacity(l.id)), "x")]);
    }
    for c in 0..8 {
        let fx = 16.0 + c as f32 * ((WIDTH - 32.0) / 8.0);
        node(x, "BUTTON", &format!("Column {}", c + 1), [fx, HEIGHT - 150.0, (WIDTH - 32.0) / 8.0 - 10.0, 110.0], [255, 196, 70], &[("buttonType", 0)], &[(default_osc_address(&ControlTarget::TriggerColumn(c)), "x")]);
    }
}

/// The layout as TouchOSC XML (before compression).
pub fn layout_xml(comp: &Composition) -> String {
    let sf = &comp.surface;
    let cell = WIDTH / sf.columns.max(1) as f32;
    let mut x = String::from("<?xml version='1.0' encoding='UTF-8'?><lexml version='3'>");
    let _ = write!(x, "<node ID='{}' type='GROUP'><properties>", uuid());
    prop_s(&mut x, "name", "Phosphor");
    prop_frame(&mut x, [0.0, 0.0, WIDTH, HEIGHT]);
    x.push_str("</properties><values/><messages/><children>");
    let _ = write!(x, "<node ID='{}' type='PAGER'><properties>", uuid());
    prop_s(&mut x, "name", "pages");
    prop_frame(&mut x, [0.0, 0.0, WIDTH, HEIGHT]);
    x.push_str("</properties><values/><messages/><children>");
    let pages: Vec<String> = if sf.widgets.is_empty() { vec!["Phosphor".into()] } else { sf.pages.clone() };
    for (p, name) in pages.iter().enumerate() {
        let _ = write!(x, "<node ID='{}' type='GROUP'><properties>", uuid());
        prop_s(&mut x, "name", name);
        prop_s(&mut x, "tabLabel", name);
        prop_frame(&mut x, [0.0, TABS, WIDTH, HEIGHT - TABS]);
        x.push_str("</properties><values/><messages/><children>");
        if sf.widgets.is_empty() {
            starter(&mut x, comp);
        } else {
            for w in sf.widgets.iter().filter(|w| w.page == p) {
                widget(&mut x, w, cell);
            }
        }
        x.push_str("</children></node>");
    }
    x.push_str("</children></node></children></node></lexml>");
    x
}

/// The `.tosc` file: the layout XML, zlib-compressed.
pub fn export(comp: &Composition) -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = z.write_all(layout_xml(comp).as_bytes());
    z.finish().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_controls_with_their_addresses() {
        let mut comp = Composition::default();
        comp.add_layer();
        let starter = layout_xml(&comp);
        assert!(starter.contains("/phosphor/master") && starter.contains("/phosphor/column/8"));
        let id = comp.surface.add(WidgetKind::XyPad);
        let xml = layout_xml(&comp);
        assert!(xml.contains(&format!("/phosphor/surface/{id}/y")));
        assert!(xml.contains("type='XY'"));
        // Round trip through zlib.
        let bytes = export(&comp);
        let mut back = String::new();
        std::io::Read::read_to_string(&mut flate2::read::ZlibDecoder::new(&bytes[..]), &mut back).unwrap();
        assert!(back.starts_with("<?xml") && back.contains(&format!("/phosphor/surface/{id}/y")) && back.ends_with("</lexml>"));
    }
}

/// Writes a sample layout to $PHOSPHOR_TOSC_OUT (for checking in TouchOSC by hand).
#[cfg(test)]
#[test]
fn write_sample_layout() {
    let Some(out) = std::env::var_os("PHOSPHOR_TOSC_OUT") else { return };
    let mut comp = Composition::default();
    comp.add_layer().name = "Rings".into();
    for k in [WidgetKind::Fader, WidgetKind::XyPad, WidgetKind::Button, WidgetKind::Toggle, WidgetKind::Slider, WidgetKind::Color] {
        comp.surface.add(k);
    }
    std::fs::write(out, export(&comp)).unwrap();
}
