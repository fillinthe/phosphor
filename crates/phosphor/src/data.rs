//! Data boxes (Designer phase 5): fills every modulator that has a `DataFeed` each frame (a live reading from
//! `live_data::Feeds`, a web address answering JSON fetched on a thread when due, a timer, the clock, a table file
//! read once) and fires events (a modulator's value crossing its threshold sets a control). Costs nothing while no
//! modulator has data or an event.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use phosphor_engine::{DataKind, ISS_READINGS, QUAKE_READINGS, WEATHER_READINGS};

use phosphor_engine::Composition;

use crate::app::State;

/// A web address's latest answer.
#[derive(Default)]
struct Web {
    busy: bool,
    asked: f64,
    value: Option<Result<serde_json::Value, String>>,
}

/// What the data boxes keep between frames.
pub struct DataRun {
    tx: Sender<(String, Result<serde_json::Value, String>)>,
    rx: Receiver<(String, Result<serde_json::Value, String>)>,
    web: HashMap<String, Web>,
    /// When each timer (or table) modulator was started.
    started: HashMap<u64, f64>,
    tables: HashMap<String, Result<Vec<Vec<String>>, String>>,
    /// Whether each event modulator was last above its threshold.
    above: HashMap<u64, bool>,
}

impl Default for DataRun {
    fn default() -> Self {
        let (tx, rx) = channel();
        DataRun { tx, rx, web: HashMap::new(), started: HashMap::new(), tables: HashMap::new(), above: HashMap::new() }
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
        for (t, v) in run(&mut self.data, &mut self.comp, &self.feeds, &self.mod_inputs.shared, now) {
            self.set_control(&t, v, now);
        }
    }
}

/// One frame of the data boxes: the modulators' values and words, fetches started, and the events to fire
/// (target, value).
pub fn run(data: &mut DataRun, comp: &mut Composition, feeds: &crate::live_data::Feeds, shared: &[(u64, f32)], now: f64) -> Vec<(phosphor_engine::ControlTarget, f32)> {
    {
        if !comp.modulators.iter().any(|m| m.data.is_some() || m.event.is_some()) {
            return Vec::new();
        }
        for (url, r) in data.rx.try_iter() {
            let w = data.web.entry(url).or_default();
            w.busy = false;
            w.value = Some(r);
        }
        let (tod, _) = crate::clock::local_time();
        let mut fetch: Vec<String> = Vec::new();
        let live_ids: Vec<u64> = comp.modulators.iter().filter(|m| m.data.is_some()).map(|m| m.id).collect();
        data.started.retain(|id, _| live_ids.contains(id));
        for i in 0..comp.modulators.len() {
            let Some(feed) = comp.modulators[i].data.clone() else { continue };
            let id = comp.modulators[i].id;
            let mut value: Option<f32> = None;
            let mut raw = 0.0;
            let mut shown = String::new();
            let mut problem = None;
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
                        if !w.busy && (w.asked == 0.0 || now - w.asked >= (*every).max(2.0) as f64) {
                            w.busy = true;
                            w.asked = now;
                            fetch.push(url.clone());
                        }
                        match &w.value {
                            Some(Ok(v)) => match phosphor_engine::json_path(v, path) {
                                Some(x) => {
                                    raw = x;
                                    shown = format!("{x}");
                                    value = Some(feed.unit(x));
                                }
                                None => problem = Some(format!("No number at {path:?} in the answer")),
                            },
                            Some(Err(e)) => problem = Some(e.clone()),
                            None => {}
                        }
                    }
                }
                DataKind::Timer { seconds } => {
                    let start = *data.started.entry(id).or_insert(now);
                    let t = ((now - start) / (*seconds).max(0.01) as f64).clamp(0.0, 1.0);
                    raw = now - start;
                    shown = format!("{:.1} s", now - start);
                    value = Some(t as f32);
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
                        let rows = data.tables.entry(path.clone()).or_insert_with(|| std::fs::read_to_string(path).map(|t| phosphor_engine::table_rows(&t)).map_err(|e| e.to_string()));
                        match rows {
                            Ok(rows) if !rows.is_empty() => {
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
                            Ok(_) => problem = Some("The table is empty".to_string()),
                            Err(e) => problem = Some(format!("Couldn't read the table: {e}")),
                        }
                    }
                }
            }
            let m = &mut comp.modulators[i];
            if let Some(v) = value {
                m.value = v;
            }
            m.live = phosphor_engine::DataLive { raw, shown, problem };
        }
        for url in fetch {
            let tx = data.tx.clone();
            let _ = std::thread::Builder::new().name("data box".into()).spawn(move || {
                let r = crate::online::get_bytes(&url).and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).map_err(|_| "The answer isn't JSON".to_string()));
                let _ = tx.send((url, r));
            });
        }
        // Events: a value crossing its threshold.
        let mut fire: Vec<(phosphor_engine::ControlTarget, f32)> = Vec::new();
        for m in &comp.modulators {
            let Some(e) = &m.event else { continue };
            let value = shared.iter().find(|(id, _)| *id == m.id).map_or(0.0, |x| x.1);
            let above = value >= e.threshold;
            let was = data.above.insert(m.id, above);
            if let (Some(was), Some(t)) = (was, &e.target)
                && was != above
                && (above || e.both_ways)
            {
                fire.push((t.clone(), e.value));
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
        let timer = Modulator { id: 1, data: Some(DataFeed { kind: DataKind::Timer { seconds: 10.0 }, ..Default::default() }), ..Default::default() };
        let clock = Modulator { id: 2, data: Some(DataFeed { kind: DataKind::Clock, ..Default::default() }), ..Default::default() };
        let table = Modulator { id: 3, data: Some(DataFeed { kind: DataKind::Table { path: path.to_string_lossy().to_string(), column: 1, seconds: 1.0 }, min: 0.0, max: 100.0 }), ..Default::default() };
        let event = Modulator { id: 4, event: Some(Trigger { threshold: 0.5, target: Some(phosphor_engine::ControlTarget::MasterFade), value: 0.25, ..Default::default() }), ..Default::default() };
        comp.modulators = vec![timer, clock, table, event];
        let step = |data: &mut DataRun, comp: &mut Composition, shared: &[(u64, f32)], now: f64| run(data, comp, &feeds, shared, now);
        step(&mut data, &mut comp, &shared, 100.0);
        let m = |comp: &Composition, id: u64| comp.modulators.iter().find(|m| m.id == id).unwrap().clone();
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
        assert_eq!(step(&mut data, &mut comp, &shared, 109.0), vec![(phosphor_engine::ControlTarget::MasterFade, 0.25)], "fired");
        assert!(step(&mut data, &mut comp, &shared, 110.0).is_empty(), "not again while above");
        let _ = std::fs::remove_file(path);
    }
}
