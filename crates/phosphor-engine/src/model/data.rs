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
    /// 0 to 1 over `seconds` from when it was started (Restart sets it going again).
    Timer { seconds: f32 },
    /// The time of day, 0 at midnight to 1 the next midnight.
    Clock,
    /// A table file (CSV / tab-separated): `column` of one row after another, each for `seconds`.
    Table { path: String, column: usize, seconds: f32 },
}

impl DataKind {
    /// One of each, to start from.
    pub const ALL: [DataKind; 5] = [
        DataKind::Reading(String::new()),
        DataKind::Json { url: String::new(), path: String::new(), every: 60.0 },
        DataKind::Timer { seconds: 10.0 },
        DataKind::Clock,
        DataKind::Table { path: String::new(), column: 0, seconds: 1.0 },
    ];

    pub fn name(&self) -> &'static str {
        match self {
            DataKind::Reading(_) => "Live reading",
            DataKind::Json { .. } => "Web (JSON)",
            DataKind::Timer { .. } => "Timer",
            DataKind::Clock => "Clock",
            DataKind::Table { .. } => "Table file",
        }
    }

    pub fn about(&self) -> &'static str {
        match self {
            DataKind::Reading(_) => "The weather where you are, where the ISS is, or earthquakes (LFOs & Modulators › Live data picks the place)",
            DataKind::Json { .. } => "A web address answering JSON: a number out of it, asked again every so often",
            DataKind::Timer { .. } => "0 to 1 over so many seconds from when it's started",
            DataKind::Clock => "The time of day: 0 at midnight, 1 at the next",
            DataKind::Table { .. } => "A CSV or tab-separated file: one row after another, a column's number",
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
        if (hi - lo).abs() < 1e-9 {
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

/// The number at a dotted path in JSON ("main.temp", "list.0.value"; a number in a string counts; true = 1).
pub fn json_path(v: &serde_json::Value, path: &str) -> Option<f64> {
    let mut cur = v;
    for key in path.split('.').map(str::trim).filter(|k| !k.is_empty()) {
        cur = match cur {
            serde_json::Value::Array(a) => a.get(key.parse::<usize>().ok()?)?,
            serde_json::Value::Object(o) => o.get(key)?,
            _ => return None,
        };
    }
    match cur {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
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

/// The words a Text box shows for a value: `{value}` (0..1, two decimals), `{percent}` (0..100), `{raw}` (the
/// reading's own number or words: "21 °C", a table cell).
pub fn fill_template(template: &str, value: f32, live: &DataLive) -> String {
    let raw = if live.shown.is_empty() { format!("{:.1}", live.raw) } else { live.shown.clone() };
    template.replace("{value}", &format!("{value:.2}")).replace("{percent}", &format!("{:.0}", value * 100.0)).replace("{raw}", &raw)
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
        let live = DataLive { raw: 21.5, shown: "21 °C".into(), problem: None };
        assert_eq!(fill_template("It's {raw}, {percent}% ({value})", 0.75, &live), "It's 21 °C, 75% (0.75)");
        let json = serde_json::to_string(&feed).unwrap();
        assert_eq!(serde_json::from_str::<DataFeed>(&json).unwrap(), feed);
        let e = Trigger { target: Some(ControlTarget::LayerOpacity(1)), ..Default::default() };
        let back: Trigger = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }
}
