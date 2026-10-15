//! `--bench` mode: build a composition from the command line, run it for N seconds,
//! print one JSON stats line per second plus a summary, optionally save a PNG snapshot.
//!
//!   phosphor --bench 20 --gen plasma --gen noise --image photo.jpg --fx kaleidoscope \
//!            --size 3840x2160 --snapshot out.png

use phosphor_engine::{BlendMode, Composition};

#[derive(Clone, Debug, Default)]
pub struct BenchConfig {
    pub seconds: f64,
    pub generators: Vec<String>,
    /// `--text "words"`: a text layer on top.
    pub texts: Vec<String>,
    pub images: Vec<String>,
    pub movies: Vec<String>,
    pub effects: Vec<String>,
    pub size: Option<(u32, u32)>,
    pub snapshot: Option<String>,
    /// Export the bench project as a standalone show into this folder (tests Export Show).
    pub export: Option<String>,
    /// Bring this panel to the front (its title, e.g. "Control Surface"), for screenshots.
    pub tab: Option<String>,
    /// Select this layer/group id (after the project loads), for UI shots of the Inspector.
    pub select: Option<u64>,
    /// Save a picture of the whole Phosphor window here at the end (checks UI changes).
    pub ui_shot: Option<String>,
    /// A live input layer: "format|url" (e.g. "lavfi|testsrc=size=640x360:rate=30", "avfoundation|FaceTime HD Camera").
    pub live: Option<String>,
    /// Send the output to the display whose name contains this (tests screen placement).
    pub send_output: Option<String>,
    /// Open the default audio input and report its level.
    pub audio: bool,
    /// A .phosphor project to load instead of building one from the flags.
    pub project: Option<String>,
    /// Record the run to this file (.mp4 = H.264, .mov = ProRes) to exercise the streaming path.
    pub record: Option<String>,
    /// Trigger clip column N (1-based) at `trigger_at` seconds, to exercise transitions.
    pub trigger_column: Option<usize>,
    pub trigger_at: f64,
    /// Host a network session and analyse viewer audio.
    pub net: bool,
    /// Turn NDI output on.
    pub ndi: bool,
    /// Require this password for remote control.
    pub remote_password: Option<String>,
    /// Watch this folder (prints how many media files landed in the bin).
    pub watch: Option<String>,
    /// Render this many live effect previews every frame (to measure the explorer's cost).
    pub previews: usize,
}

impl BenchConfig {
    pub fn from_args(args: impl Iterator<Item = String>) -> Option<Self> {
        let args: Vec<String> = args.collect();
        let mut cfg = BenchConfig::default();
        let mut bench = false;
        let mut i = 0;
        while i < args.len() {
            let value = args.get(i + 1).cloned();
            match (args[i].as_str(), value) {
                ("--bench", Some(v)) => {
                    bench = true;
                    cfg.seconds = v.parse().unwrap_or(10.0);
                }
                ("--gen", Some(v)) => cfg.generators.push(v),
                ("--text", Some(v)) => cfg.texts.push(v),
                ("--image", Some(v)) => cfg.images.push(v),
                ("--movie", Some(v)) => cfg.movies.push(v),
                ("--fx", Some(v)) => cfg.effects.push(v),
                ("--snapshot", Some(v)) => cfg.snapshot = Some(v),
                ("--export", Some(v)) => cfg.export = Some(v),
                ("--tab", Some(v)) => cfg.tab = Some(v),
                ("--select", Some(v)) => cfg.select = v.parse().ok(),
                ("--ui-shot", Some(v)) => cfg.ui_shot = Some(v),
                ("--live", Some(v)) => cfg.live = Some(v),
                ("--send-output", Some(v)) => cfg.send_output = Some(v),
                ("--project", Some(v)) => cfg.project = Some(v),
                ("--record", Some(v)) => cfg.record = Some(v),
                ("--remote-password", Some(v)) => cfg.remote_password = Some(v),
                ("--watch", Some(v)) => cfg.watch = Some(v),
                ("--previews", Some(v)) => cfg.previews = v.parse().unwrap_or(0),
                ("--column", Some(v)) => cfg.trigger_column = v.parse::<usize>().ok().map(|c| c.saturating_sub(1)),
                ("--at", Some(v)) => cfg.trigger_at = v.parse().unwrap_or(1.0),
                ("--audio", _) => {
                    cfg.audio = true;
                    i += 1;
                    continue;
                }
                ("--net", _) => {
                    cfg.net = true;
                    i += 1;
                    continue;
                }
                ("--ndi", _) => {
                    cfg.ndi = true;
                    i += 1;
                    continue;
                }
                ("--size", Some(v)) => {
                    cfg.size = v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
                }
                _ => {
                    i += 1;
                    continue;
                }
            }
            i += 2;
        }
        bench.then_some(cfg)
    }

    /// Bottom-to-top: generators, then images, then movies. Upper layers screen-blend at 70%
    /// so every layer contributes to the picture (and to the cost).
    pub fn composition(&self) -> Composition {
        if let Some(path) = &self.project
            && let Ok(json) = std::fs::read_to_string(path)
            && let Ok(comp) = Composition::from_json(&json)
        {
            return comp;
        }
        let mut comp = Composition::default();
        if let Some((w, h)) = self.size {
            comp.width = w;
            comp.height = h;
        }
        if let Some((format, url)) = self.live.as_deref().and_then(|l| l.split_once('|')) {
            let layer = comp.add_layer();
            layer.source = phosphor_engine::SourceSpec::Live { name: "Live".into(), format: format.into(), url: url.into() };
            return comp;
        }
        let mut sources: Vec<Box<dyn Fn(&mut phosphor_engine::LayerState)>> = Vec::new();
        for g in &self.generators {
            let g = g.clone();
            sources.push(Box::new(move |l| l.set_generator(&g)));
        }
        for p in &self.images {
            let p = p.clone();
            sources.push(Box::new(move |l| l.source = crate::media::spec_for_path(&p)));
        }
        for p in &self.movies {
            let p = p.clone();
            sources.push(Box::new(move |l| l.source = crate::media::spec_for_path(&p)));
        }
        for t in &self.texts {
            let t = t.replace("\\n", "\n");
            sources.push(Box::new(move |l| l.source = phosphor_engine::SourceSpec::Text(phosphor_engine::TextStyle { text: t.clone(), ..crate::text::default_style() })));
        }
        for (i, set_source) in sources.iter().enumerate() {
            let effects: Vec<_> = self.effects.iter().filter_map(|e| comp.new_effect(e)).collect();
            let layer = comp.add_layer();
            set_source(layer);
            layer.effects = effects;
            if i > 0 {
                layer.blend = BlendMode::Screen;
                layer.opacity = 0.7;
            }
        }
        comp
    }
}
