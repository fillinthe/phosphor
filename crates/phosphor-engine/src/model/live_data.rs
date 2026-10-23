//! Live data from the internet (weather, the ISS, earthquakes) as shared modulators: which feeds a show uses. The
//! app fetches them (`phosphor::live_data`) and fills the modulators named in `LiveData::readings`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveData {
    /// The weather where `place` is (Open-Meteo).
    pub weather: bool,
    /// The place's name, as found ("Berlin, Germany"); empty = not picked yet.
    pub place: String,
    pub latitude: f64,
    pub longitude: f64,
    /// Where the International Space Station is.
    pub iss: bool,
    /// Earthquakes around the world in the last hour (USGS).
    pub quakes: bool,
}

/// Each feed's modulators.
pub const WEATHER_READINGS: [&str; 4] = ["Weather temperature", "Weather wind", "Weather clouds", "Weather rain"];
pub const ISS_READINGS: [&str; 2] = ["ISS north-south", "ISS around the world"];
pub const QUAKE_READINGS: [&str; 2] = ["Earthquake size", "Earthquakes this hour"];

impl LiveData {
    /// The modulators the feeds that are on fill in.
    pub fn readings(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.weather && !self.place.is_empty() {
            out.extend(WEATHER_READINGS);
        }
        if self.iss {
            out.extend(ISS_READINGS);
        }
        if self.quakes {
            out.extend(QUAKE_READINGS);
        }
        out
    }
}
