//! Starter shows (Online › Start a show): one click builds a show from the online libraries for a mood. Three layers,
//! each with a row of clips from one search; clips land in their cells as their downloads finish.

use phosphor_engine::BlendMode;

use super::{Kind, Source};

/// One layer of a starter show: where its clips come from and how it mixes.
pub struct Row {
    pub layer: &'static str,
    pub source: Source,
    pub kind: Kind,
    pub query: &'static str,
    pub blend: BlendMode,
    pub opacity: f32,
}

pub struct Starter {
    pub name: &'static str,
    pub about: &'static str,
    pub rows: [Row; 3],
}

/// Clips per layer.
pub const CLIPS: usize = 4;
/// Files bigger than this are skipped (when the library says how big): a starter should arrive in minutes.
pub const MAX_SIZE: u64 = 80 << 20;

const fn row(layer: &'static str, source: Source, kind: Kind, query: &'static str, blend: BlendMode, opacity: f32) -> Row {
    Row { layer, source, kind, query, blend, opacity }
}

pub const STARTERS: [Starter; 5] = [
    Starter {
        name: "Space",
        about: "NASA's nebulas and Earth from orbit, with a MilkDrop visualiser on top",
        rows: [
            row("Nebulas", Source::Nasa, Kind::Video, "nebula", BlendMode::Normal, 1.0),
            row("Earth", Source::Nasa, Kind::Video, "earth timelapse", BlendMode::Screen, 0.6),
            row("Visualiser", Source::MilkDrop, Kind::Visualizer, "space", BlendMode::Add, 0.5),
        ],
    },
    Starter {
        name: "Nature",
        about: "Waves, auroras and forests from Wikimedia Commons, softly layered",
        rows: [
            row("Ocean", Source::Commons, Kind::Video, "ocean waves", BlendMode::Normal, 1.0),
            row("Aurora", Source::Commons, Kind::Video, "aurora borealis timelapse", BlendMode::Screen, 0.7),
            row("Forest", Source::Commons, Kind::Video, "forest", BlendMode::Overlay, 0.5),
        ],
    },
    Starter {
        name: "Retro",
        about: "Old public-domain films (dancing, cars, cartoons) from the Internet Archive",
        rows: [
            row("Dancing", Source::Archive, Kind::Video, "dance", BlendMode::Normal, 1.0),
            row("Cars", Source::Archive, Kind::Video, "automobile", BlendMode::Screen, 0.6),
            row("Cartoons", Source::Archive, Kind::Video, "cartoon", BlendMode::Add, 0.5),
        ],
    },
    Starter {
        name: "Club",
        about: "MilkDrop visualisers that move to the music: dancers, fractals and tunnels",
        rows: [
            row("Dancers", Source::MilkDrop, Kind::Visualizer, "dancer", BlendMode::Normal, 1.0),
            row("Fractals", Source::MilkDrop, Kind::Visualizer, "fractal", BlendMode::Screen, 0.7),
            row("Tunnels", Source::MilkDrop, Kind::Visualizer, "tunnel", BlendMode::Add, 0.5),
        ],
    },
    Starter {
        name: "Gallery",
        about: "Paintings from the Art Institute of Chicago, with sky and sea from NASA and Commons",
        rows: [
            row("Paintings", Source::Artic, Kind::Picture, "landscape", BlendMode::Normal, 1.0),
            row("Sky", Source::Nasa, Kind::Picture, "clouds", BlendMode::Multiply, 0.5),
            row("Sea", Source::Commons, Kind::Picture, "sea painting", BlendMode::Screen, 0.4),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_starter_asks_libraries_for_what_they_have() {
        for s in &STARTERS {
            for r in &s.rows {
                assert!(r.source.kinds().contains(&r.kind), "{}: {} has no {:?}", s.name, r.source.name(), r.kind);
                assert!(!r.query.is_empty() && (0.0..=1.0).contains(&r.opacity));
            }
        }
    }
}
