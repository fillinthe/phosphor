//! Resolume Arena / Avenue compositions (.avc, XML): layers, decks and clips (video and image files) become a
//! Phosphor project — layers, clip columns, media bin, tempo and canvas size. Generators, effects and routing
//! that Resolume renders itself aren't carried over (they're counted so the import can say so).

use phosphor_engine::{Clip, Composition};
#[cfg(test)]
use phosphor_engine::SourceSpec;

/// One clip slot in a deck.
#[derive(Clone, Debug)]
pub struct AvcClip {
    pub layer: usize,
    pub column: usize,
    pub name: String,
    /// The media file, for video / image clips.
    pub path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AvcDeck {
    pub name: String,
    pub columns: usize,
    pub clips: Vec<AvcClip>,
}

impl AvcDeck {
    /// Clips with a media file.
    pub fn files(&self) -> usize {
        self.clips.iter().filter(|c| c.path.is_some()).count()
    }
}

/// What a composition file holds.
#[derive(Clone, Debug)]
pub struct AvcFile {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub bpm: Option<f32>,
    /// Layer names, bottom first (Resolume's layer 1 first).
    pub layers: Vec<String>,
    pub decks: Vec<AvcDeck>,
}

fn param<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|c| c.has_tag_name("Params"))?
        .children()
        .find(|p| p.attribute("name") == Some(name))?
        .attribute("value")
}

pub fn read(path: &str) -> Result<AvcFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("can't read it ({e})"))?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<AvcFile, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("not a Resolume composition ({e})"))?;
    let root = doc.root_element();
    if !root.has_tag_name("Composition") {
        return Err("not a Resolume composition (.avc)".into());
    }
    let info = root.children().find(|c| c.has_tag_name("CompositionInfo"));
    let num = |n: Option<roxmltree::Node>, a: &str| n.and_then(|n| n.attribute(a)).and_then(|v| v.parse::<u32>().ok());
    let deck_names: Vec<String> = info.map(|i| i.children().filter(|c| c.has_tag_name("DeckInfo")).filter_map(|d| d.attribute("name").map(str::to_string)).collect()).unwrap_or_default();
    let mut layers: Vec<(usize, String)> = root
        .children()
        .filter(|c| c.has_tag_name("Layer"))
        .enumerate()
        .map(|(i, l)| {
            let index = l.attribute("layerIndex").and_then(|v| v.parse().ok()).unwrap_or(i);
            (index, param(l, "Name").filter(|n| !n.is_empty()).map_or_else(|| format!("Layer {}", index + 1), str::to_string))
        })
        .collect();
    layers.sort_by_key(|(i, _)| *i);
    let bpm = root
        .children()
        .find(|c| c.has_tag_name("TempoController"))
        .and_then(|t| t.descendants().find(|p| p.attribute("name") == Some("Tempo")))
        .and_then(|p| p.attribute("value"))
        .and_then(|v| v.parse::<f32>().ok());
    let decks = root
        .children()
        .filter(|c| c.has_tag_name("Deck"))
        .enumerate()
        .map(|(d, deck)| {
            let clips: Vec<AvcClip> = deck
                .children()
                .filter(|c| c.has_tag_name("Clip"))
                .filter_map(|c| {
                    let layer = c.attribute("layerIndex")?.parse().ok()?;
                    let column = c.attribute("columnIndex")?.parse().ok()?;
                    let path = c
                        .descendants()
                        .find(|n| n.has_tag_name("VideoFile"))
                        .and_then(|v| v.attribute("value"))
                        .or_else(|| c.descendants().find(|n| n.has_tag_name("VideoFormatReaderSource")).and_then(|v| v.attribute("fileName")))
                        .map(str::to_string);
                    let name = param(c, "Name").map(str::to_string).unwrap_or_default();
                    Some(AvcClip { layer, column, name, path })
                })
                .collect();
            let columns = clips.iter().map(|c| c.column + 1).max().unwrap_or(0);
            let name = param(deck, "Name").map(str::to_string).or_else(|| deck_names.get(d).cloned()).unwrap_or_else(|| format!("Deck {}", d + 1));
            AvcDeck { name, columns, clips }
        })
        .collect();
    Ok(AvcFile {
        name: info.and_then(|i| i.attribute("name")).unwrap_or("Resolume composition").to_string(),
        width: num(info, "width").unwrap_or(1920),
        height: num(info, "height").unwrap_or(1080),
        bpm,
        layers: layers.into_iter().map(|(_, n)| n).collect(),
        decks,
    })
}

/// A name to show for a clip: Resolume's name, unless it's just the file's number, then the file name.
fn clip_name(c: &AvcClip) -> String {
    let file = c.path.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map(|s| s.to_string_lossy().to_string());
    match (&file, c.name.is_empty()) {
        (Some(f), true) => f.clone(),
        _ => c.name.clone(),
    }
}

/// The chosen decks as a Phosphor project: decks side by side (one after another in the clip columns).
/// Returns the project and how many clips were left out (generators, audio, Resolume-only sources).
pub fn to_composition(file: &AvcFile, decks: &[usize]) -> (Composition, usize) {
    let mut comp = Composition { width: file.width.max(16), height: file.height.max(16), ..Default::default() };
    if let Some(bpm) = file.bpm {
        comp.bpm = bpm.clamp(20.0, 300.0);
    }
    let n_layers = file.layers.len().max(decks.iter().filter_map(|d| file.decks.get(*d)).flat_map(|d| d.clips.iter().map(|c| c.layer + 1)).max().unwrap_or(0));
    let ids: Vec<u64> = (0..n_layers)
        .map(|i| {
            let l = comp.add_layer();
            l.name = file.layers.get(i).cloned().unwrap_or_else(|| format!("Layer {}", i + 1));
            l.id
        })
        .collect();
    let mut offset = 0;
    let mut skipped = 0;
    for deck in decks.iter().filter_map(|d| file.decks.get(*d)) {
        for c in &deck.clips {
            let Some(path) = &c.path else {
                skipped += 1;
                continue;
            };
            let name = clip_name(c);
            let id = comp.new_id();
            let Some(layer) = ids.get(c.layer).and_then(|lid| comp.layer_mut(*lid)) else { continue };
            *layer.clip_slot_mut(offset + c.column) = Clip { id, name: name.clone(), source: crate::media::spec_for_path(path), ..Default::default() };
            let file_name = std::path::Path::new(path).file_name().map_or(name, |f| f.to_string_lossy().to_string());
            comp.add_media(&file_name, path);
        }
        offset += deck.columns;
    }
    comp.clip_columns = offset.max(1);
    (comp, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Composition name="Composition" numDecks="2" numLayers="2">
  <CompositionInfo name="Gig" width="1280" height="720"><DeckInfo name="Warmup"/><DeckInfo name="Peak"/></CompositionInfo>
  <Layer uniqueId="1" layerIndex="0"><Params name="Params"><Param name="Name" value="Backgrounds"/></Params></Layer>
  <Layer uniqueId="2" layerIndex="1"><Params name="Params"><Param name="Name" value="Logos"/></Params></Layer>
  <TempoController name="TempoController"><Params name="Params"><ParamRange name="Tempo" value="128"/></Params></TempoController>
  <Deck name="Deck">
    <Clip layerIndex="0" columnIndex="0"><PreloadData><VideoFile value="/footage/a.mov"/></PreloadData><Params name="Params"><Param name="Name" value="Clouds"/></Params></Clip>
    <Clip layerIndex="1" columnIndex="2"><PreloadData><VideoFile value="/footage/logo.png"/></PreloadData><Params name="Params"><Param name="Name" value=""/></Params></Clip>
    <Clip layerIndex="0" columnIndex="1"><Params name="Params"><Param name="Name" value="Generator"/></Params></Clip>
  </Deck>
  <Deck name="Deck">
    <Clip layerIndex="0" columnIndex="0"><PreloadData><VideoFile value="/footage/b.mov"/></PreloadData></Clip>
  </Deck>
</Composition>"#;

    #[test]
    fn reads_decks_layers_and_clips() {
        let f = parse(SAMPLE).unwrap();
        assert_eq!((f.width, f.height, f.bpm), (1280, 720, Some(128.0)));
        assert_eq!(f.layers, vec!["Backgrounds", "Logos"]);
        assert_eq!(f.decks.len(), 2);
        assert_eq!((f.decks[0].name.as_str(), f.decks[0].columns, f.decks[0].files()), ("Warmup", 3, 2));
        let (comp, skipped) = to_composition(&f, &[0, 1]);
        assert_eq!(skipped, 1, "the generator clip");
        assert_eq!(comp.layers[0].name, "Backgrounds");
        assert_eq!(comp.layers[0].clips[0].name, "Clouds");
        assert!(matches!(comp.layers[1].clips[2].source, SourceSpec::Image { .. }), "logo.png is an image");
        assert_eq!(comp.layers[1].clips[2].name, "logo", "nameless clip takes the file name");
        assert!(matches!(&comp.layers[0].clips[3].source, SourceSpec::Movie { path: Some(p), .. } if p == "/footage/b.mov"), "deck 2 follows deck 1");
        assert_eq!(comp.clip_columns, 4);
        assert_eq!(comp.media_bin.len(), 3);
    }

    /// The owner's real compositions, when they're on this Mac.
    #[test]
    fn reads_real_compositions() {
        let Some(home) = std::env::var_os("HOME") else { return };
        let dir = std::path::Path::new(&home).join("Documents/Resolume Arena/Compositions");
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "avc")) {
            let f = read(&e.path().to_string_lossy()).unwrap_or_else(|err| panic!("{}: {err}", e.path().display()));
            assert!(!f.decks.is_empty() && !f.layers.is_empty(), "{}", e.path().display());
            let all: Vec<usize> = (0..f.decks.len()).collect();
            let (comp, _) = to_composition(&f, &all);
            assert!(comp.layers.iter().any(|l| !l.clips.is_empty()));
        }
    }
}

/// Converts $PHOSPHOR_AVC_IN (all decks) to a Phosphor project at $PHOSPHOR_AVC_OUT (for checking by hand).
#[cfg(test)]
#[test]
fn convert_sample() {
    let (Some(input), Some(out)) = (std::env::var_os("PHOSPHOR_AVC_IN"), std::env::var_os("PHOSPHOR_AVC_OUT")) else { return };
    let f = read(&input.to_string_lossy()).unwrap();
    let all: Vec<usize> = (0..f.decks.len()).collect();
    let (comp, skipped) = to_composition(&f, &all);
    println!("layers {} columns {} bin {} skipped {skipped}", comp.layers.len(), comp.clip_columns, comp.media_bin.len());
    std::fs::write(out, comp.to_json()).unwrap();
}
