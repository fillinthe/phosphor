//! VDMX 5 / 6 projects (.vdmx5, an XML property list): the media bin pages and their clips. Each page becomes
//! a "deck" for the same chooser as Resolume imports; pages go side by side on the first layer (VDMX triggers
//! clips onto any layer) and each page's files become a media collection. The layers come from the project's
//! first preset (archived Cocoa objects, `keyed`): their names, opacity, blend, fit and effect chains (ISF
//! effects as the same shader with its settings, Quartz Composer / Core Image ones as the nearest built-in), and
//! the main output's effects as the master effects.

use phosphor_engine::FitMode;
#[cfg(test)]
use phosphor_engine::BlendMode;

use super::keyed::Ar;
use super::looks::{LayerLook, LookEffect};
use super::resolume::{AvcClip, AvcDeck, AvcFile};

/// A property-list value (only what's needed here).
enum Value<'a> {
    Dict(Vec<(&'a str, Value<'a>)>),
    Array(Vec<Value<'a>>),
    Str(&'a str),
    /// Base64 text (a `<data>` element).
    Data(&'a str),
    Other,
}

impl<'a> Value<'a> {
    fn get(&self, key: &str) -> Option<&Value<'a>> {
        match self {
            Value::Dict(items) => items.iter().find(|(k, _)| *k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn str(&self) -> Option<&'a str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    fn array(&self) -> &[Value<'a>] {
        match self {
            Value::Array(a) => a,
            _ => &[],
        }
    }
}

fn value<'a>(node: roxmltree::Node<'a, 'a>) -> Value<'a> {
    match node.tag_name().name() {
        "dict" => {
            let mut items = Vec::new();
            let mut key = None;
            for c in node.children().filter(|c| c.is_element()) {
                if c.has_tag_name("key") {
                    key = c.text();
                } else if let Some(k) = key.take() {
                    items.push((k, value(c)));
                }
            }
            Value::Dict(items)
        }
        "array" => Value::Array(node.children().filter(|c| c.is_element()).map(value).collect()),
        "string" => Value::Str(node.text().unwrap_or("")),
        "data" => Value::Data(node.text().unwrap_or("")),
        _ => Value::Other,
    }
}

pub fn read(path: &str) -> Result<AvcFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("can't read it ({e})"))?;
    let name = std::path::Path::new(path).file_stem().map_or("VDMX project".into(), |s| s.to_string_lossy().to_string());
    parse(&text, &name)
}

pub fn parse(text: &str, name: &str) -> Result<AvcFile, String> {
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = roxmltree::Document::parse_with_options(text, opts).map_err(|e| format!("not a VDMX project ({e})"))?;
    let root = doc.root_element().children().find(|c| c.is_element()).map(value).ok_or("not a VDMX project")?;
    let pages = root.get("_projPages").and_then(|p| p.get("contents")).ok_or("no media bin in this VDMX project")?;
    let decks: Vec<AvcDeck> = pages
        .array()
        .iter()
        .enumerate()
        .map(|(i, page)| {
            let clips: Vec<AvcClip> = page
                .get("contents")
                .map(Value::array)
                .unwrap_or_default()
                .iter()
                .filter_map(|c| {
                    let path = c.get("mediaPath").and_then(Value::str).filter(|p| !p.is_empty())?;
                    let name = c.get("clipName").or_else(|| c.get("name")).and_then(Value::str).unwrap_or("").to_string();
                    Some((path.to_string(), name))
                })
                .enumerate()
                .map(|(col, (path, name))| AvcClip { layer: 0, column: col, name, path: Some(path), generator: None })
                .collect();
            let name = page.get("name").and_then(Value::str).filter(|n| !n.is_empty()).map_or_else(|| format!("Page {}", i + 1), str::to_string);
            AvcDeck { name, columns: clips.len(), clips }
        })
        .collect();
    let (looks, master, size) = root
        .get("_prsts")
        .and_then(|p| p.get("prsts"))
        .and_then(|p| p.array().first())
        .and_then(|p| match p.get("_layersSnapData") {
            Some(Value::Data(b64)) => Some(*b64),
            _ => None,
        })
        .and_then(|b64| {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.decode(b64.split_whitespace().collect::<String>()).ok()
        })
        .and_then(|bytes| super::keyed::unarchive(&bytes).ok())
        .map(|tree| layer_setup(&tree))
        .unwrap_or_default();
    let layers = if looks.is_empty() { vec!["Clips".into()] } else { looks.iter().map(|l| l.name.clone()).collect() };
    let (width, height) = size.unwrap_or((1920, 1080));
    Ok(AvcFile { name: name.to_string(), width, height, bpm: None, layers, decks, looks, master })
}

/// The layers (bottom first), the main output's effects and the canvas size from a preset's archived layer setup.
fn layer_setup(tree: &Ar) -> (Vec<LayerLook>, Vec<LookEffect>, Option<(u32, u32)>) {
    let looks = tree
        .get("lyrs")
        .map(Ar::list)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, ly)| {
            let pick = |key: &str| ly.at(&["CmpCtrlr", key, "selectedItemName"]).and_then(Ar::str).unwrap_or("").to_lowercase();
            let fit = match pick("sizeMode").as_str() {
                "fit" => Some(FitMode::Fit),
                "fill" => Some(FitMode::Fill),
                "stretch" => Some(FitMode::Stretch),
                _ => None,
            };
            LayerLook {
                name: ly.get("name").and_then(Ar::str).filter(|n| !n.is_empty()).map_or_else(|| format!("Layer {}", i + 1), str::to_string),
                opacity: ly.at(&["CmpCtrlr", "opacity", "val"]).and_then(Ar::num).map(|v| v as f32),
                blend: super::looks::blend_named(&pick("compMode")),
                fit,
                effects: effects(ly.get("FXCtrlr")),
            }
        })
        .collect();
    let master = effects(tree.get("FXCtrlr"));
    let size = tree.at(&["GrpCtrlr", "wide"]).and_then(Ar::num).zip(tree.at(&["GrpCtrlr", "high"]).and_then(Ar::num)).map(|(w, h)| (w.max(16.0) as u32, h.max(16.0) as u32));
    (looks, master, size)
}

/// An effect chain (`FXCtrlr`): each effect's file or name, on / off, dry / wet and settings.
fn effects(fx: Option<&Ar>) -> Vec<LookEffect> {
    fx.and_then(|f| f.at(&["fxChainSnap", "itemArray"]))
        .map(Ar::list)
        .unwrap_or_default()
        .iter()
        .map(|it| {
            let path = it.get("resourcePath").and_then(Ar::str).unwrap_or("").to_string();
            let name = std::path::Path::new(&path).file_stem().map_or_else(|| path.clone(), |s| s.to_string_lossy().to_string());
            let values = it.get("uiBuilder").map(Ar::dict).unwrap_or_default().iter().filter_map(|(k, v)| Some((k.clone(), v.get("val")?.num()? as f32))).collect();
            LookEffect {
                name: if path.ends_with(".qtz") { format!("{name}.qtz") } else { name },
                path: path.ends_with(".fs").then(|| path.clone()),
                enabled: it.at(&["enableButton", "val"]).and_then(Ar::num).is_none_or(|v| v != 0.0),
                mix: it.at(&["wetDrySlider", "val"]).and_then(Ar::num).map_or(1.0, |v| v as f32),
                values,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>_projPages</key><dict><key>contents</key><array>
    <dict><key>name</key><string>MIX</string><key>contents</key><array>
      <dict><key>clipName</key><string>clouds</string><key>mediaPath</key><string>/v/clouds.mov</string><key>uniqueID</key><integer>1</integer></dict>
      <dict><key>clipName</key><string>logo</string><key>mediaPath</key><string>/v/logo.png</string></dict>
    </array></dict>
    <dict><key>name</key><string>GIFS</string><key>contents</key><array>
      <dict><key>name</key><string>cat</string><key>mediaPath</key><string>/v/cat.gif</string></dict>
    </array></dict>
  </array></dict>
  <key>pvn</key><integer>1</integer>
</dict></plist>"#;

    #[test]
    fn reads_media_bin_pages() {
        let f = parse(SAMPLE, "Party").unwrap();
        assert_eq!(f.decks.len(), 2);
        assert_eq!((f.decks[0].name.as_str(), f.decks[0].files()), ("MIX", 2));
        assert_eq!(f.decks[1].clips[0].name, "cat");
        let (comp, skipped) = super::super::resolume::to_composition(&f, &[0, 1]);
        assert_eq!(skipped, 0);
        assert_eq!(comp.layers.len(), 1);
        assert_eq!(comp.layers[0].clips.iter().filter(|c| !c.is_empty()).count(), 3);
        assert!(comp.media_bin.iter().any(|m| m.collections == vec!["GIFS".to_string()]));
    }

    /// A preset's archived layer setup: two layers (names, opacity, blend, fit, effects) and a master effect.
    #[test]
    fn reads_layers_and_effects_from_the_first_preset() {
        use super::super::keyed::sample::Archive;
        use plist::Value;
        let mut a = Archive::new();
        let val = |a: &mut Archive, v: f64| a.dict(vec![("val", Value::Real(v))]);
        let pick = |a: &mut Archive, s: &str| {
            let s = a.string(s);
            a.dict(vec![("selectedItemName", s)])
        };
        let effect = |a: &mut Archive, path: &str, class: &str, on: bool, settings: Vec<(&str, f64)>| {
            let path = a.string(path);
            let class = a.string(class);
            let on = a.dict(vec![("val", Value::Boolean(on))]);
            let wet = val(a, 0.5);
            let items: Vec<(&str, Value)> = settings.into_iter().map(|(k, v)| (k, val(a, v))).collect();
            let ui = a.dict(items);
            a.dict(vec![("resourcePath", path), ("className", class), ("enableButton", on), ("wetDrySlider", wet), ("uiBuilder", ui)])
        };
        let chain = |a: &mut Archive, items: Vec<Value>| {
            let arr = a.array(items);
            let snap = a.dict(vec![("itemArray", arr)]);
            a.dict(vec![("fxChainSnap", snap)])
        };
        let layer = |a: &mut Archive, name: &str, opacity: f64, mode: &str, size: &str, fx: Vec<Value>| {
            let name = a.string(name);
            let op = val(a, opacity);
            let mode = pick(a, mode);
            let size = pick(a, size);
            let cmp = a.dict(vec![("opacity", op), ("compMode", mode), ("sizeMode", size)]);
            let fx = chain(a, fx);
            a.dict(vec![("name", name), ("CmpCtrlr", cmp), ("FXCtrlr", fx)])
        };
        let bad_tv = effect(&mut a, "/Library/Graphics/ISF/Bad TV.fs", "ISFFXObject", false, vec![("noiseLevel", 0.4)]);
        let mirror = effect(&mut a, "/Library/Compositions/Mirror.qtz", "QCFXObject", true, vec![]);
        let bottom = layer(&mut a, "ALL EFFECTS", 1.0, "OpenGL- Over", "Fill", vec![bad_tv, mirror]);
        let top = layer(&mut a, "Headliner", 0.0, "OpenGL- Add", "Fit", vec![]);
        let layers = a.array(vec![bottom, top]);
        let crop = effect(&mut a, "CICrop", "CIFXObject", true, vec![]);
        let kaleido = effect(&mut a, "CIKaleidoscope", "CIFXObject", true, vec![]);
        let master = chain(&mut a, vec![crop, kaleido]);
        let wide = Value::Integer(1280.into());
        let high = Value::Integer(720.into());
        let grp = a.dict(vec![("wide", wide), ("high", high)]);
        let root = a.dict(vec![("lyrs", layers), ("FXCtrlr", master), ("GrpCtrlr", grp)]);
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(a.bytes(root));
        let xml = SAMPLE.replace("<key>pvn</key>", &format!("<key>_prsts</key><dict><key>prsts</key><array><dict><key>name</key><string>BEGIN</string><key>_layersSnapData</key><data>{b64}</data></dict></array></dict><key>pvn</key>"));
        let f = parse(&xml, "Party").unwrap();
        assert_eq!(f.layers, vec!["ALL EFFECTS".to_string(), "Headliner".to_string()]);
        assert_eq!((f.width, f.height), (1280, 720));
        let bottom = &f.looks[0];
        assert_eq!((bottom.opacity, bottom.blend, bottom.fit), (Some(1.0), Some(BlendMode::Normal), Some(FitMode::Fill)));
        assert_eq!(bottom.effects[0].path.as_deref(), Some("/Library/Graphics/ISF/Bad TV.fs"));
        assert_eq!((bottom.effects[0].enabled, bottom.effects[0].mix, bottom.effects[0].values.clone()), (false, 0.5, vec![("noiseLevel".to_string(), 0.4)]));
        assert_eq!(bottom.effects[1].name, "Mirror.qtz");
        assert_eq!((f.looks[1].opacity, f.looks[1].blend, f.looks[1].fit), (Some(0.0), Some(BlendMode::Add), Some(FitMode::Fit)));
        let (comp, _) = super::super::resolume::to_composition(&f, &[0, 1]);
        assert_eq!(comp.layers.len(), 2);
        assert_eq!(comp.layers[0].clips.iter().filter(|c| !c.is_empty()).count(), 3, "the pages' clips on the first layer");
        assert_eq!(comp.layers[0].effects.iter().map(|e| e.effect.as_str()).collect::<Vec<_>>(), vec!["glitch", "mirror"], "Bad TV and Mirror by their nearest built-ins here");
        assert_eq!(comp.layers[1].blend, BlendMode::Add);
        assert_eq!(comp.master_effects.iter().map(|e| e.effect.as_str()).collect::<Vec<_>>(), vec!["kaleidoscope"], "the crop is left out");
        let (stand_ins, left) = super::super::looks::report(&f.looks, &f.master);
        assert_eq!((stand_ins.len(), left), (3, vec!["CICrop".to_string()]));
    }

    /// Real VDMX projects in the folder $PHOSPHOR_VDMX_DIR: each reads, with its layers.
    #[test]
    fn reads_real_projects() {
        let Some(dir) = std::env::var_os("PHOSPHOR_VDMX_DIR") else { return };
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let (mut read_any, mut layers) = (false, 0);
        for e in entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "vdmx5")) {
            // Very old projects may be binary plists; those are reported, not imported.
            if let Ok(f) = read(&e.path().to_string_lossy()) {
                read_any |= f.decks.iter().any(|d| d.files() > 0);
                layers += f.looks.len();
                let all: Vec<usize> = (0..f.decks.len()).collect();
                super::super::resolume::to_composition(&f, &all);
            }
        }
        assert!(read_any, "at least one real project has clips");
        println!("VDMX layers read: {layers}");
    }
}
