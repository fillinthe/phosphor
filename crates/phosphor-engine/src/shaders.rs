//! Built-in effect and generator catalog. Each entry maps to a WGSL fragment entry point
//! in `shaders/effects.wgsl` or `shaders/generators.wgsl`; parameters land in `u.p[..]`.

/// Floats a shader can take: 11 of the 12 uniform vec4s (the last is reserved).
pub const MAX_PARAMS: usize = 60;

#[derive(Clone, Copy, Debug)]
pub struct Param {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    /// A menu: the name of each whole number from `min` up (ISF "long" inputs, the LUT mixer's LUTs). Empty = a slider.
    pub labels: &'static [&'static str],
}

const fn p(name: &'static str, min: f32, max: f32, default: f32) -> Param {
    Param { name, min, max, default, labels: &[] }
}

impl Param {
    /// A plain slider parameter.
    pub const fn new(name: &'static str, min: f32, max: f32, default: f32) -> Param {
        p(name, min, max, default)
    }

    /// The menu entry a value picks (None for sliders).
    pub fn label_of(&self, v: f32) -> Option<&'static str> {
        let i = (v - self.min).round();
        (i >= 0.0).then(|| self.labels.get(i as usize).copied()).flatten()
    }
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

/// A shader's point settings (ISF point2D: a "Name X" slider followed by its "Name Y"), as (x index, y index).
pub fn point_pairs(params: &[Param]) -> Vec<(usize, usize)> {
    params
        .windows(2)
        .enumerate()
        .filter_map(|(i, w)| {
            let (x, y) = (w[0].name.strip_suffix(" X")?, w[1].name.strip_suffix(" Y")?);
            (x == y && w[0].labels.is_empty()).then_some((i, i + 1))
        })
        .collect()
}

/// Puts every point setting of `params` at `uv` (0..1 across the picture, top-left origin; shaders count Y upward).
pub fn set_points(params: &[Param], values: &mut Vec<f32>, uv: [f32; 2]) -> bool {
    let pairs = point_pairs(params);
    while values.len() < params.len() {
        values.push(params[values.len()].default);
    }
    for &(x, y) in &pairs {
        let (px, py) = (&params[x], &params[y]);
        values[x] = px.min + uv[0].clamp(0.0, 1.0) * (px.max - px.min);
        values[y] = py.min + (1.0 - uv[1].clamp(0.0, 1.0)) * (py.max - py.min);
    }
    !pairs.is_empty()
}

/// Holds (or lets go of) every "Mouse down" setting of `params` (Shadertoy's iMouse button).
pub fn set_mouse_down(params: &[Param], values: &mut Vec<f32>, down: bool) {
    while values.len() < params.len() {
        values.push(params[values.len()].default);
    }
    for (i, p) in params.iter().enumerate().filter(|(_, p)| p.name == "Mouse down") {
        values[i] = if down { p.max } else { p.min };
    }
}

#[cfg(test)]
mod point_tests {
    use super::*;

    #[test]
    fn points_follow_the_mouse() {
        let params = [p("Amount", 0.0, 1.0, 0.5), p("Centre X", 0.0, 1.0, 0.5), p("Centre Y", 0.0, 1.0, 0.5), p("Size X", 0.0, 2.0, 1.0)];
        assert_eq!(point_pairs(&params), vec![(1, 2)]);
        let mut v = vec![0.3];
        assert!(set_points(&params, &mut v, [0.25, 0.25]));
        assert_eq!(v, vec![0.3, 0.25, 0.75, 1.0], "Y counts up; the rest keep their defaults");
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
    ShaderDesc {
        id: "color_match",
        name: "Colour Match",
        entry: "fx_color_match",
        params: &[p("Gain R", 0.0, 4.0, 1.0), p("Gain G", 0.0, 4.0, 1.0), p("Gain B", 0.0, 4.0, 1.0), p("Offset R", -1.0, 1.0, 0.0), p("Offset G", -1.0, 1.0, 0.0), p("Offset B", -1.0, 1.0, 0.0)],
    },
    ShaderDesc {
        id: "palette",
        name: "Palette",
        entry: "fx_palette",
        params: &[
            p("Colour 1 R", 0.0, 1.0, 0.05), p("Colour 1 G", 0.0, 1.0, 0.05), p("Colour 1 B", 0.0, 1.0, 0.1),
            p("Colour 2 R", 0.0, 1.0, 0.35), p("Colour 2 G", 0.0, 1.0, 0.1), p("Colour 2 B", 0.0, 1.0, 0.45),
            p("Colour 3 R", 0.0, 1.0, 0.9), p("Colour 3 G", 0.0, 1.0, 0.3), p("Colour 3 B", 0.0, 1.0, 0.4),
            p("Colour 4 R", 0.0, 1.0, 1.0), p("Colour 4 G", 0.0, 1.0, 0.75), p("Colour 4 B", 0.0, 1.0, 0.3),
            p("Colour 5 R", 0.0, 1.0, 0.4), p("Colour 5 G", 0.0, 1.0, 0.9), p("Colour 5 B", 0.0, 1.0, 0.8),
            p("Colour 6 R", 0.0, 1.0, 0.98), p("Colour 6 G", 0.0, 1.0, 0.97), p("Colour 6 B", 0.0, 1.0, 0.9),
            p("Colours", 2.0, 6.0, 6.0),
            p("Amount", 0.0, 1.0, 1.0),
        ],
    },
    ShaderDesc { id: "pixelate", name: "Pixelate", entry: "fx_pixelate", params: &[p("Cell Size", 1.0, 128.0, 16.0)] },
    ShaderDesc {
        id: "hide_face",
        name: "Hide Face",
        entry: "fx_hide_face",
        params: &[p("X", 0.0, 1.0, 0.5), p("Y", 0.0, 1.0, 0.5), p("Size", 0.0, 1.0, 0.3), p("Found", 0.0, 1.0, 1.0), p("Style", 0.0, 2.0, 0.0), p("Cells", 4.0, 64.0, 24.0)],
    },
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
            p("Lighting", 0.0, 1.0, 0.0),
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
            p("Lighting", 0.0, 1.0, 0.0),
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
        id: "flow",
        name: "Motion Flow",
        entry: "fx_flow",
        params: &[p("Amount", 0.0, 4.0, 1.0), p("Scale (px)", 1.0, 24.0, 4.0), p("Mode", 0.0, 2.0, 0.0)],
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
        id: "remove_bg",
        name: "Remove Background",
        entry: "fx_cutout",
        params: &[p("Edge", 0.0, 1.0, 0.5), p("Softness", 0.0, 0.5, 0.15), p("Invert", 0.0, 1.0, 0.0)],
    },
    ShaderDesc {
        id: "blur_bg",
        name: "Blur Background",
        entry: "fx_cutout_blur",
        params: &[p("Blur", 0.0, 1.0, 0.5), p("Edge", 0.0, 1.0, 0.5), p("Softness", 0.0, 0.5, 0.15), p("Invert", 0.0, 1.0, 0.0)],
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
        id: "spectrum",
        name: "Spectrum Bars",
        entry: "gen_spectrum",
        params: &[p("Bars", 4.0, 256.0, 48.0), p("Height", 0.1, 3.0, 1.2), p("Hue", 0.0, 1.0, 0.6), p("Mirror", 0.0, 1.0, 0.0)],
    },
    ShaderDesc {
        id: "waveform",
        name: "Waveform",
        entry: "gen_waveform",
        params: &[p("Thickness", 0.002, 0.1, 0.015), p("Gain", 0.1, 4.0, 1.0), p("Hue", 0.0, 1.0, 0.3), p("Glow", 0.0, 1.0, 0.6)],
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
        id: "particles",
        name: "Particles",
        entry: "gen_particles",
        params: &[p("Count", 10.0, 300.0, 120.0), p("Speed", 0.1, 3.0, 1.0), p("Gravity", -2.0, 2.0, 0.6), p("Spread", 0.0, 1.0, 0.35), p("Size", 0.002, 0.05, 0.012), p("Emitter X", 0.0, 1.0, 0.5), p("Emitter Y", 0.0, 1.0, 0.15)],
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
        // Keep this frame's picture for the next one, then displace by the motion between the two.
        "fx_flow" => &["fx_flow_keep", "fx_flow"],
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

static CUSTOM: std::sync::RwLock<Vec<&'static ShaderDesc>> = std::sync::RwLock::new(Vec::new());

/// How many buffer textures a multi-pass shader can use (bindings 6..).
pub const MAX_BUFFERS: usize = 12;

/// A shader that runs several times per frame (ISF `PASSES`): each pass renders into one of the
/// shader's buffers or, without a target, into its output; every pass can read every buffer
/// (`buf0`…). Persistent buffers keep their picture between frames (feedback).
#[derive(Clone, Debug, Default)]
pub struct PassPlan {
    pub passes: Vec<PlanPass>,
    /// Number of buffers (at most `MAX_BUFFERS`).
    pub buffers: usize,
    /// Parameter names usable in size expressions → their index in the parameter list.
    pub vars: Vec<(String, usize)>,
    /// Buffer slots that are pictures the app provides (`Engine::set_plan_image`), not render targets.
    pub images: Vec<usize>,
    /// Buffer slots that show another layer's picture: (buffer, the parameter that picks the layer by number,
    /// 1 = the first layer, 0 = none). See `is_layer_pick`.
    pub layer_images: Vec<(usize, usize)>,
}

/// A setting that picks a layer by number (0 = none / this one): its name ends in "layer #". The app shows a list
/// of layer names for it.
pub fn is_layer_pick(p: &Param) -> bool {
    p.labels.is_empty() && p.name.ends_with("layer #")
}

#[derive(Clone, Debug, Default)]
pub struct PlanPass {
    /// Buffer index this pass writes, or `None` for the output.
    pub target: Option<usize>,
    /// The buffer it writes is 16-bit float (ISF `FLOAT: true`): feedback and accumulation keep their precision.
    pub float: bool,
    /// Size expressions (`$WIDTH`, `$HEIGHT`, parameter names); `None` = full size.
    pub width: Option<String>,
    pub height: Option<String>,
}

static PLANS: std::sync::RwLock<Vec<(&'static str, std::sync::Arc<PassPlan>)>> = std::sync::RwLock::new(Vec::new());

/// Registers the pass plan for a (custom/ISF) entry point.
pub fn set_pass_plan(entry: &'static str, plan: PassPlan) {
    if let Ok(mut p) = PLANS.write() {
        p.retain(|(e, _)| *e != entry);
        p.push((entry, std::sync::Arc::new(plan)));
    }
}

/// What an audio picture slot carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioTexture {
    /// One row of samples, 0.5 = silence (ISF `audio`).
    Wave,
    /// One row of magnitudes 0..1, low to high (ISF `audioFFT`).
    Fft,
}

static AUDIO_SLOTS: std::sync::RwLock<Vec<(&'static str, std::sync::Arc<Vec<(usize, AudioTexture)>>)>> = std::sync::RwLock::new(Vec::new());

/// Which of a shader's texture slots (1 or 2) take the live audio pictures.
pub fn set_audio_slots(entry: &'static str, slots: Vec<(usize, AudioTexture)>) {
    if let Ok(mut a) = AUDIO_SLOTS.write() {
        a.retain(|(e, _)| *e != entry);
        if !slots.is_empty() {
            a.push((entry, std::sync::Arc::new(slots)));
        }
    }
}

pub fn audio_slots(entry: &str) -> Option<std::sync::Arc<Vec<(usize, AudioTexture)>>> {
    AUDIO_SLOTS.read().ok()?.iter().find(|(e, _)| *e == entry).map(|(_, s)| s.clone())
}

static VERTEX_ENTRIES: std::sync::RwLock<Vec<(&'static str, &'static str)>> = std::sync::RwLock::new(Vec::new());

/// A shader with its own vertex stage (an ISF vertex shader that moves the quad): drawn as six vertices through
/// `vs_entry` instead of the fullscreen triangle.
pub fn set_vertex_entry(entry: &'static str, vs_entry: &'static str) {
    if let Ok(mut v) = VERTEX_ENTRIES.write() {
        v.retain(|(e, _)| *e != entry);
        v.push((entry, vs_entry));
    }
}

pub fn vertex_entry(entry: &str) -> Option<&'static str> {
    VERTEX_ENTRIES.read().ok()?.iter().find(|(e, _)| *e == entry).map(|(_, v)| *v)
}

/// How many copies an instanced shader draws: a fixed number, or the value of one of its parameters (float offset).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Instances {
    Fixed(u32),
    Param(usize),
}

impl Instances {
    /// The count for a pass with these parameter values (at least 1, at most 4096).
    pub fn count(self, p: &[[f32; 4]; 16]) -> u32 {
        let n = match self {
            Instances::Fixed(n) => n as f32,
            Instances::Param(i) => p.get(i / 4).map_or(1.0, |v| v[i % 4]),
        };
        (n.round() as u32).clamp(1, 4096)
    }
}

static INSTANCES: std::sync::RwLock<Vec<(&'static str, Instances)>> = std::sync::RwLock::new(Vec::new());

/// Shader `entry` draws its quad several times (a vertex stage that places each copy by its instance number).
pub fn set_instances(entry: &'static str, instances: Instances) {
    if let Ok(mut v) = INSTANCES.write() {
        v.retain(|(e, _)| *e != entry);
        v.push((entry, instances));
    }
}

pub fn instances(entry: &str) -> Option<Instances> {
    INSTANCES.read().ok()?.iter().find(|(e, _)| *e == entry).map(|(_, i)| *i)
}

/// Any shader at all wants audio pictures (the app refreshes them each frame only then).
pub fn any_audio_slots() -> bool {
    AUDIO_SLOTS.read().is_ok_and(|a| !a.is_empty())
}

pub fn pass_plan(entry: &str) -> Option<std::sync::Arc<PassPlan>> {
    PLANS.read().ok()?.iter().find(|(e, _)| *e == entry).map(|(_, p)| p.clone())
}

/// Prefix for user effects/generators made in the Shader Editor.
pub const CUSTOM_EFFECT: &str = "custom:";
pub const CUSTOM_GENERATOR: &str = "customgen:";
/// Prefix for imported ISF shaders.
pub const ISF_EFFECT: &str = "isf:";
pub const ISF_GENERATOR: &str = "isfgen:";
/// Imported ISF transitions (old picture → new picture as `progress` goes 0 → 1): clip transition styles.
pub const ISF_TRANSITION: &str = "isftr:";
/// FFGL plugins (Mac): effects and sources drawn by the app (`ext:` entries, see `is_external`).
pub const FFGL_EFFECT: &str = "ffgl:";
pub const FFGL_GENERATOR: &str = "ffglgen:";
/// FFGL mixer plugins: blend a layer onto what's below (`LayerState::mixer`).
pub const FFGL_MIXER: &str = "ffglmix:";
/// User blend modes (`// @kind blend`): drawn natively, `src(uv)` = the layer, `below(uv)` = the stack under it.
pub const CUSTOM_BLEND: &str = "customblend:";

/// An effect / generator drawn outside the engine (its entry starts with `ext:`; the engine's `external` module).
pub fn is_external(entry: &str) -> bool {
    entry.starts_with("ext:")
}

static CUSTOM_INFO: std::sync::RwLock<Vec<(&'static str, &'static str, &'static str)>> = std::sync::RwLock::new(Vec::new());

/// Category and description for a registered (user or imported) shader.
pub fn set_custom_info(id: &'static str, category: &'static str, about: &'static str) {
    let mut list = CUSTOM_INFO.write().unwrap();
    list.retain(|(i, _, _)| *i != id);
    list.push((id, category, about));
}

fn is_custom_effect(id: &str) -> bool {
    id.starts_with(CUSTOM_EFFECT) || id.starts_with(ISF_EFFECT) || id.starts_with(FFGL_EFFECT)
}

fn is_custom_generator(id: &str) -> bool {
    id.starts_with(CUSTOM_GENERATOR) || id.starts_with(ISF_GENERATOR) || id.starts_with(FFGL_GENERATOR)
}

/// Adds or replaces a user shader (by id).
pub fn register_custom(desc: &'static ShaderDesc) {
    let mut list = CUSTOM.write().unwrap();
    list.retain(|d| d.id != desc.id);
    list.push(desc);
}

pub fn unregister_custom(id: &str) {
    CUSTOM.write().unwrap().retain(|d| d.id != id);
}

fn custom(id: &str) -> Option<&'static ShaderDesc> {
    CUSTOM.read().unwrap().iter().find(|d| d.id == id).copied()
}

/// Built-in effects followed by the user's.
pub fn all_effects() -> Vec<&'static ShaderDesc> {
    let user = CUSTOM.read().unwrap();
    EFFECTS.iter().chain(user.iter().copied().filter(|d| is_custom_effect(d.id))).collect()
}

/// Shader transitions (Fade style › Shader transition), by name.
pub fn all_transitions() -> Vec<&'static ShaderDesc> {
    let mut list: Vec<_> = CUSTOM.read().unwrap().iter().copied().filter(|d| d.id.starts_with(ISF_TRANSITION)).collect();
    list.sort_by_key(|d| d.name.to_lowercase());
    list
}

pub fn transition(id: &str) -> Option<&'static ShaderDesc> {
    if id.starts_with(ISF_TRANSITION) { custom(id) } else { None }
}

/// The parameter a transition's progress goes to.
pub fn progress_index(desc: &ShaderDesc) -> Option<usize> {
    desc.params.iter().position(|p| p.name.eq_ignore_ascii_case("progress"))
}

pub fn all_generators() -> Vec<&'static ShaderDesc> {
    let user = CUSTOM.read().unwrap();
    GENERATORS.iter().chain(user.iter().copied().filter(|d| is_custom_generator(d.id))).collect()
}

/// Explorer category and a one-line description.
pub fn info(id: &str) -> (&'static str, &'static str) {
    if let Some((_, cat, about)) = CUSTOM_INFO.read().unwrap().iter().find(|(i, _, _)| *i == id) {
        return (cat, about);
    }
    if id.starts_with(CUSTOM_EFFECT) || id.starts_with(CUSTOM_GENERATOR) {
        return ("Yours", "Made in the Shader Editor");
    }
    if id.starts_with(ISF_EFFECT) || id.starts_with(ISF_GENERATOR) {
        return ("ISF", "Imported ISF shader");
    }
    if id.starts_with(FFGL_EFFECT) || id.starts_with(FFGL_GENERATOR) {
        return ("FFGL", "FFGL plugin");
    }
    match id {
        "invert" => ("Color", "Negative image"),
        "color_adjust" => ("Color", "Hue, saturation, brightness, contrast"),
        "gamma" => ("Color", "Gamma, lift and gain"),
        "posterize" => ("Color", "Fewer colour levels"),
        "threshold" => ("Color", "Hard black and white"),
        "solarize" => ("Color", "Flip tones above a threshold"),
        "technicolor" => ("Film", "Two-strip film colour"),
        "bleach" => ("Film", "Desaturated, punchy contrast"),
        "vignette" => ("Film", "Darkened corners"),
        "grain" => ("Film", "Film grain and flicker"),
        "blur" => ("Blur", "Soft Gaussian blur"),
        "zoom_blur" => ("Blur", "Radial streaks from a point"),
        "motion_blur" => ("Blur", "Streaks in one direction"),
        "glow" => ("Light", "Bloom on bright areas"),
        "edge_glow" => ("Light", "Glowing outlines"),
        "rgb_shift" => ("Distort", "Split colour channels"),
        "kaleidoscope" => ("Distort", "Mirrored segments"),
        "mirror" => ("Distort", "Mirror halves or quarters"),
        "zoom_rotate" => ("Distort", "Zoom and spin"),
        "wave" => ("Distort", "Wavy warp"),
        "tile" => ("Distort", "Repeat the image"),
        "pixelate" => ("Stylize", "Big pixels"),
        "edges" => ("Stylize", "Edge detection"),
        "halftone" => ("Stylize", "Printed dots"),
        "emboss" => ("Stylize", "Raised relief"),
        "sharpen" => ("Stylize", "Crisper detail"),
        "crt" => ("Retro", "Scanlines and tube curve"),
        "glitch" => ("Retro", "Analog tracking glitches"),
        "strobe" => ("Time", "Flash on and off"),
        "trails" => ("Time", "Feedback trails and tunnels"),
        "slitscan" => ("Time", "Time smeared across the frame"),
        "rutt_etra" => ("3D", "Scan-processor lines (v002)"),
        "rutt_etra_points" => ("3D", "Scan-processor points (v002)"),
        "chroma_key" => ("Key", "Make a colour transparent"),
        "remove_bg" => ("Key", "Keep the people, lose the background (Mac)"),
        "blur_bg" => ("Key", "People sharp, the background blurred (or the other way round) (Mac)"),
        "plasma" | "gradient" | "noise" | "voronoi" => ("Patterns", "Animated pattern"),
        "rings" | "spiral" | "tunnel" => ("Patterns", "Hypnotic motion"),
        "checker" | "color_bars" | "solid" => ("Utility", "Test and fill"),
        "palette" => ("Colour", "Every pixel becomes the nearest of a few colours (Palette from this picture: the layer menu picks them)"),
        "color_match" => ("Colour", "Per-colour gain and offset: Match colour (layer menu) sets it so the layer looks like a reference picture"),
        "hide_face" => ("Tracking", "Pixelates, blurs or blacks out a face: with face tracking on, it follows the face (Modulators › Video tracking › Hide faces)"),
        "starfield" | "lines" => ("Motion", "Moving lines and points"),
        "particles" => ("Motion", "A fountain of glowing particles from one point (map the emitter to a hand or the mouse)"),
        _ => ("Other", ""),
    }
}

pub fn effect(id: &str) -> Option<&'static ShaderDesc> {
    EFFECTS.iter().find(|d| d.id == id).or_else(|| if is_custom_effect(id) { custom(id) } else { None })
}

/// A mixer plugin (blends a layer onto the stack below).
pub fn mixer(id: &str) -> Option<&'static ShaderDesc> {
    if id.starts_with(FFGL_MIXER) || id.starts_with(CUSTOM_BLEND) { custom(id) } else { None }
}

/// Every mixer plugin, by name.
pub fn all_mixers() -> Vec<&'static ShaderDesc> {
    let mut list: Vec<_> = CUSTOM.read().unwrap().iter().copied().filter(|d| d.id.starts_with(FFGL_MIXER) || d.id.starts_with(CUSTOM_BLEND)).collect();
    list.sort_by_key(|d| d.name.to_lowercase());
    list
}

pub fn generator(id: &str) -> Option<&'static ShaderDesc> {
    GENERATORS.iter().find(|d| d.id == id).or_else(|| if is_custom_generator(id) { custom(id) } else { None })
}

/// Shared declarations and helpers (bindings, `VOut`, `src`, `palette`, `hash21`, …): the prelude
/// for user and imported shaders, which are compiled as separate small modules.
pub fn common_source() -> &'static str {
    include_str!("shaders/common.wgsl")
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
