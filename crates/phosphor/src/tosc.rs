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
        WidgetKind::Label | WidgetKind::Text => node(x, "LABEL", &w.label, frame, w.color, &[], &[]),
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

/// A TouchOSC node's property by key.
fn prop<'a, 'i>(n: roxmltree::Node<'a, 'i>, key: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name("properties"))?.children().find(|p| p.has_tag_name("property") && p.children().any(|k| k.has_tag_name("key") && k.text() == Some(key)))
}

/// What an import brought in.
pub struct Imported {
    pub pages: usize,
    pub widgets: usize,
    pub mappings: usize,
}

/// Reads a TouchOSC layout (`.tosc`, or its plain XML) into the Control Surface: each TouchOSC page becomes a surface
/// page, each fader / button / XY pad / radio / label a widget in the same place (scaled to the grid), and each
/// control's OSC address is mapped to its widget, so the layout drives Phosphor as it is.
pub fn import(comp: &mut Composition, bytes: &[u8]) -> Result<Imported, String> {
    let text = if bytes.first() == Some(&b'<') {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        let mut out = String::new();
        std::io::Read::read_to_string(&mut flate2::read::ZlibDecoder::new(bytes), &mut out).map_err(|e| format!("not a TouchOSC file ({e})"))?;
        out
    };
    let doc = roxmltree::Document::parse(&text).map_err(|e| format!("not a TouchOSC layout ({e})"))?;
    let value_text = |n: roxmltree::Node, key: &str| prop(n, key).and_then(|p| p.children().find(|c| c.has_tag_name("value"))).and_then(|v| v.text()).map(str::to_string);
    let frame = |n: roxmltree::Node| -> [f32; 4] {
        let Some(v) = prop(n, "frame").and_then(|p| p.children().find(|c| c.has_tag_name("value"))) else { return [0.0; 4] };
        let get = |k: &str| v.children().find(|c| c.has_tag_name(k)).and_then(|c| c.text()).and_then(|t| t.trim().parse::<f32>().ok()).unwrap_or(0.0);
        [get("x"), get("y"), get("w"), get("h")]
    };
    let color = |n: roxmltree::Node| -> Option<[u8; 3]> {
        let v = prop(n, "color")?.children().find(|c| c.has_tag_name("value"))?;
        let get = |k: &str| v.children().find(|c| c.has_tag_name(k)).and_then(|c| c.text()).and_then(|t| t.trim().parse::<f32>().ok()).unwrap_or(1.0);
        Some([(get("r") * 255.0) as u8, (get("g") * 255.0) as u8, (get("b") * 255.0) as u8])
    };
    // A control's OSC messages: (address, the variable it sends: x / y).
    let messages = |n: roxmltree::Node| -> Vec<(String, String)> {
        let Some(m) = n.children().find(|c| c.has_tag_name("messages")) else { return Vec::new() };
        m.children()
            .filter(|o| o.has_tag_name("osc"))
            .filter_map(|o| {
                let path: String = o.children().find(|c| c.has_tag_name("path"))?.children().filter(|p| p.has_tag_name("partial")).filter_map(|p| p.children().find(|c| c.has_tag_name("value")).and_then(|v| v.text())).collect();
                let var = o.descendants().find(|c| c.has_tag_name("var")).and_then(|v| v.text()).unwrap_or("x").to_string();
                path.starts_with('/').then_some((path, var))
            })
            .collect()
    };
    let root = doc.root_element().children().find(|n| n.has_tag_name("node")).ok_or("empty layout")?;
    let root_w = frame(root)[2].max(1.0);
    let cell = root_w / comp.surface.columns.max(1) as f32;
    // Pages: the PAGER's children, else the whole layout as one page.
    let pager = root.descendants().find(|n| n.attribute("type") == Some("PAGER"));
    let pages: Vec<roxmltree::Node> = match pager {
        Some(p) => p.children().filter(|c| c.has_tag_name("children")).flat_map(|c| c.children().filter(|n| n.has_tag_name("node"))).collect(),
        None => vec![root],
    };
    let mut out = Imported { pages: 0, widgets: 0, mappings: 0 };
    let first_new_page = comp.surface.pages.len();
    for (k, page) in pages.iter().enumerate() {
        let name = value_text(*page, "tabLabel").or_else(|| value_text(*page, "name")).unwrap_or_else(|| format!("TouchOSC {}", k + 1));
        comp.surface.pages.push(name);
        let page_index = first_new_page + k;
        out.pages += 1;
        let page_origin = frame(*page);
        for n in page.descendants().filter(|n| n.has_tag_name("node") && *n != *page) {
            let kind = match (n.attribute("type"), value_text(n, "buttonType").as_deref(), value_text(n, "orientation").as_deref()) {
                (Some("BUTTON"), Some("0") | None, _) => WidgetKind::Button,
                (Some("BUTTON"), ..) => WidgetKind::Toggle,
                (Some("FADER"), _, Some("1") | Some("3")) | (Some("ENCODER"), ..) => WidgetKind::Slider,
                (Some("FADER"), ..) => WidgetKind::Fader,
                (Some("XY"), ..) => WidgetKind::XyPad,
                (Some("RADIO"), ..) => WidgetKind::Choices,
                (Some("LABEL"), ..) => WidgetKind::Label,
                _ => continue,
            };
            // Frames are inside their parent group's: add up the offsets up to the page.
            let mut f = frame(n);
            for anc in n.ancestors().skip(1).filter(|a| a.has_tag_name("node")).take_while(|a| a != page) {
                let af = frame(anc);
                f[0] += af[0];
                f[1] += af[1];
            }
            let _ = page_origin;
            let id = comp.surface.add(kind);
            let w = comp.surface.widget_mut(id).expect("just added");
            w.page = page_index;
            w.x = (f[0] / cell).round().max(0.0);
            w.y = (f[1] / cell).round().max(0.0);
            w.w = (f[2] / cell).round().max(1.0);
            w.h = (f[3] / cell).round().max(1.0);
            w.label = if kind == WidgetKind::Label { value_text(n, "text") } else { None }.or_else(|| value_text(n, "name")).unwrap_or_default();
            if let Some(c) = color(n) {
                w.color = c;
            }
            if kind == WidgetKind::Choices {
                let steps = value_text(n, "steps").and_then(|s| s.parse::<usize>().ok()).unwrap_or(4).clamp(2, 32);
                w.items = (1..=steps).map(|i| i.to_string()).collect();
            }
            out.widgets += 1;
            for (address, var) in messages(n) {
                let target = if var == "y" { ControlTarget::SurfaceY(id) } else { ControlTarget::Surface(id) };
                comp.control_maps.push(phosphor_engine::ControlMap::new(phosphor_engine::ControlSource::Osc { address }, target));
                out.mappings += 1;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_comes_back_in() {
        let mut comp = Composition::default();
        let fader = comp.surface.add(WidgetKind::Fader);
        comp.surface.widget_mut(fader).unwrap().label = "Dimmer".into();
        comp.surface.add(WidgetKind::XyPad);
        comp.surface.add(WidgetKind::Label);
        let bytes = export(&comp);
        let mut fresh = Composition::default();
        let got = import(&mut fresh, &bytes).unwrap();
        assert_eq!((got.pages, got.widgets, got.mappings), (1, 3, 3), "fader x, pad x and y");
        let w = fresh.surface.widgets.iter().find(|w| w.label == "Dimmer").expect("the fader");
        assert_eq!((w.kind, w.page), (WidgetKind::Fader, 1));
        let orig = comp.surface.widget(fader).unwrap();
        assert_eq!((w.x, w.y, w.w, w.h), (orig.x, orig.y, orig.w, orig.h), "same place on the grid");
        assert!(fresh.control_maps.iter().any(|m| m.source == phosphor_engine::ControlSource::Osc { address: format!("/phosphor/surface/{fader}") } && m.target == ControlTarget::Surface(w.id)));
        assert!(import(&mut fresh, b"not a layout").is_err());
    }

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
