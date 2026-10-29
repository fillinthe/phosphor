//! Resolume's Advanced Output (a screen setup preset, XML): each screen becomes a Phosphor output with its slices
//! and masks. A slice keeps where it lands (its warped corners: the homography's, or its output rectangle) and the
//! part of the composition it shows (its input rectangle), or the layer it takes ("Input Source" 3:N = layer N).
//! Bezier warps beyond the four corners aren't carried over. Resolume doesn't save which setup a composition used:
//! the one in use last (Preferences › AdvancedOutput.xml) is offered.

use phosphor_engine::Composition;
use phosphor_engine::mapping::{Mask, Surface};

#[derive(Clone, Debug, PartialEq)]
pub struct ScreenSetup {
    pub name: String,
    /// The composition size the input rectangles are measured in.
    pub comp_size: (f32, f32),
    pub screens: Vec<Screen>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub name: String,
    pub enabled: bool,
    pub size: (f32, f32),
    pub slices: Vec<SliceIn>,
    pub masks: Vec<MaskIn>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SliceIn {
    pub name: String,
    pub enabled: bool,
    /// Composition pixels.
    pub input: [[f32; 2]; 4],
    /// Screen pixels, top-left, top-right, bottom-right, bottom-left of the slice's own picture.
    pub corners: [[f32; 2]; 4],
    /// The layer it shows (1 = the bottom one), None = the composition.
    pub layer: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaskIn {
    pub name: String,
    pub enabled: bool,
    pub invert: bool,
    /// Screen pixels.
    pub points: Vec<[f32; 2]>,
}

fn params<'a>(node: roxmltree::Node<'a, 'a>, group: &str, name: &str) -> Option<&'a str> {
    node.children().filter(|c| c.has_tag_name("Params") && c.attribute("name") == Some(group)).flat_map(|p| p.children()).find(|p| p.attribute("name") == Some(name))?.attribute("value")
}

fn points(node: Option<roxmltree::Node>) -> Vec<[f32; 2]> {
    node.map(|n| n.children().filter(|v| v.has_tag_name("v")).map(|v| [v.attribute("x").and_then(|x| x.parse().ok()).unwrap_or(0.0), v.attribute("y").and_then(|y| y.parse().ok()).unwrap_or(0.0)]).collect()).unwrap_or_default()
}

fn four(p: &[[f32; 2]]) -> Option<[[f32; 2]; 4]> {
    (p.len() == 4).then(|| [p[0], p[1], p[2], p[3]])
}

pub fn read(path: &std::path::Path) -> Result<ScreenSetup, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("can't read it ({e})"))?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<ScreenSetup, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("not a Resolume screen setup ({e})"))?;
    let setup = doc.descendants().find(|n| n.has_tag_name("ScreenSetup")).ok_or("not a Resolume screen setup")?;
    let name = doc.root_element().attribute("name").unwrap_or("Advanced Output").to_string();
    let size = |n: Option<roxmltree::Node>, w: &str, h: &str| n.and_then(|n| Some((n.attribute(w)?.parse::<f32>().ok()?, n.attribute(h)?.parse::<f32>().ok()?)));
    let comp_size = size(setup.children().find(|c| c.has_tag_name("CurrentCompositionTextureSize")), "width", "height").unwrap_or((1920.0, 1080.0));
    let mut screens = Vec::new();
    for s in setup.descendants().filter(|n| n.has_tag_name("Screen")) {
        let device = s.descendants().find(|n| n.parent().is_some_and(|p| p.has_tag_name("OutputDevice")));
        let screen_size = size(device, "width", "height").unwrap_or(comp_size);
        let mut screen = Screen {
            name: params(s, "Params", "Name").filter(|n| !n.is_empty()).or(s.attribute("name")).unwrap_or("Screen").to_string(),
            enabled: params(s, "Params", "Enabled") != Some("0"),
            size: (screen_size.0.max(1.0), screen_size.1.max(1.0)),
            slices: Vec::new(),
            masks: Vec::new(),
        };
        let Some(items) = s.children().find(|c| c.has_tag_name("layers")) else { continue };
        for item in items.children().filter(|c| c.is_element()) {
            let name = params(item, "Common", "Name").unwrap_or("").to_string();
            let enabled = params(item, "Common", "Enabled") != Some("0");
            if item.has_tag_name("Slice") {
                let warped = item.descendants().find(|n| n.has_tag_name("Homography")).and_then(|h| h.children().find(|c| c.has_tag_name("dst")));
                let corners = four(&points(warped)).or_else(|| four(&points(item.children().find(|c| c.has_tag_name("OutputRect")))));
                let input = four(&points(item.children().find(|c| c.has_tag_name("InputRect"))));
                let (Some(corners), Some(input)) = (corners, input) else { continue };
                let layer = params(item, "Input", "Input Source").and_then(|v| v.strip_prefix("3:")).and_then(|n| n.parse::<usize>().ok()).filter(|n| *n > 0);
                screen.slices.push(SliceIn { name, enabled, input, corners, layer });
            } else if item.has_tag_name("Mask") {
                let shape = item.descendants().find(|n| n.has_tag_name("points"));
                let outline = points(shape);
                let outline = if outline.len() >= 3 { outline } else { points(item.descendants().find(|n| n.has_tag_name("Rect"))) };
                if outline.len() >= 3 {
                    screen.masks.push(MaskIn { name, enabled, invert: params(item, "Output", "Invert") == Some("1"), points: outline });
                }
            }
        }
        screens.push(screen);
    }
    Ok(ScreenSetup { name, comp_size, screens })
}

/// The screen setup Resolume (Arena, else Avenue) used last, by its preferences, when it's on this computer.
pub fn current() -> Option<std::path::PathBuf> {
    let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
    for app in ["Resolume Arena", "Resolume Avenue"] {
        let dir = home.join("Documents").join(app);
        let Ok(prefs) = std::fs::read_to_string(dir.join("Preferences/AdvancedOutput.xml")) else { continue };
        let Ok(doc) = roxmltree::Document::parse(&prefs) else { continue };
        let Some(name) = doc.descendants().find_map(|n| n.attribute("presetFile")) else { continue };
        let path = dir.join("Presets/Advanced Output").join(format!("{name}.xml"));
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Adds an output per screen with its slices and masks. `layers` = the project's layer ids, bottom first (a slice
/// that takes layer N shows `layers[N - 1]`). Returns (outputs, slices, masks) added.
pub fn add_to(comp: &mut Composition, setup: &ScreenSetup, layers: &[u64]) -> (usize, usize, usize) {
    let (mut outs, mut slices, mut masks) = (0, 0, 0);
    let (cw, ch) = setup.comp_size;
    for s in &setup.screens {
        let id = comp.add_output();
        let mut list = Vec::new();
        let mut mask_list = Vec::new();
        for sl in &s.slices {
            let xs = sl.input.iter().map(|p| p[0] / cw.max(1.0));
            let ys = sl.input.iter().map(|p| p[1] / ch.max(1.0));
            let (x0, x1) = (xs.clone().fold(f32::MAX, f32::min), xs.fold(f32::MIN, f32::max));
            let (y0, y1) = (ys.clone().fold(f32::MAX, f32::min), ys.fold(f32::MIN, f32::max));
            let at = |p: [f32; 2]| [p[0] / s.size.0, p[1] / s.size.1];
            list.push(Surface {
                id: comp.new_id(),
                name: if sl.name.is_empty() { format!("Slice {}", list.len() + 1) } else { sl.name.clone() },
                enabled: sl.enabled,
                corners: sl.corners.map(at),
                input: [x0, y0, (x1 - x0).max(1e-4), (y1 - y0).max(1e-4)],
                layer: sl.layer.and_then(|n| layers.get(n - 1).copied()),
                ..Default::default()
            });
        }
        for m in &s.masks {
            mask_list.push(Mask { id: comp.new_id(), name: m.name.clone(), enabled: m.enabled, invert: m.invert, points: m.points.iter().map(|p| [p[0] / s.size.0, p[1] / s.size.1]).collect(), ..Default::default() });
        }
        let out = comp.output_mut(id).unwrap();
        out.name = s.name.clone();
        out.enabled = s.enabled;
        (out.width, out.height) = (s.size.0 as u32, s.size.1 as u32);
        out.mapping.enabled = true;
        slices += list.len();
        masks += mask_list.len();
        out.mapping.surfaces = list;
        out.mapping.masks = mask_list;
        outs += 1;
    }
    (outs, slices, masks)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<XmlState name="Club">
  <ScreenSetup name="ScreenSetup">
    <CurrentCompositionTextureSize width="1920" height="1080"/>
    <screens>
      <Screen name="Projector" uniqueId="1">
        <Params name="Params"><Param name="Name" value="Projector"/><Param name="Enabled" value="1"/></Params>
        <layers>
          <Slice uniqueId="2">
            <Params name="Common"><Param name="Name" value="Left half"/><Param name="Enabled" value="1"/></Params>
            <Params name="Input"><ParamChoice name="Input Source" value="0:1"/></Params>
            <InputRect orientation="0"><v x="0" y="0"/><v x="960" y="0"/><v x="960" y="1080"/><v x="0" y="1080"/></InputRect>
            <OutputRect orientation="0"><v x="0" y="0"/><v x="640" y="0"/><v x="640" y="720"/><v x="0" y="720"/></OutputRect>
            <Warper><Homography><src><v x="0" y="0"/><v x="640" y="0"/><v x="640" y="720"/><v x="0" y="720"/></src>
              <dst><v x="64" y="0"/><v x="640" y="0"/><v x="640" y="720"/><v x="0" y="720"/></dst></Homography></Warper>
          </Slice>
          <Slice uniqueId="3">
            <Params name="Common"><Param name="Name" value="Logo"/><Param name="Enabled" value="0"/></Params>
            <Params name="Input"><ParamChoice name="Input Source" value="3:2"/></Params>
            <InputRect orientation="0"><v x="0" y="0"/><v x="1920" y="0"/><v x="1920" y="1080"/><v x="0" y="1080"/></InputRect>
            <OutputRect orientation="0"><v x="640" y="360"/><v x="1280" y="360"/><v x="1280" y="720"/><v x="640" y="720"/></OutputRect>
          </Slice>
          <Mask uniqueId="4">
            <Params name="Common"><Param name="Name" value="Cut"/><Param name="Enabled" value="1"/></Params>
            <Params name="Output"><Param name="Invert" value="1"/></Params>
            <ShapeObject><Rect><v x="0" y="0"/><v x="1" y="0"/><v x="1" y="1"/><v x="0" y="1"/></Rect>
              <Shape><Contour closed="1"><points><v x="0" y="0"/><v x="1280" y="0"/><v x="640" y="720"/></points></Contour></Shape></ShapeObject>
          </Mask>
        </layers>
        <OutputDevice><OutputDeviceVirtual name="Projector" width="1280" height="720"/></OutputDevice>
      </Screen>
    </screens>
  </ScreenSetup>
</XmlState>"#;

    #[test]
    fn screens_become_outputs_with_slices_and_masks() {
        let setup = parse(SAMPLE).unwrap();
        assert_eq!(setup.name, "Club");
        let s = &setup.screens[0];
        assert_eq!((s.name.as_str(), s.size, s.slices.len(), s.masks.len()), ("Projector", (1280.0, 720.0), 2, 1));
        assert_eq!(s.slices[0].corners[0], [64.0, 0.0], "the warped corner");
        assert_eq!((s.slices[1].layer, s.slices[1].enabled), (Some(2), false));
        let mut comp = Composition::default();
        let ids = [comp.add_layer().id, comp.add_layer().id];
        assert_eq!(add_to(&mut comp, &setup, &ids), (1, 2, 1));
        let out = &comp.outputs[0];
        assert_eq!((out.name.as_str(), out.width, out.height), ("Projector", 1280, 720));
        let left = &out.mapping.surfaces[0];
        assert_eq!(left.corners, [[0.05, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]]);
        assert_eq!(left.input, [0.0, 0.0, 0.5, 1.0]);
        assert_eq!(left.layer, None, "the composition");
        assert_eq!(out.mapping.surfaces[1].layer, Some(ids[1]), "layer 2");
        let mask = &out.mapping.masks[0];
        assert!(mask.invert && mask.points == vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]);
    }

    /// The Advanced Output Resolume used last on this Mac, when there is one.
    #[test]
    fn reads_the_current_setup() {
        let Some(path) = current() else { return };
        let setup = read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut comp = Composition::default();
        let ids: Vec<u64> = (0..8).map(|_| comp.add_layer().id).collect();
        let (outs, slices, _) = add_to(&mut comp, &setup, &ids);
        assert!(outs > 0 && slices > 0);
    }
}
