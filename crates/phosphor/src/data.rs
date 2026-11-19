//! Data boxes (Designer phase 5): fills every modulator that has a `DataFeed` each frame (a live reading from
//! `live_data::Feeds`, a web address answering JSON fetched on a thread when due, a timer, the clock, a table file
//! read once, on a thread too) and fires events (a modulator's value crossing its threshold sets a control). Costs nothing while no
//! modulator has data or an event.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use phosphor_engine::{DataKind, ISS_READINGS, QUAKE_READINGS, WEATHER_READINGS};

use phosphor_engine::Composition;

use crate::app::State;

/// What a web address answered: JSON, or a news feed's titles (RSS / Atom).
#[derive(Debug, PartialEq)]
enum Answer {
    Json(serde_json::Value),
    Feed(Vec<String>),
}

/// A download as an answer: JSON, else a news feed (on the fetching thread).
fn read_answer(bytes: &[u8]) -> Result<Answer, String> {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) {
        return Ok(Answer::Json(v));
    }
    crate::live_data::read_headlines(&String::from_utf8_lossy(bytes)).map(Answer::Feed).map_err(|_| "The answer isn't JSON or a news feed (RSS / Atom)".to_string())
}

/// A web address's latest answer. A failed ask keeps the last good answer (`error` says what went wrong).
#[derive(Default)]
struct Web {
    busy: bool,
    asked: f64,
    value: Option<Answer>,
    error: Option<String>,
    /// Answers so far (a list is worked out again when it grows).
    got: u64,
}

impl Web {
    /// Whether to ask again: every `every` s, sooner (30 s) while nothing has come back but errors.
    fn due(&self, every: f64, now: f64) -> bool {
        let every = if self.value.is_none() && self.error.is_some() { every.min(30.0) } else { every };
        !self.busy && (self.asked == 0.0 || now - self.asked >= every)
    }
}

type Fetched = (String, Result<Answer, String>);

/// What the data boxes keep between frames.
pub struct DataRun {
    tx: Sender<Fetched>,
    rx: Receiver<Fetched>,
    web: HashMap<String, Web>,
    /// Each Web list box's items: (the answer they came from, the path, the items), worked out once per answer.
    lists: HashMap<u64, (u64, String, Vec<String>)>,
    /// When each timer (or table) modulator was started.
    started: HashMap<u64, f64>,
    /// Each table file: the read it waits for (a number, so a read from before `forget_table` is dropped) and its rows
    /// (None while the file is read, off the UI thread: a slow drive must never hold up the show).
    tables: HashMap<String, (u64, Option<Result<Vec<Vec<String>>, String>>)>,
    reads: u64,
    table_tx: Sender<(String, u64, Result<Vec<Vec<String>>, String>)>,
    table_rx: Receiver<(String, u64, Result<Vec<Vec<String>>, String>)>,
    /// Whether each event modulator was last above its threshold.
    above: HashMap<u64, bool>,
}

impl Default for DataRun {
    fn default() -> Self {
        let (tx, rx) = channel();
        let (table_tx, table_rx) = channel();
        DataRun { tx, rx, web: HashMap::new(), lists: HashMap::new(), started: HashMap::new(), tables: HashMap::new(), reads: 0, table_tx, table_rx, above: HashMap::new() }
    }
}

impl DataRun {
    /// Starts modulator `id`'s timer (or table) from 0 again.
    pub fn restart(&mut self, id: u64) {
        self.started.remove(&id);
    }

    /// Forgets a table file (read again after it changed).
    pub fn forget_table(&mut self, path: &str) {
        self.tables.remove(path);
    }
}

impl State {
    /// Fills the data modulators and fires events. Once per frame, after the live feeds.
    pub(crate) fn run_data(&mut self, now: f64) {
        for (t, to, v) in run(&mut self.data, &mut self.comp, &self.feeds, &self.mod_inputs.shared, now) {
            if let Some(t) = t {
                self.set_control(&t, v, now);
            }
            if let Some(to) = to {
                self.control.send_to(&self.comp, &to, v);
            }
        }
    }
}

/// One frame of the data boxes: the modulators' values and words, fetches started, and the events to fire
/// (control to set, message to send, value).
pub fn run(data: &mut DataRun, comp: &mut Composition, feeds: &crate::live_data::Feeds, shared: &[(u64, f32)], now: f64) -> Vec<(Option<phosphor_engine::ControlTarget>, Option<phosphor_engine::SendTo>, f32)> {
    {
        if !comp.modulators.iter().any(|m| m.data.is_some() || m.event.is_some()) {
            return Vec::new();
        }
        for (url, r) in data.rx.try_iter() {
            let w = data.web.entry(url).or_default();
            w.busy = false;
            match r {
                Ok(a) => {
                    (w.value, w.error) = (Some(a), None);
                    w.got += 1;
                }
                Err(e) => w.error = Some(e),
            }
        }
        for (path, read, r) in data.table_rx.try_iter() {
            if let Some(t) = data.tables.get_mut(&path).filter(|t| t.0 == read) {
                t.1 = Some(r);
            }
        }
        let (tod, _) = crate::clock::local_time();
        let mut fetch: Vec<String> = Vec::new();
        let live_ids: Vec<u64> = comp.modulators.iter().filter(|m| m.data.is_some()).map(|m| m.id).collect();
        data.started.retain(|id, _| live_ids.contains(id));
        data.lists.retain(|id, _| live_ids.contains(id));
        for i in 0..comp.modulators.len() {
            let Some(feed) = comp.modulators[i].data.clone() else { continue };
            let id = comp.modulators[i].id;
            let mut value: Option<f32> = None;
            let mut raw = 0.0;
            let mut shown = String::new();
            let mut problem = None;
            let mut note = String::new();
            match &feed.kind {
                DataKind::Reading(name) => {
                    // The feed it belongs to is turned on (the place for the weather is picked in Live data).
                    let data = &mut comp.live_data;
                    if WEATHER_READINGS.contains(&name.as_str()) {
                        data.weather = true;
                        if data.place.is_empty() {
                            problem = Some("Pick the place in LFOs & Modulators › Live data".to_string());
                        }
                    } else if ISS_READINGS.contains(&name.as_str()) {
                        data.iss = true;
                    } else if QUAKE_READINGS.contains(&name.as_str()) {
                        data.quakes = true;
                    }
                    if let Some(r) = feeds.reading(name) {
                        value = Some(r.value);
                        raw = r.value as f64;
                        shown = r.shown.clone();
                    }
                }
                DataKind::Json { url, path, every } => {
                    if url.trim().is_empty() {
                        problem = Some("Give it a web address".to_string());
                    } else {
                        let w = data.web.entry(url.clone()).or_default();
                        if w.due((*every).max(2.0) as f64, now) {
                            w.busy = true;
                            w.asked = now;
                            fetch.push(url.clone());
                        }
                        match &w.value {
                            Some(Answer::Json(v)) => match phosphor_engine::json_path(v, path) {
                                Some(x) => {
                                    raw = x;
                                    shown = format!("{x}");
                                    value = Some(feed.unit(x));
                                }
                                None => problem = Some(format!("No number at {path:?} in the answer")),
                            },
                            Some(Answer::Feed(_)) => problem = Some("That's a news feed: a Web list box shows its headlines".to_string()),
                            None => problem = w.error.clone(),
                        }
                        if let (Some(_), Some(e)) = (&w.value, &w.error) {
                            note = format!("couldn't ask again: {e}");
                        }
                    }
                }
                DataKind::List { url, path, every, seconds } => {
                    if url.trim().is_empty() {
                        problem = Some("Give it a web address (JSON or a news feed)".to_string());
                    } else {
                        let w = data.web.entry(url.clone()).or_default();
                        if w.due((*every).max(30.0) as f64, now) {
                            w.busy = true;
                            w.asked = now;
                            fetch.push(url.clone());
                        }
                        // The items, worked out again only when a new answer came or the path changed.
                        let fresh = data.lists.get(&id).is_none_or(|(got, p, _)| *got != w.got || p != path);
                        if fresh && w.value.is_some() {
                            let items = match &w.value {
                                Some(Answer::Json(v)) => phosphor_engine::json_list(v, path),
                                Some(Answer::Feed(t)) => t.iter().take(phosphor_engine::LIST_MAX).cloned().collect(),
                                None => Vec::new(),
                            };
                            data.lists.insert(id, (w.got, path.clone(), items));
                        }
                        match data.lists.get(&id).map(|l| &l.2) {
                            Some(items) if !items.is_empty() => {
                                let start = *data.started.entry(id).or_insert(now);
                                let i = phosphor_engine::list_step(now - start, *seconds, items.len());
                                shown = items[i].clone();
                                raw = (i + 1) as f64;
                                value = Some(if items.len() > 1 { i as f32 / (items.len() - 1) as f32 } else { 0.0 });
                                note = format!("{} of {}", i + 1, items.len());
                                if let Some(e) = &w.error {
                                    note = format!("{note} · couldn't ask again: {e}");
                                }
                            }
                            Some(_) if matches!(w.value, Some(Answer::Json(_))) => problem = Some(if path.trim().is_empty() { "Give the path to the list: articles, or articles.*.title".to_string() } else { format!("No list at {path:?} in the answer") }),
                            Some(_) => problem = Some("The list is empty".to_string()),
                            None => problem = w.error.clone(),
                        }
                    }
                }
                DataKind::Timer { seconds, down } => {
                    let start = *data.started.entry(id).or_insert(now);
                    let t = ((now - start) / (*seconds).max(0.01) as f64).clamp(0.0, 1.0);
                    if *down {
                        // The seconds left, counted the way a countdown shows them (4.2 s left reads 00:05).
                        raw = (*seconds as f64 - (now - start)).max(0.0).ceil();
                        shown = phosphor_engine::clock_words(raw, raw >= 3600.0);
                        value = Some(1.0 - t as f32);
                    } else {
                        raw = now - start;
                        shown = format!("{:.1} s", now - start);
                        value = Some(t as f32);
                    }
                }
                DataKind::Clock => {
                    raw = tod as f64;
                    shown = format!("{:02}:{:02}:{:02}", tod / 3600, tod / 60 % 60, tod % 60);
                    value = Some(tod as f32 / 86400.0);
                }
                DataKind::Table { path, column, seconds } => {
                    if path.trim().is_empty() {
                        problem = Some("Pick a table file".to_string());
                    } else {
                        if !data.tables.contains_key(path) {
                            data.reads += 1;
                            data.tables.insert(path.clone(), (data.reads, None));
                            let (tx, file, read) = (data.table_tx.clone(), path.clone(), data.reads);
                            let spawned = std::thread::Builder::new().name("data table".into()).spawn(move || {
                                let r = std::fs::read_to_string(&file).map(|t| phosphor_engine::table_rows(&t)).map_err(|e| e.to_string());
                                let _ = tx.send((file, read, r));
                            });
                            // No thread (the system is out of them): say so, and try again next frame.
                            if spawned.is_err() {
                                data.tables.remove(path);
                            }
                        }
                        match data.tables.get(path).map(|t| &t.1) {
                            None => problem = Some("Couldn't start reading the table".to_string()),
                            Some(None) => shown = "reading…".to_string(),
                            Some(Some(Ok(rows))) if !rows.is_empty() => {
                                let start = *data.started.entry(id).or_insert(now);
                                let row = ((now - start) / (*seconds).max(0.05) as f64) as usize % rows.len();
                                let cell = rows[row].get(*column).cloned().unwrap_or_default();
                                shown = cell.clone();
                                match cell.trim().parse::<f64>() {
                                    Ok(x) => {
                                        raw = x;
                                        value = Some(feed.unit(x));
                                    }
                                    Err(_) => raw = row as f64,
                                }
                            }
                            Some(Some(Ok(_))) => problem = Some("The table is empty".to_string()),
                            Some(Some(Err(e))) => problem = Some(format!("Couldn't read the table: {e}")),
                        }
                    }
                }
            }
            let m = &mut comp.modulators[i];
            if let Some(v) = value {
                m.value = v;
            }
            m.live = phosphor_engine::DataLive { raw, shown, problem, note };
        }
        for url in fetch {
            let tx = data.tx.clone();
            let spawned = std::thread::Builder::new().name("data box".into()).spawn({
                let url = url.clone();
                move || {
                    let r = crate::online::get_bytes(&url).and_then(|b| read_answer(&b));
                    let _ = tx.send((url, r));
                }
            });
            // No thread: ask again next frame.
            if spawned.is_err()
                && let Some(w) = data.web.get_mut(&url)
            {
                w.busy = false;
                w.asked = 0.0;
            }
        }
        // Events: a value crossing its threshold.
        let mut fire: Vec<(Option<phosphor_engine::ControlTarget>, Option<phosphor_engine::SendTo>, f32)> = Vec::new();
        for m in &comp.modulators {
            let Some(e) = &m.event else { continue };
            let value = shared.iter().find(|(id, _)| *id == m.id).map_or(0.0, |x| x.1);
            let above = value >= e.threshold;
            let was = data.above.insert(m.id, above);
            if let Some(was) = was
                && was != above
                && (above || e.both_ways)
                && (e.target.is_some() || e.send.is_some())
            {
                fire.push((e.target.clone(), e.send.clone(), e.value));
            }
        }
        fire
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phosphor_engine::{DataFeed, Modulator, Trigger};

    /// A timer runs 0 to 1 and restarts; the clock is the time of day; a table steps through its rows; an event
    /// fires once when the value crosses its threshold.
    #[test]
    fn timers_tables_and_events_run_without_the_internet() {
        let (mut data, mut comp, feeds) = (DataRun::default(), Composition::default(), crate::live_data::Feeds::default());
        let mut shared: Vec<(u64, f32)> = vec![(4, 0.2)];
        let path = std::env::temp_dir().join("phosphor-data-box.csv");
        std::fs::write(&path, "name,level\na,10\nb,30\n").unwrap();
        let timer = Modulator { id: 1, data: Some(DataFeed { kind: DataKind::Timer { seconds: 10.0, down: false }, ..Default::default() }), ..Default::default() };
        let clock = Modulator { id: 2, data: Some(DataFeed { kind: DataKind::Clock, ..Default::default() }), ..Default::default() };
        let table = Modulator { id: 3, data: Some(DataFeed { kind: DataKind::Table { path: path.to_string_lossy().to_string(), column: 1, seconds: 1.0 }, min: 0.0, max: 100.0 }), ..Default::default() };
        let event = Modulator { id: 4, event: Some(Trigger { threshold: 0.5, target: Some(phosphor_engine::ControlTarget::MasterFade), value: 0.25, ..Default::default() }), ..Default::default() };
        comp.modulators = vec![timer, clock, table, event];
        let step = |data: &mut DataRun, comp: &mut Composition, shared: &[(u64, f32)], now: f64| run(data, comp, &feeds, shared, now);
        step(&mut data, &mut comp, &shared, 100.0);
        let m = |comp: &Composition, id: u64| comp.modulators.iter().find(|m| m.id == id).unwrap().clone();
        assert_eq!(m(&comp, 3).live.shown, "reading…", "the file is read off the UI thread");
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            step(&mut data, &mut comp, &shared, 100.0);
            if m(&comp, 3).live.shown != "reading…" {
                break;
            }
        }
        assert_eq!(m(&comp, 3).live.shown, "level", "the first row (the heading) first");
        step(&mut data, &mut comp, &shared, 105.0);
        assert!((m(&comp, 1).value - 0.5).abs() < 0.01, "half way: {}", m(&comp, 1).value);
        assert!(m(&comp, 2).value >= 0.0 && m(&comp, 2).value <= 1.0 && m(&comp, 2).live.shown.len() == 8);
        assert_eq!((m(&comp, 3).live.shown.as_str(), m(&comp, 3).value), ("30", 0.3), "row 5 of 3 = the third, at 1 s a row");
        step(&mut data, &mut comp, &shared, 106.5);
        assert_eq!((m(&comp, 3).live.shown.as_str(), m(&comp, 3).value), ("level", 0.3), "round again (not a number: the value stays)");
        data.restart(1);
        step(&mut data, &mut comp, &shared, 107.0);
        assert_eq!(m(&comp, 1).value, 0.0, "restarted");
        // The event: below, then above (fires once), then still above (quiet).
        assert!(step(&mut data, &mut comp, &shared, 108.0).is_empty());
        shared[0] = (4, 0.9);
        assert_eq!(step(&mut data, &mut comp, &shared, 109.0), vec![(Some(phosphor_engine::ControlTarget::MasterFade), None, 0.25)], "fired");
        assert!(step(&mut data, &mut comp, &shared, 110.0).is_empty(), "not again while above");
        // Sending only (no control): fires too, with the message.
        let osc = phosphor_engine::SendTo::Osc { address: "/show/hit".into() };
        comp.modulators[3].event = Some(Trigger { threshold: 0.5, target: None, value: 1.0, send: Some(osc.clone()), ..Default::default() });
        shared[0] = (4, 0.1);
        step(&mut data, &mut comp, &shared, 111.0);
        shared[0] = (4, 0.8);
        assert_eq!(step(&mut data, &mut comp, &shared, 112.0), vec![(None, Some(osc), 1.0)]);
        let _ = std::fs::remove_file(path);
    }

    /// A countdown's number is the seconds left; a Web list steps through a news feed's titles (or a JSON list),
    /// keeps the last list when asking again fails, and one number fills several templates.
    #[test]
    fn countdowns_and_web_lists_without_the_internet() {
        let (mut data, mut comp, feeds) = (DataRun::default(), Composition::default(), crate::live_data::Feeds::default());
        let url = "http://127.0.0.1:9/list".to_string();
        let down = Modulator { id: 1, data: Some(DataFeed { kind: DataKind::Timer { seconds: 300.0, down: true }, ..Default::default() }), ..Default::default() };
        let list = Modulator { id: 2, data: Some(DataFeed { kind: DataKind::List { url: url.clone(), path: String::new(), every: 300.0, seconds: 5.0 }, ..Default::default() }), ..Default::default() };
        comp.modulators = vec![down, list];
        // (Already asking: no download starts in a test.)
        data.web.insert(url.clone(), Web { busy: true, asked: 1.0, ..Default::default() });
        let m = |comp: &Composition, id: u64| comp.modulators.iter().find(|m| m.id == id).unwrap().live.clone();
        run(&mut data, &mut comp, &feeds, &[], 10.0);
        assert!(m(&comp, 2).shown.is_empty() && m(&comp, 2).problem.is_none(), "waiting quietly: {:?}", m(&comp, 2));
        let rss = br#"<?xml version="1.0"?><rss><channel><title>Feed</title><item><title>First song</title></item><item><title>Second song</title></item><item><title>Third song</title></item></channel></rss>"#;
        assert_eq!(read_answer(rss), Ok(Answer::Feed(vec!["First song".into(), "Second song".into(), "Third song".into()])));
        assert!(read_answer(b"<html>nope</html>").is_err());
        data.tx.send((url.clone(), read_answer(rss))).unwrap();
        run(&mut data, &mut comp, &feeds, &[], 10.0);
        assert_eq!((m(&comp, 2).shown.as_str(), m(&comp, 2).note.as_str()), ("First song", "1 of 3"));
        run(&mut data, &mut comp, &feeds, &[], 21.0);
        assert_eq!(m(&comp, 2).shown, "Third song", "5 s each");
        // Asking again fails: the list stays, the box says so.
        data.tx.send((url.clone(), Err("example.com didn't answer".into()))).unwrap();
        run(&mut data, &mut comp, &feeds, &[], 26.0);
        assert_eq!(m(&comp, 2).shown, "First song", "round again, from the kept list");
        assert!(m(&comp, 2).note.contains("didn't answer") && m(&comp, 2).problem.is_none());
        // A JSON answer with a path to its list.
        comp.modulators[1].data = Some(DataFeed { kind: DataKind::List { url: url.clone(), path: "songs.*.title".into(), every: 300.0, seconds: 5.0 }, ..Default::default() });
        data.tx.send((url.clone(), read_answer(br#"{"songs":[{"title":"Alpha"},{"title":"Beta"}]}"#))).unwrap();
        run(&mut data, &mut comp, &feeds, &[], 26.0);
        assert_eq!(m(&comp, 2).shown, "Beta");
        // The countdown: 300 s from 10 s; at 64.5 s, 245.5 s are left (shown as 246 s: 04:06).
        run(&mut data, &mut comp, &feeds, &[], 64.5);
        let left = m(&comp, 1);
        assert_eq!((left.raw, left.shown.as_str()), (246.0, "04:06"));
        let value = comp.modulators[0].value;
        assert!((value - (1.0 - 54.5 / 300.0)).abs() < 1e-3, "counts down 1 to 0: {value}");
        // One number, several texts: each its own template.
        let words: Vec<String> = ["{min} minutes left", "{mm:ss}", "Starts in {n} s"].iter().map(|t| phosphor_engine::fill_template(t, value, &left)).collect();
        assert_eq!(words, ["5 minutes left", "04:06", "Starts in 246 s"]);
        run(&mut data, &mut comp, &feeds, &[], 1000.0);
        assert_eq!((m(&comp, 1).shown.as_str(), comp.modulators[0].value), ("00:00", 0.0), "stays at zero");
    }
}
