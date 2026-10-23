//! Real lights by name, from the Open Fixture Library (open-fixture-library.org, MIT; its files on GitHub): search
//! the library, pick a light and one of its DMX modes, and it becomes a fixture with that mode's channels in order
//! (`HeadLayout::Custom` + `Role`s). Colour, dimmer, pan and tilt follow Phosphor; every other channel (gobo, strobe,
//! prism, macros…) becomes a setting of its own, starting where the light is plain and open.

use std::sync::mpsc::{Receiver, channel};

use phosphor_engine::{ExtraChannel, Fixture, FixtureShape, HeadLayout, Role};
use serde_json::Value;

const REPO: &str = "OpenLightingProject/open-fixture-library";

/// One light in the library.
#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    /// "chauvet-dj/intimidator-spot-110"
    pub key: String,
    pub maker: String,
    pub name: String,
}

/// Every light in the library (from its file list on GitHub, kept for the session), with the makers' real names
/// from its `manufacturers.json` when there (`makers`).
pub fn index(tree: &[u8], makers: Option<&[u8]>) -> Result<Vec<Light>, String> {
    let v: Value = serde_json::from_slice(tree).map_err(|_| "couldn't read the library's list".to_string())?;
    let makers: Value = makers.and_then(|m| serde_json::from_slice(m).ok()).unwrap_or_default();
    let maker_of = |slug: &str| makers[slug]["name"].as_str().map_or_else(|| words(slug), str::to_string);
    let mut out: Vec<Light> = v["tree"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| {
            let path = f["path"].as_str()?;
            let key = path.strip_prefix("fixtures/")?.strip_suffix(".json")?;
            let (maker, name) = key.split_once('/')?;
            (!name.contains('/')).then(|| Light { key: key.to_string(), maker: maker_of(maker), name: words(name) })
        })
        .collect();
    out.sort_by(|a, b| (a.maker.to_lowercase(), a.name.to_lowercase()).cmp(&(b.maker.to_lowercase(), b.name.to_lowercase())));
    Ok(out)
}

/// "intimidator-spot-110" → "Intimidator Spot 110" (until the light's own file gives its real name).
fn words(slug: &str) -> String {
    slug.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lights whose maker or name holds every word of `query`.
pub fn search<'a>(all: &'a [Light], query: &str) -> Vec<&'a Light> {
    let q: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    all.iter().filter(|l| q.iter().all(|w| format!("{} {} {}", l.maker, l.name, l.key).to_lowercase().contains(w.as_str()))).collect()
}

/// A light's file: its name, kinds ("Moving Head", "Color Changer"…) and modes (name, channel names).
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub name: String,
    pub categories: Vec<String>,
    pub modes: Vec<(String, Vec<String>)>,
    json: Value,
}

pub fn definition(body: &[u8]) -> Result<Definition, String> {
    let json: Value = serde_json::from_slice(body).map_err(|_| "couldn't read the light's file".to_string())?;
    let text = |v: &Value| v.as_str().unwrap_or("").to_string();
    let modes = json["modes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            // Unused channels are null: they still take a slot.
            let chans = m["channels"].as_array().into_iter().flatten().map(|c| c.as_str().map(str::to_string).unwrap_or_default()).collect();
            (text(&m["name"]), chans)
        })
        .collect::<Vec<_>>();
    if modes.is_empty() {
        return Err("this light has no DMX modes in the library".into());
    }
    Ok(Definition { name: text(&json["name"]), categories: json["categories"].as_array().into_iter().flatten().map(text).collect(), modes, json })
}

impl Definition {
    /// The light as a fixture in `mode` (its channels in order); place, universe and channel are the caller's.
    pub fn fixture(&self, maker: &str, mode: usize) -> Fixture {
        let (_, chans) = &self.modes[mode.min(self.modes.len() - 1)];
        let available = &self.json["availableChannels"];
        // A fine channel is listed under its coarse one's `fineChannelAliases`.
        let fine_of = |name: &str| -> Option<&Value> {
            available.as_object()?.values().find(|c| c["fineChannelAliases"].as_array().is_some_and(|a| a.iter().any(|x| x.as_str() == Some(name))))
        };
        let mut roles = Vec::new();
        let mut extra: Vec<ExtraChannel> = Vec::new();
        for name in chans {
            let role = if let Some(coarse) = fine_of(name) {
                match capability_type(coarse) {
                    "Pan" => Role::PanFine,
                    "Tilt" => Role::TiltFine,
                    _ => Role::Zero,
                }
            } else {
                let ch = &available[name.as_str()];
                match (capability_type(ch), ch["capability"]["color"].as_str().unwrap_or("")) {
                    ("Pan", _) => Role::Pan,
                    ("Tilt", _) => Role::Tilt,
                    ("Intensity", _) => Role::Dimmer,
                    ("ColorIntensity", c) => match c {
                        "Red" => Role::Red,
                        "Green" => Role::Green,
                        "Blue" => Role::Blue,
                        "White" | "Warm White" | "Cold White" => Role::White,
                        "Amber" => Role::Amber,
                        "UV" => Role::Uv,
                        "Cyan" => Role::Cyan,
                        "Magenta" => Role::Magenta,
                        "Yellow" => Role::Yellow,
                        _ => Role::Zero,
                    },
                    _ if name.is_empty() => Role::Zero,
                    _ => {
                        extra.push(ExtraChannel { name: name.clone(), value: resting(ch) as f32 / 255.0 });
                        Role::Extra(extra.len() as u16 - 1)
                    }
                }
            };
            roles.push(role);
        }
        Fixture {
            name: format!("{maker} {}", self.name).trim().to_string(),
            shape: FixtureShape::MovingHead,
            head: HeadLayout::Custom,
            roles,
            extra,
            gamma: 1.0,
            ..Default::default()
        }
    }

    /// "12 ch: Pan, Pan fine, Tilt…" for a mode.
    pub fn mode_summary(&self, mode: usize) -> String {
        let (_, chans) = &self.modes[mode];
        let names: Vec<&str> = chans.iter().map(|c| if c.is_empty() { "–" } else { c.as_str() }).collect();
        format!("{} ch: {}", chans.len(), names.join(", "))
    }
}

/// A channel's one capability type, if it has just one.
fn capability_type(ch: &Value) -> &str {
    ch["capability"]["type"].as_str().unwrap_or("")
}

/// Where a channel rests: its `defaultValue`, else the start of an "open" shutter range (so the light shows), else
/// the start of a "no function" range, else 0.
fn resting(ch: &Value) -> u8 {
    if let Some(d) = ch["defaultValue"].as_u64() {
        return d.min(255) as u8;
    }
    let caps = ch["capabilities"].as_array();
    let start = |pick: &dyn Fn(&Value) -> bool| caps.into_iter().flatten().find(|c| pick(c)).and_then(|c| c["dmxRange"][0].as_u64()).map(|v| v.min(255) as u8);
    start(&|c| c["type"] == "ShutterStrobe" && c["shutterEffect"] == "Open")
        .or_else(|| start(&|c| c["type"] == "NoFunction"))
        .unwrap_or(0)
}

// ---- fetching ----

enum Msg {
    Index(Result<Vec<Light>, String>),
    Definition(String, Result<Definition, String>),
}

/// The "Find a light" window's fetching (in `State::ofl`).
pub struct Library {
    tx: std::sync::mpsc::Sender<Msg>,
    rx: Receiver<Msg>,
    pub lights: Option<Result<Vec<Light>, String>>,
    asked: bool,
    /// The light being looked at, and its file once here.
    pub open: Option<(Light, Option<Result<Definition, String>>)>,
    /// Bench: look at the first light matching this once the list is here.
    pub bench_pick: Option<String>,
}

impl Default for Library {
    fn default() -> Self {
        let (tx, rx) = channel();
        Library { tx, rx, lights: None, asked: false, open: None, bench_pick: None }
    }
}

impl Library {
    /// Fetches the list once; takes in answers. Call while the window is open.
    pub fn poll(&mut self, ctx: &eframe::egui::Context) {
        if !self.asked {
            self.asked = true;
            let (tx, ctx) = (self.tx.clone(), ctx.clone());
            let _ = std::thread::Builder::new().name("fixture library".into()).spawn(move || {
                let url = format!("https://api.github.com/repos/{REPO}/git/trees/master?recursive=1");
                let makers = crate::online::get_cached(&format!("https://raw.githubusercontent.com/{REPO}/master/fixtures/manufacturers.json")).ok();
                let _ = tx.send(Msg::Index(crate::online::get_cached(&url).and_then(|b| index(&b, makers.as_deref()))));
                ctx.request_repaint();
            });
        }
        let news: Vec<Msg> = self.rx.try_iter().collect();
        for m in news {
            match m {
                Msg::Index(r) => {
                    let first = self.bench_pick.take().and_then(|q| r.as_ref().ok().and_then(|all| search(all, &q).first().map(|l| (*l).clone())));
                    self.lights = Some(r);
                    if let Some(l) = first {
                        self.look_at(&l, ctx);
                    }
                }
                Msg::Definition(key, r) => {
                    if let Some((l, d)) = self.open.as_mut()
                        && l.key == key
                    {
                        *d = Some(r);
                    }
                }
            }
        }
    }

    /// Opens a light: fetches its file.
    pub fn look_at(&mut self, light: &Light, ctx: &eframe::egui::Context) {
        self.open = Some((light.clone(), None));
        let (tx, ctx, key) = (self.tx.clone(), ctx.clone(), light.key.clone());
        let _ = std::thread::Builder::new().name("fixture file".into()).spawn(move || {
            let url = format!("https://raw.githubusercontent.com/{REPO}/master/fixtures/{key}.json");
            let _ = tx.send(Msg::Definition(key, crate::online::get_bytes(&url).and_then(|b| definition(&b))));
            ctx.request_repaint();
        });
    }

    /// Asks for the list again (after a failure).
    pub fn retry(&mut self) {
        self.asked = false;
        self.lights = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPOT: &str = r#"{"name":"Intimidator Spot 110","categories":["Moving Head","Color Changer"],
        "availableChannels":{
            "Pan":{"fineChannelAliases":["Pan fine"],"capability":{"type":"Pan"}},
            "Tilt":{"fineChannelAliases":["Tilt fine"],"capability":{"type":"Tilt"}},
            "Color Wheel":{"capabilities":[{"dmxRange":[0,31],"type":"WheelSlot"}]},
            "Strobe":{"capabilities":[{"dmxRange":[0,15],"type":"NoFunction"},{"dmxRange":[16,131],"type":"ShutterStrobe","shutterEffect":"Strobe"},{"dmxRange":[132,255],"type":"ShutterStrobe","shutterEffect":"Open"}]},
            "Dimmer":{"capability":{"type":"Intensity"}},
            "Red":{"capability":{"type":"ColorIntensity","color":"Red"}},
            "Macro":{"defaultValue":12,"capability":{"type":"Effect"}}},
        "modes":[{"name":"8-channel","channels":["Pan","Pan fine","Tilt","Tilt fine","Color Wheel","Strobe","Dimmer",null]},{"name":"3-channel","channels":["Red","Dimmer","Macro"]}]}"#;

    #[test]
    fn a_library_light_becomes_a_fixture_with_its_channels() {
        let d = definition(SPOT.as_bytes()).unwrap();
        assert_eq!(d.modes.len(), 2);
        assert_eq!(d.mode_summary(0), "8 ch: Pan, Pan fine, Tilt, Tilt fine, Color Wheel, Strobe, Dimmer, –");
        let f = d.fixture("Chauvet DJ", 0);
        assert_eq!(f.name, "Chauvet DJ Intimidator Spot 110");
        assert_eq!(f.roles, vec![Role::Pan, Role::PanFine, Role::Tilt, Role::TiltFine, Role::Extra(0), Role::Extra(1), Role::Dimmer, Role::Zero]);
        assert_eq!(f.extra.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Color Wheel", "Strobe"]);
        assert_eq!((f.extra[1].value * 255.0).round() as u8, 132, "the shutter rests open");
        assert_eq!(f.span(), "U0 ch 1 – U0 ch 8");
        let small = d.fixture("", 1);
        assert_eq!(small.roles, vec![Role::Red, Role::Dimmer, Role::Extra(0)]);
        assert_eq!((small.extra[0].value * 255.0).round() as u8, 12, "its default value");
    }

    #[test]
    fn the_library_list_is_read_and_searched() {
        let tree = br#"{"tree":[{"path":"fixtures/chauvet-dj/intimidator-spot-110.json"},{"path":"fixtures/manufacturers.json"},{"path":"fixtures/adj/mega-par.json"},{"path":"docs/x.md"}]}"#;
        let all = index(tree, Some(br#"{"adj":{"name":"ADJ"}}"#)).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!((all[0].maker.as_str(), all[0].name.as_str()), ("ADJ", "Mega Par"), "the maker's real name");
        assert_eq!(all[1].maker, "Chauvet Dj", "else from the folder");
        assert_eq!(search(&all, "spot chauvet").iter().map(|l| l.key.as_str()).collect::<Vec<_>>(), ["chauvet-dj/intimidator-spot-110"]);
    }

    /// The real library: `PHOSPHOR_ONLINE_TEST=1 cargo test ofl_online -- --nocapture`.
    #[test]
    fn ofl_online() {
        if std::env::var_os("PHOSPHOR_ONLINE_TEST").is_none() {
            return;
        }
        let makers = crate::online::get_cached(&format!("https://raw.githubusercontent.com/{REPO}/master/fixtures/manufacturers.json")).ok();
        let all = index(&crate::online::get_cached(&format!("https://api.github.com/repos/{REPO}/git/trees/master?recursive=1")).unwrap(), makers.as_deref()).unwrap();
        let spot = search(&all, "intimidator spot 110")[0].clone();
        let d = definition(&crate::online::get_bytes(&format!("https://raw.githubusercontent.com/{REPO}/master/fixtures/{}.json", spot.key)).unwrap()).unwrap();
        let f = d.fixture(&spot.maker, 0);
        assert_eq!(spot.maker, "Chauvet DJ");
        println!("OFL {} lights; {} mode {}: {:?} extras {:?}", all.len(), d.name, d.modes[0].0, f.roles, f.extra.iter().map(|e| (&e.name, (e.value * 255.0).round())).collect::<Vec<_>>());
        assert!(f.roles.contains(&Role::Pan) && f.roles.contains(&Role::Dimmer));
    }
}
