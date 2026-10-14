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
            p("Draw (0 lines,1 mesh,2 tri,3 vert)", 0.0, 3.0, 0.0),
            p("Displace (0 luma, 1 RGB)", 0.0, 1.0, 0.0),
            p("Displace from layer # (0 = this)", 0.0, 16.0, 0.0),
        ],
    },
    ShaderDesc {
        id: "rutt_etra_points",
        name: "Rutt-Etra Points",
        entry: "fs_rutt_points",
        params: &[
            p("Rows", 4.0, 400.0, 120.0),
            p("Points per row", 8.0, 512.0, 240.0),
            p("Extrude", 0.0, 2.0, 0.5),
            p("Rotate X", -1.0, 1.0, 0.0),
            p("Rotate Y", -1.0, 1.0, 0.0),
            p("Zoom", 0.25, 4.0, 1.0),
            p("Point size", 1.0, 8.0, 2.0),
            p("Brightness", 0.0, 3.0, 1.5),
            p("Source colour", 0.0, 1.0, 0.8),
            p("Perspective", 0.0, 2.0, 0.6),
            p("(unused)", 0.0, 0.0, 0.0),
            p("Displace (0 luma, 1 RGB)", 0.0, 1.0, 0.0),
            p("Displace from layer # (0 = this)", 0.0, 16.0, 0.0),
        ],
    },
    ShaderDesc { id: "blur", name: "Gaussian Blur", entry: "fx_blur", params: &[p("Radius (px)", 0.0, 64.0, 8.0)] },
    ShaderDesc {
        id: "zoom_blur",
        name: "Zoom Blur",
        entry: "fx_zoom_blur",
        params: &[p("Amount", 0.0, 1.0, 0.2), p("Center X", 0.0, 1.0, 0.5), p("Center Y", 0.0, 1.0, 0.5)],
    },
    ShaderDesc {
        id: "motion_blur",
        name: "Directional Blur",
        entry: "fx_motion_blur",
        params: &[p("Length", 0.0, 0.3, 0.05), p("Angle", 0.0, 1.0, 0.0)],
    },
    ShaderDesc {
        id: "glow",
        name: "Glow",
        entry: "fx_glow",
        params: &[p("Radius (px)", 1.0, 64.0, 16.0), p("Threshold", 0.0, 1.0, 0.6), p("Intensity", 0.0, 3.0, 1.0)],
    },
    ShaderDesc {
        id: "trails",
        name: "Trails / Feedback",
        entry: "fx_trails",
        params: &[
            p("Decay", 0.0, 0.995, 0.9),
            p("Zoom", 0.8, 1.2, 1.0),
            p("Rotate", -0.05, 0.05, 0.0),
            p("Mode (0 max,1 add,2 mix)", 0.0, 2.0, 0.0),
        ],
    },
    ShaderDesc { id: "technicolor", name: "Technicolor", entry: "fx_technicolor", params: &[p("Amount", 0.0, 1.0, 1.0)] },
    ShaderDesc { id: "bleach", name: "Bleach Bypass", entry: "fx_bleach", params: &[p("Amount", 0.0, 1.0, 1.0)] },
    ShaderDesc {
        id: "vignette",
        name: "Vignette",
        entry: "fx_vignette",
        params: &[p("Radius", 0.2, 1.2, 0.8), p("Softness", 0.01, 1.0, 0.45), p("Amount", 0.0, 1.0, 1.0)],
    },
    ShaderDesc { id: "grain", name: "Film Grain", entry: "fx_grain", params: &[p("Amount", 0.0, 1.0, 0.25), p("Flicker", 0.0, 0.5, 0.05)] },
    ShaderDesc {
        id: "crt",
        name: "CRT",
        entry: "fx_crt",
        params: &[p("Scanlines", 0.0, 1.0, 0.5), p("Aperture mask", 0.0, 1.0, 0.4), p("Curvature", 0.0, 1.0, 0.3), p("Line density", 0.25, 2.0, 0.5)],
    },
    ShaderDesc {
        id: "glitch",
        name: "Analog Glitch",
        entry: "fx_glitch",
        params: &[p("Amount", 0.0, 0.3, 0.05), p("Bands", 2.0, 60.0, 12.0), p("Rate (Hz)", 1.0, 30.0, 8.0), p("Density", 0.0, 1.0, 0.4)],
    },
    ShaderDesc { id: "halftone", name: "Halftone", entry: "fx_halftone", params: &[p("Dot size (px)", 2.0, 48.0, 10.0), p("Colour", 0.0, 1.0, 0.0)] },
    ShaderDesc { id: "sharpen", name: "Sharpen", entry: "fx_sharpen", params: &[p("Amount", 0.0, 4.0, 1.0)] },
    ShaderDesc {
        id: "emboss",
        name: "Emboss",
        entry: "fx_emboss",
        params: &[p("Depth", 0.0, 10.0, 3.0), p("Angle", 0.0, 1.0, 0.125), p("Distance (px)", 0.5, 6.0, 1.5), p("Colour", 0.0, 1.0, 0.0)],
    },
    ShaderDesc { id: "tile", name: "Tile", entry: "fx_tile", params: &[p("Tiles", 1.0, 12.0, 2.0), p("Mirror", 0.0, 1.0, 1.0)] },
    ShaderDesc {
        id: "chroma_key",
        name: "Chroma Key",
        entry: "fx_chroma_key",
        params: &[p("Key hue", 0.0, 1.0, 0.33), p("Tolerance", 0.0, 0.3, 0.08), p("Softness", 0.0, 0.3, 0.05)],
    },
    ShaderDesc {
        id: "gamma",
        name: "Gamma / Lift / Gain",
        entry: "fx_gamma",
        params: &[p("Gamma", 0.2, 3.0, 1.0), p("Lift", -0.5, 0.5, 0.0), p("Gain", 0.0, 2.0, 1.0)],
    },
    ShaderDesc { id: "solarize", name: "Solarize", entry: "fx_solarize", params: &[p("Threshold", 0.0, 1.0, 0.5), p("Amount", 0.0, 1.0, 1.0)] },
    ShaderDesc {
        id: "slitscan",
        name: "Slit-scan",
        entry: "fx_slitscan",
        params: &[p("Slit position", 0.0, 1.0, 0.5), p("Shift (px)", 0.0, 8.0, 1.0), p("Vertical", 0.0, 1.0, 0.0)],
    },
    ShaderDesc {
        id: "edge_glow",
        name: "Edge Glow",
        entry: "fx_edge_glow",
        params: &[p("Radius (px)", 1.0, 64.0, 8.0), p("Edge strength", 0.0, 8.0, 3.0), p("Intensity", 0.0, 3.0, 1.2)],
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
        id: "tunnel",
        name: "Tunnel",
        entry: "gen_tunnel",
        params: &[p("Depth scale", 0.05, 1.0, 0.3), p("Speed", -4.0, 4.0, 1.0), p("Segments", 2.0, 32.0, 8.0), p("Hue", 0.0, 1.0, 0.0)],
    },
    ShaderDesc {
        id: "starfield",
        name: "Starfield",
        entry: "gen_starfield",
        params: &[p("Density", 2.0, 30.0, 10.0), p("Speed", 0.0, 10.0, 2.0), p("Brightness", 0.0, 2.0, 1.0)],
    },
    ShaderDesc {
        id: "spiral",
        name: "Spiral",
        entry: "gen_spiral",
        params: &[p("Arms", 1.0, 12.0, 3.0), p("Twist", 0.0, 20.0, 6.0), p("Speed", -6.0, 6.0, 2.0)],
    },
    ShaderDesc {
        id: "voronoi",
        name: "Voronoi Cells",
        entry: "gen_voronoi",
        params: &[p("Scale", 1.0, 20.0, 5.0), p("Speed", 0.0, 3.0, 0.5), p("Hue", 0.0, 1.0, 0.0)],
    },
    ShaderDesc {
        id: "lines",
        name: "Oscilloscope Lines",
        entry: "gen_lines",
        params: &[p("Lines", 1.0, 12.0, 6.0), p("Amplitude", 0.0, 0.5, 0.2), p("Frequency", 0.5, 8.0, 2.0), p("Speed", -6.0, 6.0, 2.0)],
    },
    ShaderDesc {
        id: "solid",
        name: "Solid Color",
        entry: "gen_solid",
        params: &[p("Red", 0.0, 1.0, 1.0), p("Green", 0.0, 1.0, 1.0), p("Blue", 0.0, 1.0, 1.0)],
    },
];

/// Effects whose entry point is really a sequence of passes. The last pass may read the chain's
/// dry input through `tex1`.
pub fn multipass(entry: &str) -> &'static [&'static str] {
    match entry {
        "fx_blur" => &["fx_blur_h", "fx_blur_v"],
        "fx_glow" => &["fx_bright", "fx_blur_h", "fx_blur_v", "fx_glow_add"],
        "fx_edge_glow" => &["fx_edges_only", "fx_blur_h", "fx_blur_v", "fx_glow_add"],
        _ => &[],
    }
}

/// Every fragment entry point an effect can use (including sub-passes), for pipeline warm-up.
pub fn all_effect_entries() -> Vec<&'static str> {
    let mut out = Vec::new();
    for d in EFFECTS {
        let sub = multipass(d.entry);
        if sub.is_empty() {
            out.push(d.entry);
        } else {
            out.extend_from_slice(sub);
        }
    }
    out
}

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
        for d in GENERATORS {
            assert!(entries.contains(&d.entry), "missing entry point {}", d.entry);
        }
        for e in all_effect_entries() {
            assert!(entries.contains(&e), "missing entry point {e}");
        }
        for d in EFFECTS {
            assert!(d.params.len() <= MAX_PARAMS);
        }
    }
}
