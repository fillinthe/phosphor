//! Built-in effect and generator catalog. Each entry maps to a WGSL fragment entry point
//! in `shaders/effects.wgsl` or `shaders/generators.wgsl`; parameters land in `u.p[..]`.

pub const MAX_PARAMS: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct Param {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

const fn p(name: &'static str, min: f32, max: f32, default: f32) -> Param {
    Param { name, min, max, default }
}

#[derive(Clone, Copy, Debug)]
pub struct ShaderDesc {
    pub id: &'static str,
    pub name: &'static str,
    pub entry: &'static str,
    pub params: &'static [Param],
}

impl ShaderDesc {
    pub fn defaults(&self) -> Vec<f32> {
        self.params.iter().map(|p| p.default).collect()
    }
}

pub const EFFECTS: &[ShaderDesc] = &[
    ShaderDesc { id: "invert", name: "Invert", entry: "fx_invert", params: &[p("Amount", 0.0, 1.0, 1.0)] },
    ShaderDesc {
        id: "color_adjust",
        name: "Color Adjust",
        entry: "fx_color_adjust",
        params: &[
            p("Hue", -0.5, 0.5, 0.08),
            p("Saturation", 0.0, 2.0, 1.5),
            p("Brightness", -1.0, 1.0, 0.0),
            p("Contrast", 0.0, 2.0, 1.0),
        ],
    },
    ShaderDesc {
        id: "rgb_shift",
        name: "RGB Shift",
        entry: "fx_rgb_shift",
        params: &[p("Amount", 0.0, 0.1, 0.01), p("Angle", 0.0, 1.0, 0.0)],
    },
    ShaderDesc {
        id: "kaleidoscope",
        name: "Kaleidoscope",
        entry: "fx_kaleidoscope",
        params: &[p("Segments", 2.0, 16.0, 6.0), p("Rotation", 0.0, 1.0, 0.0), p("Zoom", 0.25, 4.0, 1.0)],
    },
    ShaderDesc { id: "pixelate", name: "Pixelate", entry: "fx_pixelate", params: &[p("Cell Size", 1.0, 128.0, 16.0)] },
    ShaderDesc { id: "mirror", name: "Mirror", entry: "fx_mirror", params: &[p("Mode", 0.0, 2.0, 0.0)] },
    ShaderDesc { id: "posterize", name: "Posterize", entry: "fx_posterize", params: &[p("Levels", 2.0, 16.0, 4.0)] },
    ShaderDesc {
        id: "edges",
        name: "Edge Detect",
        entry: "fx_edges",
        params: &[p("Strength", 0.0, 8.0, 2.0), p("Mix", 0.0, 1.0, 1.0)],
    },
    ShaderDesc {
        id: "zoom_rotate",
        name: "Zoom / Rotate",
        entry: "fx_zoom_rotate",
        params: &[p("Zoom", 0.25, 4.0, 1.4), p("Rotate", -1.0, 1.0, 0.0), p("Spin", -2.0, 2.0, 0.15)],
    },
    ShaderDesc {
        id: "wave",
        name: "Wave Warp",
        entry: "fx_wave",
        params: &[p("Amplitude", 0.0, 0.1, 0.02), p("Frequency", 1.0, 50.0, 10.0), p("Speed", 0.0, 10.0, 2.0)],
    },
    ShaderDesc {
        id: "threshold",
        name: "Threshold",
        entry: "fx_threshold",
        params: &[p("Level", 0.0, 1.0, 0.5), p("Softness", 0.0, 0.5, 0.05)],
    },
    ShaderDesc {
        id: "rutt_etra",
        name: "Rutt-Etra",
        entry: "fs_rutt",
        params: &[
            p("Scanlines", 4.0, 400.0, 120.0),
            p("Points per line", 8.0, 512.0, 240.0),
            p("Extrude", 0.0, 2.0, 0.5),
            p("Rotate X", -1.0, 1.0, 0.0),
            p("Rotate Y", -1.0, 1.0, 0.0),
            p("Zoom", 0.25, 4.0, 1.0),
            p("Line width", 1.0, 8.0, 2.0),
            p("Brightness", 0.0, 3.0, 1.2),
            p("Source colour", 0.0, 1.0, 0.6),
            p("Perspective", 0.0, 2.0, 0.6),
        ],
    },
    ShaderDesc {
        id: "strobe",
        name: "Strobe",
        entry: "fx_strobe",
        params: &[p("Rate (Hz)", 0.5, 30.0, 8.0), p("Duty", 0.05, 0.95, 0.5)],
    },
];

pub const GENERATORS: &[ShaderDesc] = &[
    ShaderDesc {
        id: "plasma",
        name: "Plasma",
        entry: "gen_plasma",
        params: &[p("Speed", 0.0, 4.0, 1.0), p("Scale", 1.0, 20.0, 6.0), p("Hue", 0.0, 1.0, 0.0)],
    },
    ShaderDesc {
        id: "rings",
        name: "Rings",
        entry: "gen_rings",
        params: &[p("Count", 1.0, 40.0, 10.0), p("Speed", -4.0, 4.0, 1.0), p("Width", 0.02, 0.98, 0.5)],
    },
    ShaderDesc {
        id: "gradient",
        name: "Gradient Sweep",
        entry: "gen_gradient",
        params: &[p("Speed", -2.0, 2.0, 0.2), p("Angle", 0.0, 1.0, 0.0)],
    },
    ShaderDesc {
        id: "checker",
        name: "Checkerboard",
        entry: "gen_checker",
        params: &[p("Cells", 2.0, 64.0, 8.0), p("Speed", -2.0, 2.0, 0.0)],
    },
    ShaderDesc {
        id: "noise",
        name: "Noise Field",
        entry: "gen_noise",
        params: &[p("Scale", 1.0, 32.0, 6.0), p("Speed", 0.0, 4.0, 0.5)],
    },
    ShaderDesc { id: "color_bars", name: "Color Bars", entry: "gen_color_bars", params: &[] },
    ShaderDesc {
        id: "solid",
        name: "Solid Color",
        entry: "gen_solid",
        params: &[p("Red", 0.0, 1.0, 1.0), p("Green", 0.0, 1.0, 1.0), p("Blue", 0.0, 1.0, 1.0)],
    },
];

pub fn effect(id: &str) -> Option<&'static ShaderDesc> {
    EFFECTS.iter().find(|d| d.id == id)
}

pub fn generator(id: &str) -> Option<&'static ShaderDesc> {
    GENERATORS.iter().find(|d| d.id == id)
}

/// All WGSL concatenated into one module: compiled once, one pipeline per entry point.
pub(crate) fn wgsl_source() -> String {
    [
        include_str!("shaders/common.wgsl"),
        include_str!("shaders/composite.wgsl"),
        include_str!("shaders/effects.wgsl"),
        include_str!("shaders/generators.wgsl"),
        include_str!("shaders/mapping.wgsl"),
        include_str!("shaders/ruttetra.wgsl"),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses and validates all WGSL without a GPU, and checks every catalog entry point exists.
    #[test]
    fn wgsl_validates_and_entry_points_exist() {
        let module = naga::front::wgsl::parse_str(&wgsl_source()).unwrap_or_else(|e| {
            panic!("{}", e.emit_to_string(&wgsl_source()));
        });
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&wgsl_source())));
        let entries: Vec<_> = module.entry_points.iter().map(|e| e.name.as_str()).collect();
        for d in EFFECTS.iter().chain(GENERATORS) {
            assert!(entries.contains(&d.entry), "missing entry point {}", d.entry);
            assert!(d.params.len() <= MAX_PARAMS);
        }
    }
}
