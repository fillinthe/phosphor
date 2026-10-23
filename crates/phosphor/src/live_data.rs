//! Live data feeds (Modulators › Live data): the weather somewhere (Open-Meteo), where the ISS is
//! (wheretheiss.at) and the world's earthquakes in the last hour (USGS), all free with no key. Each feed is fetched
//! on its own thread when it's due (and only while it's on); its readings, scaled 0..1, fill the shared modulators
//! named in `phosphor_engine::LiveData::readings`, so any control can follow the weather.

use std::sync::mpsc::{Receiver, Sender, channel};

use phosphor_engine::{Composition, ISS_READINGS, LiveData, QUAKE_READINGS, WEATHER_READINGS};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Feed {
    Weather,
    Iss,
    Quakes,
}

impl Feed {
    const ALL: [Feed; 3] = [Feed::Weather, Feed::Iss, Feed::Quakes];

    /// Seconds between fetches (about as often as each one changes).
    fn every(self) -> f64 {
        match self {
            Feed::Weather => 600.0,
            Feed::Iss => 5.0,
            Feed::Quakes => 60.0,
        }
    }

    fn on(self, d: &LiveData) -> bool {
        match self {
            Feed::Weather => d.weather && !d.place.is_empty(),
            Feed::Iss => d.iss,
            Feed::Quakes => d.quakes,
        }
    }

    fn url(self, d: &LiveData) -> String {
        match self {
            Feed::Weather => format!(
                "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}&current=temperature_2m,wind_speed_10m,cloud_cover,precipitation",
                d.latitude, d.longitude
            ),
            Feed::Iss => "https://api.wheretheiss.at/v1/satellites/25544".into(),
            Feed::Quakes => "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_hour.geojson".into(),
        }
    }
}

/// One reading: the modulator it fills, its value 0..1 and how it reads for people.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub name: &'static str,
    pub value: f32,
    pub shown: String,
}

/// `v` from `lo..hi` to 0..1.
fn unit(v: f64, lo: f64, hi: f64) -> f32 {
    ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32
}

/// A feed's answer as readings.
pub fn read(feed: Feed, body: &[u8]) -> Result<Vec<Reading>, String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "an unreadable answer".to_string())?;
    let num = |x: &Value| x.as_f64().ok_or_else(|| "a missing value".to_string());
    Ok(match feed {
        Feed::Weather => {
            let c = &v["current"];
            let (t, wind, clouds, rain) = (num(&c["temperature_2m"])?, num(&c["wind_speed_10m"])?, num(&c["cloud_cover"])?, num(&c["precipitation"])?);
            let [nt, nw, nc, nr] = WEATHER_READINGS;
            vec![
                Reading { name: nt, value: unit(t, -10.0, 35.0), shown: format!("{t:.0} °C") },
                Reading { name: nw, value: unit(wind, 0.0, 60.0), shown: format!("{wind:.0} km/h") },
                Reading { name: nc, value: unit(clouds, 0.0, 100.0), shown: format!("{clouds:.0}% cloud") },
                Reading { name: nr, value: unit(rain, 0.0, 5.0), shown: format!("{rain:.1} mm rain") },
            ]
        }
        Feed::Iss => {
            let (lat, lon) = (num(&v["latitude"])?, num(&v["longitude"])?);
            let [ns, around] = ISS_READINGS;
            // Its orbit never goes past 51.6° north or south.
            vec![
                Reading { name: ns, value: unit(lat, -51.6, 51.6), shown: format!("{:.1}° {}", lat.abs(), if lat >= 0.0 { "N" } else { "S" }) },
                Reading { name: around, value: unit(lon, -180.0, 180.0), shown: format!("{:.1}° {}", lon.abs(), if lon >= 0.0 { "E" } else { "W" }) },
            ]
        }
        Feed::Quakes => {
            let quakes = v["features"].as_array().ok_or("no list of earthquakes")?;
            // The newest one (the list is newest first) and how many there were.
            let newest = quakes.iter().max_by_key(|q| q["properties"]["time"].as_i64().unwrap_or(0));
            let mag = newest.and_then(|q| q["properties"]["mag"].as_f64()).unwrap_or(0.0);
            let place = newest.and_then(|q| q["properties"]["place"].as_str()).unwrap_or("");
            let [size, count] = QUAKE_READINGS;
            vec![
                Reading { name: size, value: unit(mag, 0.0, 7.0), shown: if place.is_empty() { "none".into() } else { format!("M{mag:.1}, {place}") } },
                Reading { name: count, value: unit(quakes.len() as f64, 0.0, 30.0), shown: format!("{} in the last hour", quakes.len()) },
            ]
        }
    })
}

/// A place found by name: what to call it and where it is.
pub fn read_place(body: &[u8]) -> Result<(String, f64, f64), String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "an unreadable answer".to_string())?;
    let r = v["results"].get(0).ok_or("no place by that name")?;
    let name = [r["name"].as_str(), r["admin1"].as_str(), r["country"].as_str()].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>();
    let mut name = name;
    name.dedup();
    Ok((name.join(", "), r["latitude"].as_f64().ok_or("no latitude")?, r["longitude"].as_f64().ok_or("no longitude")?))
}

enum Msg {
    Read(Feed, Result<Vec<Reading>, String>),
    Place(Result<(String, f64, f64), String>),
}

/// The feeds' fetching and latest readings (in `State::feeds`).
pub struct Feeds {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    busy: Vec<Feed>,
    /// When each feed was last asked (seconds, the frame clock).
    asked: Vec<(Feed, f64)>,
    /// Where the weather was last asked for (a new place asks again at once).
    weather_at: (f64, f64),
    pub readings: Vec<Reading>,
    /// Per feed: what went wrong last time, if anything.
    pub problems: Vec<(Feed, String)>,
    /// A place lookup running, then its result.
    pub finding: bool,
    pub found: Option<Result<(String, f64, f64), String>>,
}

impl Default for Feeds {
    fn default() -> Self {
        let (tx, rx) = channel();
        Feeds { tx, rx, busy: Vec::new(), asked: Vec::new(), weather_at: (f64::NAN, f64::NAN), readings: Vec::new(), problems: Vec::new(), finding: false, found: None }
    }
}

impl Feeds {
    /// Starts any fetch that's due, takes in what came back and fills the modulators. Call once per frame; it does
    /// nothing while every feed is off.
    pub fn update(&mut self, comp: &mut Composition, now: f64) {
        let data = comp.live_data.clone();
        for msg in self.rx.try_iter() {
            match msg {
                Msg::Read(feed, result) => {
                    self.busy.retain(|f| *f != feed);
                    self.problems.retain(|(f, _)| *f != feed);
                    match result {
                        Ok(r) => {
                            self.readings.retain(|o| !r.iter().any(|n| n.name == o.name));
                            self.readings.extend(r);
                        }
                        Err(e) => self.problems.push((feed, e)),
                    }
                }
                Msg::Place(r) => {
                    self.finding = false;
                    self.found = Some(r);
                }
            }
        }
        if (data.latitude, data.longitude) != self.weather_at && data.weather {
            self.asked.retain(|(f, _)| *f != Feed::Weather);
            self.readings.retain(|r| !WEATHER_READINGS.contains(&r.name));
        }
        for feed in Feed::ALL {
            if !feed.on(&data) {
                continue;
            }
            let due = self.asked.iter().find(|(f, _)| *f == feed).is_none_or(|(_, t)| now - t >= feed.every());
            if !due || self.busy.contains(&feed) {
                continue;
            }
            if feed == Feed::Weather {
                self.weather_at = (data.latitude, data.longitude);
            }
            self.asked.retain(|(f, _)| *f != feed);
            self.asked.push((feed, now));
            self.busy.push(feed);
            let (url, tx) = (feed.url(&data), self.tx.clone());
            let _ = std::thread::Builder::new().name("live data".into()).spawn(move || {
                let _ = tx.send(Msg::Read(feed, crate::online::get_bytes(&url).and_then(|b| read(feed, &b))));
            });
        }
        let on = data.readings();
        for r in self.readings.iter().filter(|r| on.contains(&r.name)) {
            if comp.hand(r.name) != Some(r.value) {
                comp.set_hand(r.name, r.value);
            }
        }
    }

    /// Looks a place up by name (Open-Meteo's geocoding); the answer lands in `found`.
    pub fn find_place(&mut self, name: &str) {
        let url = format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=1", crate::support::url_encode(name.trim()));
        let tx = self.tx.clone();
        self.finding = true;
        self.found = None;
        let _ = std::thread::Builder::new().name("find place".into()).spawn(move || {
            let _ = tx.send(Msg::Place(crate::online::get_bytes(&url).and_then(|b| read_place(&b))));
        });
    }

    pub fn reading(&self, name: &str) -> Option<&Reading> {
        self.readings.iter().find(|r| r.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feeds_are_read_and_scaled() {
        let w = read(Feed::Weather, br#"{"current":{"temperature_2m":12.5,"wind_speed_10m":30,"cloud_cover":90,"precipitation":0.0}}"#).unwrap();
        assert_eq!(w.len(), 4);
        assert!((w[0].value - 0.5).abs() < 1e-6, "12.5 °C is half way from -10 to 35");
        assert_eq!((w[1].value, w[2].value, w[3].value), (0.5, 0.9, 0.0));
        let iss = read(Feed::Iss, br#"{"latitude":-51.6,"longitude":90.0}"#).unwrap();
        assert_eq!((iss[0].value, iss[1].value), (0.0, 0.75));
        assert_eq!(iss[1].shown, "90.0° E");
        let q = read(Feed::Quakes, br#"{"features":[{"properties":{"mag":1.2,"time":5,"place":"A"}},{"properties":{"mag":3.5,"time":9,"place":"2 km NE of B"}}]}"#).unwrap();
        assert_eq!(q[0].value, 0.5, "the newest, not the first");
        assert_eq!(q[0].shown, "M3.5, 2 km NE of B");
        assert_eq!(q[1].shown, "2 in the last hour");
        assert!(read(Feed::Weather, b"{}").is_err());
        let place = read_place(br#"{"results":[{"name":"Berlin","admin1":"State of Berlin","country":"Germany","latitude":52.52,"longitude":13.41}]}"#).unwrap();
        assert_eq!(place, ("Berlin, State of Berlin, Germany".into(), 52.52, 13.41));
        assert!(read_place(br#"{"generationtime_ms":0.1}"#).is_err());
    }

    /// Readings fill only the modulators of the feeds that are on; nothing is fetched while all are off.
    #[test]
    fn readings_fill_modulators_of_feeds_that_are_on() {
        let mut comp = Composition::default();
        let mut f = Feeds::default();
        f.update(&mut comp, 0.0);
        assert!(f.busy.is_empty() && comp.modulators.is_empty(), "all off: nothing happens");
        f.readings = read(Feed::Iss, br#"{"latitude":0.0,"longitude":0.0}"#).unwrap();
        f.update(&mut comp, 0.0);
        assert!(comp.modulators.is_empty(), "the ISS feed is off");
        comp.live_data.iss = true;
        f.busy.push(Feed::Iss); // (no fetch from a test)
        f.update(&mut comp, 0.0);
        assert_eq!(comp.hand("ISS north-south"), Some(0.5));
        assert_eq!(comp.hand("ISS around the world"), Some(0.5));
        assert_eq!(comp.modulators.len(), 2);
    }

    /// The real feeds: `PHOSPHOR_ONLINE_TEST=1 cargo test live_data_online -- --nocapture`.
    #[test]
    fn live_data_online() {
        if std::env::var_os("PHOSPHOR_ONLINE_TEST").is_none() {
            return;
        }
        let place = read_place(&crate::online::get_bytes("https://geocoding-api.open-meteo.com/v1/search?name=Reykjavik&count=1").unwrap()).unwrap();
        println!("LIVE place {place:?}");
        let data = LiveData { weather: true, place: place.0, latitude: place.1, longitude: place.2, iss: true, quakes: true };
        for feed in Feed::ALL {
            let r = read(feed, &crate::online::get_bytes(&feed.url(&data)).unwrap()).unwrap();
            println!("LIVE {feed:?} {:?}", r.iter().map(|r| format!("{} = {:.2} ({})", r.name, r.value, r.shown)).collect::<Vec<_>>());
        }
    }
}
