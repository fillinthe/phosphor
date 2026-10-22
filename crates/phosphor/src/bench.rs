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
    /// `--rate display|60|30`: the engine's render rate for this run (default: the 60 fps preference).
    pub rate: Option<String>,
    /// Export the bench project as a standalone show into this folder (tests Export Show).
    pub export: Option<String>,
    /// Bring this panel to the front (its title, e.g. "Control Surface"), for screenshots. "all" = every panel in
    /// turn (one per 0.2 s, a TAB line each): catches a panel that panics when drawn.
    pub tab: Option<String>,
    /// Window size in points ("1280x800"): UI pictures at smaller sizes.
    pub window: Option<(f32, f32)>,
    /// Open a tab inside the unified areas ("Mixer", "Effects", "Stream & Record"…) for UI pictures.
    pub area_tab: Option<String>,
    /// Interface brightness for UI pictures (0 darkest … 1 light).
    pub brightness: Option<f32>,
    /// Play direction for movie layers ("reverse", "pingpong"): playback checks.
    pub direction: Option<String>,
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
    /// Play movie sound (bench runs are silent otherwise).
    pub sound: bool,
    /// `--movie-sound`: movie sound mixed into `--record` / `--stream` (silent here unless `--sound`).
    pub movie_sound: bool,
    /// Start the cue list's show clock at launch (timecoded cues).
    pub show_clock: bool,
    /// `--record` writes Hap instead of H.264 / ProRes.
    pub hap: bool,
    /// `--hap-kind alpha|q|qalpha`: which Hap `--record … --hap` writes (default plain Hap).
    pub hap_kind: Option<String>,
    /// `--record x.mp4 --hevc`: HEVC through the Mac encoder.
    pub hevc: bool,
    /// `--stream rtmp://…` (custom RTMP, no key) and `--stream2 rtmp://…` (second destination) and `--stream-copy file.mp4`.
    pub stream: Option<String>,
    /// `--right-click-studio`: right-clicks the middle of the Mapping Studio's output canvas (shows its menu).
    pub right_click_studio: bool,
    /// `--drag x,y:x,y` (window points; `studio` = the middle of the Mapping Studio canvas): a real drag with the
    /// mouse, pressed at the first point, moved there in steps, released at the second (drag and drop tests).
    pub drag: Option<(String, String)>,
    pub stream2: Option<String>,
    pub stream_copy: Option<String>,
    /// Open the import chooser for this Resolume / VDMX file.
    pub import: Option<String>,
    /// Start in this built-in workspace ("Mapping", "Performance"…).
    pub layout: Option<String>,
    /// Run Quick actions at launch, comma-separated: surprise, layer, shuffle, react, fill, instant.
    pub quick: Option<String>,
    /// A .phosphor project to load instead of building one from the flags.
    pub project: Option<String>,
    /// Record the run to this file (.mp4 = H.264, .mov = ProRes) to exercise the streaming path.
    pub record: Option<String>,
    /// `--render x.mp4`: render the show offline (frame by frame) for `PHOSPHOR_RENDER_SECS` (default 2) seconds.
    pub render: Option<String>,
    /// Trigger clip column N (1-based) at `trigger_at` seconds, to exercise transitions.
    pub trigger_column: Option<usize>,
    pub trigger_at: f64,
    /// Host a network session and analyse viewer audio.
    pub net: bool,
    /// Follow the Phosphor at this address (a FOLLOW line at the end).
    pub follow: Option<String>,
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
                ("--rate", Some(v)) => cfg.rate = Some(v),
                ("--stream", Some(v)) => cfg.stream = Some(v),
                ("--stream2", Some(v)) => cfg.stream2 = Some(v),
                ("--stream-copy", Some(v)) => cfg.stream_copy = Some(v),
                ("--export", Some(v)) => cfg.export = Some(v),
                ("--tab", Some(v)) => cfg.tab = Some(v),
                ("--drag", Some(v)) => cfg.drag = v.split_once(':').map(|(a, b)| (a.to_string(), b.to_string())),
                ("--follow", Some(v)) => cfg.follow = Some(v),
                ("--window", Some(v)) => cfg.window = v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))),
                ("--area-tab", Some(v)) => cfg.area_tab = Some(v),
                ("--brightness", Some(v)) => cfg.brightness = v.parse().ok(),
                ("--direction", Some(v)) => cfg.direction = Some(v),
                ("--select", Some(v)) => cfg.select = v.parse().ok(),
                ("--ui-shot", Some(v)) => cfg.ui_shot = Some(v),
                ("--live", Some(v)) => cfg.live = Some(v),
                ("--send-output", Some(v)) => cfg.send_output = Some(v),
                ("--project", Some(v)) => cfg.project = Some(v),
                ("--record", Some(v)) => cfg.record = Some(v),
                ("--render", Some(v)) => cfg.render = Some(v),
                ("--remote-password", Some(v)) => cfg.remote_password = Some(v),
                ("--watch", Some(v)) => cfg.watch = Some(v),
                ("--previews", Some(v)) => cfg.previews = v.parse().unwrap_or(0),
                ("--import", Some(v)) => cfg.import = Some(v),
                ("--layout", Some(v)) => cfg.layout = Some(v),
                ("--quick", Some(v)) => cfg.quick = Some(v),
                ("--column", Some(v)) => cfg.trigger_column = v.parse::<usize>().ok().map(|c| c.saturating_sub(1)),
                ("--at", Some(v)) => cfg.trigger_at = v.parse().unwrap_or(1.0),
                ("--hap-kind", Some(v)) => cfg.hap_kind = Some(v),
                ("--hap", _) => {
                    cfg.hap = true;
                    i += 1;
                    continue;
                }
                ("--right-click-studio", _) => {
                    cfg.right_click_studio = true;
                    i += 1;
                    continue;
                }
                ("--hevc", _) => {
                    cfg.hevc = true;
                    i += 1;
                    continue;
                }
                ("--show-clock", _) => {
                    cfg.show_clock = true;
                    i += 1;
                    continue;
                }
                ("--movie-sound", _) => {
                    // Movie sound into --record / --stream, silent on this Mac.
                    cfg.movie_sound = true;
                    i += 1;
                    continue;
                }
                ("--sound", _) => {
                    cfg.sound = true;
                    i += 1;
                    continue;
                }
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
        let mut comp = self.composition_base();
        if let Some(d) = self.direction.as_deref() {
            let dir = match d {
                "reverse" => phosphor_engine::PlayDirection::Reverse,
                "pingpong" | "ping-pong" => phosphor_engine::PlayDirection::PingPong,
                "random" => phosphor_engine::PlayDirection::Random,
                _ => phosphor_engine::PlayDirection::Forward,
            };
            for l in &mut comp.layers {
                l.direction = dir;
            }
        }
        comp
    }

    fn composition_base(&self) -> Composition {
        if let Some(path) = &self.project
            && let Ok(json) = std::fs::read_to_string(path)
            && let Ok(comp) = Composition::from_json(&json)
        {
            return comp;
        }
        // An exported show benches its own project when nothing else is asked for (checks a packed show runs).
        #[cfg(not(target_arch = "wasm32"))]
        if self.project.is_none()
            && self.live.is_none()
            && self.generators.is_empty()
            && self.images.is_empty()
            && self.movies.is_empty()
            && self.texts.is_empty()
            && let Some(comp) = crate::show::bundled().and_then(|b| crate::show::load(b).ok())
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
            // `--fx id` or `--fx id=v0,v1,…` (values in order; missing ones keep their defaults).
            let effects: Vec<_> = self
                .effects
                .iter()
                .filter_map(|e| {
                    let (id, vals) = e.split_once('=').map_or((e.as_str(), ""), |(i, v)| (i, v));
                    let mut fx = comp.new_effect(id)?;
                    for (k, v) in vals.split(',').filter(|v| !v.is_empty()).enumerate() {
                        if let (Some(slot), Ok(v)) = (fx.values.get_mut(k), v.trim().parse::<f32>()) {
                            *slot = v;
                        }
                    }
                    Some(fx)
                })
                .collect();
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
