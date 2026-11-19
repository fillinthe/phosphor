//! Data as numbers (Designer phase 5): a shared modulator can take its value from a live reading (the weather, the
//! ISS, earthquakes), a web address answering JSON, a timer, the clock, or a table file; and it can fire an event
//! (set any control) when its value crosses a threshold. The app fills the values each frame (`phosphor::data`);
//! here is what a show saves.

use serde::{Deserialize, Serialize};

use super::{ControlTarget, SendTo};

/// Where a Data modulator's value comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DataKind {
    /// A live reading by name (`live_data::{WEATHER_READINGS, ISS_READINGS, QUAKE_READINGS}`), already 0..1.
    Reading(String),
    /// A web address answering JSON; `path` picks the number ("main.temp", "list.0.value"), asked every `every` s.
    Json { url: String, path: String, every: f32 },
    /// 0 to 1 over `seconds` from when it was started (Restart sets it going again). `down`: a countdown (1 to 0;
    /// its number is the seconds left, for `{mm:ss}` in a Text).
    Timer {
        seconds: f32,
        #[serde(default)]
        down: bool,
    },
    /// The time of day, 0 at midnight to 1 the next midnight.
    Clock,
    /// A table file (CSV / tab-separated): `column` of one row after another, each for `seconds`.
    Table { path: String, column: usize, seconds: f32 },
    /// A list from the web: JSON (`path` to the list, `*` for each item: "articles.*.title") or a news feed (RSS /
    /// Atom titles), fetched again every `every` s; one item after another, each for `seconds`. Its words are the
    /// item showing (`{text}` in a Text), its number the item's place (1, 2, 3…).
    List { url: String, path: String, every: f32, seconds: f32 },
}

impl DataKind {
    /// One of each, to start from.
    pub const ALL: [DataKind; 6] = [
        DataKind::Reading(String::new()),
        DataKind::Json { url: String::new(), path: String::new(), every: 60.0 },
        DataKind::Timer { seconds: 10.0, down: false },
        DataKind::Clock,
        DataKind::Table { path: String::new(), column: 0, seconds: 1.0 },
        DataKind::List { url: String::new(), path: String::new(), every: 300.0, seconds: 5.0 },
    ];

    pub fn name(&self) -> &'static str {
        match self {
            DataKind::Reading(_) => "Live reading",
            DataKind::Json { .. } => "Web (JSON)",
            DataKind::Timer { .. } => "Timer",
            DataKind::Clock => "Clock",
            DataKind::Table { .. } => "Table file",
            DataKind::List { .. } => "Web list",
        }
    }

    pub fn about(&self) -> &'static str {
        match self {
            DataKind::Reading(_) => "The weather where you are, where the ISS is, or earthquakes (LFOs & Modulators › Live data picks the place)",
            DataKind::Json { .. } => "A web address answering JSON: a number out of it, asked again every so often",
            DataKind::Timer { .. } => "0 to 1 over so many seconds from when it's started",
            DataKind::Clock => "The time of day: 0 at midnight, 1 at the next",
            DataKind::Table { .. } => "A CSV or tab-separated file: one row after another, a column's number",
            DataKind::List { .. } => "A list from a web address (JSON, or a news feed: RSS / Atom), one item after another: wire it into a Text",
        }
    }

    /// Same kind (whatever its settings).
    pub fn same(&self, other: &DataKind) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// A modulator's data source and how its raw number maps to 0..1 (`min` → 0, `max` → 1; readings are 0..1 already).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DataFeed {
    pub kind: DataKind,
    pub min: f32,
    pub max: f32,
}

impl Default for DataFeed {
    fn default() -> Self {
        DataFeed { kind: DataKind::Clock, min: 0.0, max: 1.0 }
    }
}

impl DataFeed {
    /// A raw number as the modulator's value.
    pub fn unit(&self, raw: f64) -> f32 {
        let (lo, hi) = (self.min as f64, self.max as f64);
        if (hi - lo).abs() < 1e-9 || !raw.is_finite() {
            return 0.0;
        }
        ((raw - lo) / (hi - lo)).clamp(0.0, 1.0) as f32
    }
}

/// What a Data modulator last read (not saved): the raw number and the words for it ("21 °C", a table cell).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataLive {
    pub raw: f64,
    pub shown: String,
    pub problem: Option<String>,
    /// More to say beside the words ("3 of 20", a refresh that failed while the last list is kept).
    pub note: String,
}

/// An event: when the modulator's value crosses `threshold` upward, `target` is set to `value` and / or `value` is
/// sent out (`send`: OSC, MIDI, an Arduino pin).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Trigger {
    pub threshold: f32,
    pub target: Option<ControlTarget>,
    pub value: f32,
    /// Fire on the way down too (back below the threshold), with `value` again.
    pub both_ways: bool,
    pub send: Option<SendTo>,
}

impl Default for Trigger {
    fn default() -> Self {
        Trigger { threshold: 0.5, target: None, value: 1.0, both_ways: false, send: None }
    }
}

/// The JSON at a dotted path ("main.temp", "list.0.value"; an empty path = all of it).
pub fn json_at<'a>(v: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cur = v;
    for key in path.split('.').map(str::trim).filter(|k| !k.is_empty()) {
        cur = match cur {
            serde_json::Value::Array(a) => a.get(key.parse::<usize>().ok()?)?,
            serde_json::Value::Object(o) => o.get(key)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// The most items a Web list keeps.
pub const LIST_MAX: usize = 500;

/// The words of a list in JSON: `path` leads to the list ("articles", "data.songs"); `*` stands for each item
/// ("articles.*.title"). An item in words is itself; an object's words are its title / name / text field.
pub fn json_list(v: &serde_json::Value, path: &str) -> Vec<String> {
    fn words(v: &serde_json::Value) -> Option<String> {
        let w = match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Object(o) => ["title", "name", "text", "headline", "label", "value"].iter().find_map(|k| o.get(*k).and_then(words))?,
            _ => return None,
        };
        let w = w.split_whitespace().collect::<Vec<_>>().join(" ");
        (!w.is_empty()).then_some(w)
    }
    fn walk(v: &serde_json::Value, keys: &[&str], out: &mut Vec<String>) {
        if out.len() >= LIST_MAX {
            return;
        }
        let Some((key, rest)) = keys.split_first() else {
            match v {
                serde_json::Value::Array(a) => {
                    let room = LIST_MAX - out.len();
                    out.extend(a.iter().filter_map(words).take(room));
                }
                v => out.extend(words(v)),
            }
            return;
        };
        if *key == "*" {
            let items: Vec<&serde_json::Value> = match v {
                serde_json::Value::Array(a) => a.iter().collect(),
                serde_json::Value::Object(o) => o.values().collect(),
                _ => Vec::new(),
            };
            for item in items {
                walk(item, rest, out);
            }
        } else if let Some(next) = json_at(v, key) {
            walk(next, rest, out);
        }
    }
    let keys: Vec<&str> = path.split('.').map(str::trim).filter(|k| !k.is_empty()).collect();
    let mut out = Vec::new();
    walk(v, &keys, &mut out);
    out
}

/// Which item of a list of `len` shows `elapsed` seconds after it started, each for `seconds` (round and round).
pub fn list_step(elapsed: f64, seconds: f32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (elapsed.max(0.0) / seconds.max(0.5) as f64) as usize % len
}

/// The number at a dotted path in JSON ("main.temp", "list.0.value"; a number in a string counts; true = 1).
pub fn json_path(v: &serde_json::Value, path: &str) -> Option<f64> {
    match json_at(v, path)? {
        serde_json::Value::Number(n) => n.as_f64(),
        // ("NaN" and "inf" parse as numbers: they aren't readings.)
        serde_json::Value::String(s) => s.trim().parse().ok().filter(|n: &f64| n.is_finite()),
        serde_json::Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// A table file's rows and cells (commas or tabs; quotes dropped; blank lines skipped).
pub fn table_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let sep = if l.contains('\t') { '\t' } else { ',' };
            l.split(sep).map(|c| c.trim().trim_matches('"').to_string()).collect()
        })
        .collect()
}

/// Seconds as minutes and seconds ("04:05"; minutes past 59 go on: "75:00") or hours too ("1:02:05").
pub fn clock_words(seconds: f64, hours: bool) -> String {
    let s = if seconds.is_finite() { seconds.max(0.0) as u64 } else { 0 };
    if hours { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

/// A number in words, with `decimals` places (never "-0").
fn number_words(n: f64, decimals: usize) -> String {
    if !n.is_finite() {
        return format!("{:.decimals$}", 0.0);
    }
    let s = format!("{n:.decimals$}");
    if s.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') { format!("{:.decimals$}", 0.0) } else { s }
}

/// What a Text box's template may hold (its tooltip).
pub const TEMPLATE_HELP: &str = "{n} = the number (whole), {n.1} / {n.2} = with decimals, {mm:ss} / {h:mm:ss} = the number as a time (a countdown's seconds left), {min} = whole minutes (rounded up), {text} = the words (a list item, 21 °C, a table cell), {percent} = 0 to 100, {value} = 0 to 1";

/// The words a Text box shows for a value (`TEMPLATE_HELP`): the number is the reading's own (`live.raw`: a score,
/// a countdown's seconds left), `value` the box's 0..1. `{raw}` is the older name of `{text}`.
pub fn fill_template(template: &str, value: f32, live: &DataLive) -> String {
    if !template.contains('{') {
        return template.to_string();
    }
    let text = if live.shown.is_empty() { number_words(live.raw, 1) } else { live.shown.clone() };
    let n = live.raw;
    let minutes = if n.is_finite() { (n.max(0.0) / 60.0).ceil() as u64 } else { 0 };
    template
        .replace("{value}", &format!("{value:.2}"))
        .replace("{percent}", &format!("{:.0}", value * 100.0))
        .replace("{raw}", &text)
        .replace("{text}", &text)
        .replace("{n}", &number_words(n, 0))
        .replace("{n.1}", &number_words(n, 1))
        .replace("{n.2}", &number_words(n, 2))
        .replace("{h:mm:ss}", &clock_words(n, true))
        .replace("{mm:ss}", &clock_words(n, false))
        .replace("{min}", &minutes.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_paths_tables_and_templates() {
        let v: serde_json::Value = serde_json::from_str(r#"{"main":{"temp":21.5},"list":[{"x":"3"},{"x":true}],"name":"x"}"#).unwrap();
        assert_eq!(json_path(&v, "main.temp"), Some(21.5));
        assert_eq!(json_path(&v, "list.0.x"), Some(3.0), "a number in a string");
        assert_eq!(json_path(&v, "list.1.x"), Some(1.0));
        assert_eq!(json_path(&v, "name"), None);
        assert_eq!(json_path(&v, "nope.x"), None);
        let rows = table_rows("a,b\n\n\"1\", 2\n3\t4\n");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["1", "2"], vec!["3", "4"]]);
        let feed = DataFeed { kind: DataKind::Json { url: "u".into(), path: "p".into(), every: 5.0 }, min: -10.0, max: 30.0 };
        assert_eq!(feed.unit(10.0), 0.5);
        assert_eq!(feed.unit(99.0), 1.0);
        let live = DataLive { raw: 21.5, shown: "21 °C".into(), ..Default::default() };
        assert_eq!(fill_template("It's {raw}, {percent}% ({value})", 0.75, &live), "It's 21 °C, 75% (0.75)");
        let json = serde_json::to_string(&feed).unwrap();
        assert_eq!(serde_json::from_str::<DataFeed>(&json).unwrap(), feed);
        let e = Trigger { target: Some(ControlTarget::LayerOpacity(1)), ..Default::default() };
        let back: Trigger = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn templates_show_one_number_many_ways() {
        // A countdown with 4 min 5 s left (its number is the seconds left).
        let left = DataLive { raw: 245.0, shown: "04:05".into(), ..Default::default() };
        assert_eq!(fill_template("{min} minutes left", 0.4, &left), "5 minutes left", "rounded up");
        assert_eq!(fill_template("{mm:ss}", 0.4, &left), "04:05");
        assert_eq!(fill_template("{h:mm:ss}", 0.4, &left), "0:04:05");
        assert_eq!(fill_template("T-{n} s", 0.4, &left), "T-245 s");
        let long = DataLive { raw: 3725.9, ..Default::default() };
        assert_eq!(fill_template("{mm:ss} | {h:mm:ss}", 0.0, &long), "62:05 | 1:02:05", "minutes go on past an hour");
        // A score.
        let score = DataLive { raw: 12.0, ..Default::default() };
        assert_eq!(fill_template("Score: {n}", 0.12, &score), "Score: 12");
        assert_eq!(fill_template("{n.1} / {n.2}", 0.0, &DataLive { raw: 3.14159, ..Default::default() }), "3.1 / 3.14");
        assert_eq!(fill_template("{n}", 0.0, &DataLive { raw: -0.2, ..Default::default() }), "0", "no minus zero");
        assert_eq!(fill_template("{mm:ss}", 0.0, &DataLive { raw: -3.0, ..Default::default() }), "00:00", "never below zero");
        assert_eq!(fill_template("{text} / {raw}", 0.0, &DataLive { raw: 2.0, shown: "Song two".into(), ..Default::default() }), "Song two / Song two");
        assert_eq!(fill_template("Plain words", 0.5, &score), "Plain words");
    }

    #[test]
    fn lists_from_json_and_their_steps() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"articles":[{"title":"One  story"},{"name":"Two"},{"x":1},{"title":"Three"}],"songs":["A","B",3,null],"data":{"top":[{"song":{"title":"Deep"}}]}}"#,
        )
        .unwrap();
        assert_eq!(json_list(&v, "articles"), ["One story", "Two", "Three"], "an object's title or name; others skipped");
        assert_eq!(json_list(&v, "articles.*.title"), ["One story", "Three"]);
        assert_eq!(json_list(&v, "songs"), ["A", "B", "3"]);
        assert_eq!(json_list(&v, "data.top.*.song"), ["Deep"]);
        assert_eq!(json_list(&v, "data.top.0.song.title"), ["Deep"], "one item is a list of one");
        assert!(json_list(&v, "nope").is_empty());
        let root: serde_json::Value = serde_json::from_str(r#"["x","y"]"#).unwrap();
        assert_eq!(json_list(&root, ""), ["x", "y"]);
        let many = serde_json::Value::Array((0..900).map(serde_json::Value::from).collect());
        assert_eq!(json_list(&many, "").len(), LIST_MAX);
        assert_eq!(list_step(0.0, 5.0, 3), 0);
        assert_eq!(list_step(4.9, 5.0, 3), 0);
        assert_eq!(list_step(5.0, 5.0, 3), 1);
        assert_eq!(list_step(16.0, 5.0, 3), 0, "round again");
        assert_eq!(list_step(3.0, 5.0, 0), 0);
        // Older saves (a Timer without `down`) still open; a list round-trips.
        let old: DataKind = serde_json::from_str(r#"{"Timer":{"seconds":8.0}}"#).unwrap();
        assert_eq!(old, DataKind::Timer { seconds: 8.0, down: false });
        let list = DataKind::List { url: "https://x".into(), path: "a.*.b".into(), every: 300.0, seconds: 5.0 };
        assert_eq!(serde_json::from_str::<DataKind>(&serde_json::to_string(&list).unwrap()).unwrap(), list);
    }
}
