//! VDMX 5 / 6 projects (.vdmx5, an XML property list): the media bin pages and their clips. Each page becomes
//! a "deck" for the same chooser as Resolume imports; pages go side by side on one Clips layer (VDMX triggers
//! clips onto any layer) and each page's files become a media collection. Layer setups live in archived
//! Cocoa objects inside the file and aren't read.

use super::resolume::{AvcClip, AvcDeck, AvcFile};

/// A property-list value (only what's needed here).
enum Value<'a> {
    Dict(Vec<(&'a str, Value<'a>)>),
    Array(Vec<Value<'a>>),
    Str(&'a str),
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
                .map(|(col, (path, name))| AvcClip { layer: 0, column: col, name, path: Some(path) })
                .collect();
            let name = page.get("name").and_then(Value::str).filter(|n| !n.is_empty()).map_or_else(|| format!("Page {}", i + 1), str::to_string);
            AvcDeck { name, columns: clips.len(), clips }
        })
        .collect();
    Ok(AvcFile { name: name.to_string(), width: 1920, height: 1080, bpm: None, layers: vec!["Clips".into()], decks })
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

    /// The owner's real VDMX projects, when they're on this Mac.
    #[test]
    fn reads_real_projects() {
        let Some(home) = std::env::var_os("HOME") else { return };
        let Ok(entries) = std::fs::read_dir(std::path::Path::new(&home).join("Documents/_VDMX")) else { return };
        let mut read_any = false;
        for e in entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "vdmx5")) {
            // Very old projects may be binary plists; those are reported, not imported.
            if let Ok(f) = read(&e.path().to_string_lossy()) {
                read_any |= f.decks.iter().any(|d| d.files() > 0);
            }
        }
        assert!(read_any, "at least one real project has clips");
    }
}
