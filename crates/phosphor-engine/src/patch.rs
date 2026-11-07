//! Patches: effects and generators made by wiring blocks together, no code. A patch compiles to an ordinary user
//! shader (`custom`), so it runs, saves, exports and maps like any other: its blocks' settings become the shader's
//! parameters (the first 60; MIDI, modulation and the Inspector work on them), and the patch itself rides along in
//! the source as one `// @patch {…}` line, so opening the file again gives the blocks back.
//!
//! Each block becomes a function of the picture position (`fn n3(uv: vec2f) -> vec4f`) that calls the blocks wired
//! into it, so warps just move where their input is read. Blocks that look around a point (blur, glow, edges) need
//! their input drawn first: it gets its own pass (`@pass`), and so do their in-between steps. "Last frame" turns
//! on `@feedback`.

use serde::{Deserialize, Serialize};

/// What a block does, for the add menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Source,
    Colour,
    Warp,
    Blur,
    Mix,
    /// Blocks that make a number (0..1) to wire into any setting: LFO, a ramp across the picture, a picture's brightness…
    Number,
    Output,
}

impl Group {
    pub const ALL: [Group; 7] = [Group::Source, Group::Colour, Group::Warp, Group::Blur, Group::Mix, Group::Number, Group::Output];

    pub fn name(self) -> &'static str {
        match self {
            Group::Source => "Pictures",
            Group::Colour => "Colour",
            Group::Warp => "Warp",
            Group::Blur => "Blur & edges",
            Group::Mix => "Mix two",
            Group::Number => "Numbers",
            Group::Output => "Output",
        }
    }
}

/// One setting of a block: becomes a shader parameter.
#[derive(Clone, Copy, Debug)]
pub struct Setting {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

const fn set(name: &'static str, min: f32, max: f32, default: f32) -> Setting {
    Setting { name, min, max, default }
}

/// A kind of block. `code` is the body of `fn(uv: vec2f) -> vec4f`: `$a(p)` / `$b(p)` / `$c(p)` read inputs 1–3 at
/// `p`, `$0`… are the settings. With `pass_input`, `$A(p)` reads input 1 drawn into its own pass first; each of
/// `stages` (body, size) is one more pass in between, read by the next with `$S(p)`.
pub struct Block {
    pub id: &'static str,
    pub name: &'static str,
    pub group: Group,
    pub about: &'static str,
    pub inputs: &'static [&'static str],
    pub settings: &'static [Setting],
    pub pass_input: bool,
    pub stages: &'static [(&'static str, f32)],
    /// Makes a number (`fn(uv) -> f32`, 0..1) instead of a picture: wire it into any setting of any block (the
    /// setting then follows it, per pixel and per frame, and takes no parameter).
    pub number: bool,
    pub code: &'static str,
}

const PIC: &[&str] = &["Picture"];
const TWO: &[&str] = &["A", "B"];

pub const BLOCKS: &[Block] = &[
    // Pictures
    Block {
        id: "input",
        name: "Input",
        group: Group::Source,
        about: "The layer's own picture (effects)",
        inputs: &[],
        settings: &[],
        pass_input: false,
        stages: &[],
        number: false,
        code: "return src(uv);",
    },
    Block {
        id: "last",
        name: "Last frame",
        group: Group::Source,
        about: "This effect's own picture from the frame before: mix it back in for trails and echoes",
        inputs: &[],
        settings: &[set("Zoom", 0.8, 1.25, 1.01), set("Turn", -0.1, 0.1, 0.0), set("Fade", 0.0, 1.0, 0.9)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "var p = (uv - 0.5) * vec2f(aspect(), 1.0) / $0;
    let r = $1;
    p = vec2f(p.x * cos(r) - p.y * sin(r), p.x * sin(r) + p.y * cos(r));
    return last(p / vec2f(aspect(), 1.0) + 0.5) * $2;",
    },
    Block {
        id: "colour",
        name: "Colour",
        group: Group::Source,
        about: "One flat colour",
        inputs: &[],
        settings: &[set("Hue", 0.0, 1.0, 0.6), set("Saturation", 0.0, 1.0, 0.8), set("Brightness", 0.0, 1.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "return vec4f(hsv2rgb(vec3f($0, $1, $2)), 1.0);",
    },
    Block {
        id: "gradient",
        name: "Gradient",
        group: Group::Source,
        about: "Two colours blending across the picture, optionally sliding",
        inputs: &[],
        settings: &[set("Angle", 0.0, 1.0, 0.0), set("Hue A", 0.0, 1.0, 0.6), set("Hue B", 0.0, 1.0, 0.9), set("Speed", -1.0, 1.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let a = $0 * TAU;
    let t = dot(uv - 0.5, vec2f(cos(a), sin(a))) + 0.5 + u.time * $3;
    let k = 0.5 - 0.5 * cos(fract(t) * TAU);
    return vec4f(mix(hsv2rgb(vec3f($1, 0.8, 1.0)), hsv2rgb(vec3f($2, 0.8, 1.0)), k), 1.0);",
    },
    Block {
        id: "noise",
        name: "Noise",
        group: Group::Source,
        about: "Soft drifting clouds (grey: colour them with Palette or Tint)",
        inputs: &[],
        settings: &[set("Scale", 0.5, 20.0, 4.0), set("Speed", 0.0, 2.0, 0.3), set("Detail", 1.0, 6.0, 3.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "var p = centered(uv) * $0;
    let t = u.time * $1;
    var v = 0.0;
    var amp = 0.5;
    for (var i = 0; i < i32($2); i++) {
        v += value_noise(p + vec2f(t, t * 0.7)) * amp;
        p *= 2.0;
        amp *= 0.5;
    }
    return vec4f(vec3f(v), 1.0);",
    },
    Block {
        id: "rings",
        name: "Rings",
        group: Group::Source,
        about: "Rings moving out from the centre",
        inputs: &[],
        settings: &[set("Count", 1.0, 40.0, 8.0), set("Speed", -4.0, 4.0, 1.0), set("Sharpness", 0.0, 1.0, 0.5)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let d = length(centered(uv));
    let w = 0.5 + 0.5 * sin(d * $0 * TAU - u.time * $1 * 3.0);
    let s = mix(0.5, 0.02, $2);
    return vec4f(vec3f(smoothstep(0.5 - s, 0.5 + s, w)), 1.0);",
    },
    Block {
        id: "stripes",
        name: "Stripes",
        group: Group::Source,
        about: "Parallel stripes at any angle, optionally moving",
        inputs: &[],
        settings: &[set("Count", 1.0, 60.0, 10.0), set("Angle", 0.0, 1.0, 0.0), set("Speed", -4.0, 4.0, 0.5)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let a = $1 * TAU;
    let t = dot(centered(uv), vec2f(cos(a), sin(a))) * $0 - u.time * $2;
    return vec4f(vec3f(step(0.5, fract(t))), 1.0);",
    },
    Block {
        id: "circle",
        name: "Circle",
        group: Group::Source,
        about: "A white disc on black (use it as a mask)",
        inputs: &[],
        settings: &[set("Size", 0.0, 1.0, 0.3), set("Softness", 0.0, 1.0, 0.05), set("X", 0.0, 1.0, 0.5), set("Y", 0.0, 1.0, 0.5)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let d = length((uv - vec2f($2, $3)) * vec2f(aspect(), 1.0));
    return vec4f(vec3f(1.0 - smoothstep($0, $0 + $1 + 0.001, d)), 1.0);",
    },
    Block {
        id: "checker",
        name: "Checker",
        group: Group::Source,
        about: "A black and white checkerboard",
        inputs: &[],
        settings: &[set("Squares", 2.0, 64.0, 8.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let g = floor(centered(uv) * $0);
    return vec4f(vec3f(abs(g.x + g.y) % 2.0), 1.0);",
    },
    // Colour
    Block {
        id: "adjust",
        name: "Adjust",
        group: Group::Colour,
        about: "Brightness, contrast and saturation",
        inputs: PIC,
        settings: &[set("Brightness", -1.0, 1.0, 0.0), set("Contrast", 0.0, 3.0, 1.0), set("Saturation", 0.0, 3.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    var rgb = (c.rgb - 0.5) * $1 + 0.5 + $0;
    rgb = mix(vec3f(luma(rgb)), rgb, $2);
    return vec4f(max(rgb, vec3f(0.0)), c.a);",
    },
    Block {
        id: "hue",
        name: "Hue shift",
        group: Group::Colour,
        about: "Turns every colour round the colour wheel (Spin keeps turning)",
        inputs: PIC,
        settings: &[set("Shift", 0.0, 1.0, 0.0), set("Spin", -2.0, 2.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    var h = rgb2hsv(c.rgb);
    h.x = fract(h.x + $0 + u.time * $1 * 0.1);
    return vec4f(hsv2rgb(h), c.a);",
    },
    Block {
        id: "invert",
        name: "Invert",
        group: Group::Colour,
        about: "A negative",
        inputs: PIC,
        settings: &[set("Amount", 0.0, 1.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    return vec4f(mix(c.rgb, 1.0 - c.rgb, $0), c.a);",
    },
    Block {
        id: "threshold",
        name: "Threshold",
        group: Group::Colour,
        about: "Black and white: bright parts white, the rest black",
        inputs: PIC,
        settings: &[set("Level", 0.0, 1.0, 0.5), set("Softness", 0.0, 0.5, 0.02)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    return vec4f(vec3f(smoothstep($0 - $1, $0 + $1 + 0.0001, luma(c.rgb))), c.a);",
    },
    Block {
        id: "posterize",
        name: "Posterize",
        group: Group::Colour,
        about: "Fewer colour steps, like a print",
        inputs: PIC,
        settings: &[set("Steps", 2.0, 16.0, 4.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    let n = max(floor($0), 2.0);
    return vec4f(min(floor(c.rgb * n), vec3f(n - 1.0)) / (n - 1.0), c.a);",
    },
    Block {
        id: "tint",
        name: "Tint",
        group: Group::Colour,
        about: "Everything in one colour, keeping light and dark",
        inputs: PIC,
        settings: &[set("Hue", 0.0, 1.0, 0.6), set("Amount", 0.0, 1.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    return vec4f(mix(c.rgb, luma(c.rgb) * hsv2rgb(vec3f($0, 0.8, 1.0)) * 1.4, $1), c.a);",
    },
    Block {
        id: "palette",
        name: "Palette",
        group: Group::Colour,
        about: "Paints light and dark with a rainbow of colours",
        inputs: PIC,
        settings: &[set("Shift", 0.0, 1.0, 0.0), set("Spread", 0.0, 3.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    return vec4f(palette(luma(c.rgb) * $1 + $0), c.a);",
    },
    Block {
        id: "fade",
        name: "Fade",
        group: Group::Colour,
        about: "Darker and more see-through",
        inputs: PIC,
        settings: &[set("Amount", 0.0, 1.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "return $a(uv) * $0;",
    },
    // Warp
    Block {
        id: "transform",
        name: "Move & zoom",
        group: Group::Warp,
        about: "Moves, zooms and turns the picture",
        inputs: PIC,
        settings: &[set("Zoom", 0.1, 4.0, 1.0), set("X", -1.0, 1.0, 0.0), set("Y", -1.0, 1.0, 0.0), set("Turn", -1.0, 1.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "var p = (uv - 0.5 - vec2f($1, $2)) * vec2f(aspect(), 1.0) / $0;
    let r = $3 * TAU;
    p = vec2f(p.x * cos(r) - p.y * sin(r), p.x * sin(r) + p.y * cos(r));
    return $a(p / vec2f(aspect(), 1.0) + 0.5);",
    },
    Block {
        id: "mirror",
        name: "Mirror",
        group: Group::Warp,
        about: "Folds the picture: left onto right, top onto bottom (above half = on)",
        inputs: PIC,
        settings: &[set("Left-right", 0.0, 1.0, 1.0), set("Top-bottom", 0.0, 1.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "var p = uv;
    if $0 > 0.5 {
        p.x = 0.5 - abs(p.x - 0.5);
    }
    if $1 > 0.5 {
        p.y = 0.5 - abs(p.y - 0.5);
    }
    return $a(p);",
    },
    Block {
        id: "kaleido",
        name: "Kaleidoscope",
        group: Group::Warp,
        about: "Repeats a slice of the picture round the centre",
        inputs: PIC,
        settings: &[set("Segments", 2.0, 16.0, 6.0), set("Turn", 0.0, 1.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let p = centered(uv);
    let seg = TAU / max(floor($0), 1.0);
    var a = atan2(p.y, p.x) + $1 * TAU;
    a = abs(a - seg * floor(a / seg) - seg * 0.5);
    let q = vec2f(cos(a), sin(a)) * length(p);
    return $a(q / vec2f(aspect(), 1.0) + 0.5);",
    },
    Block {
        id: "ripple",
        name: "Ripple",
        group: Group::Warp,
        about: "Waves running out from the centre, like a stone in water",
        inputs: PIC,
        settings: &[set("Amount", 0.0, 0.1, 0.02), set("Waves", 1.0, 40.0, 10.0), set("Speed", -4.0, 4.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let p = centered(uv);
    let off = normalize(p + 0.00001) * sin(length(p) * $1 * TAU - u.time * $2 * 3.0) * $0;
    return $a(uv + off / vec2f(aspect(), 1.0));",
    },
    Block {
        id: "wave",
        name: "Wave",
        group: Group::Warp,
        about: "Bends the picture side to side",
        inputs: PIC,
        settings: &[set("Amount", 0.0, 0.2, 0.03), set("Waves", 0.0, 20.0, 4.0), set("Speed", -4.0, 4.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "return $a(uv + vec2f(sin(uv.y * $1 * TAU + u.time * $2 * 3.0) * $0, 0.0));",
    },
    Block {
        id: "swirl",
        name: "Swirl",
        group: Group::Warp,
        about: "Twists the middle of the picture",
        inputs: PIC,
        settings: &[set("Amount", -4.0, 4.0, 1.5), set("Radius", 0.05, 1.0, 0.5)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let p = centered(uv);
    let k = max(0.0, 1.0 - length(p) / $1);
    let r = $0 * k * k;
    let q = vec2f(p.x * cos(r) - p.y * sin(r), p.x * sin(r) + p.y * cos(r));
    return $a(q / vec2f(aspect(), 1.0) + 0.5);",
    },
    Block {
        id: "pixelate",
        name: "Pixelate",
        group: Group::Warp,
        about: "Big square pixels",
        inputs: PIC,
        settings: &[set("Size", 1.0, 100.0, 20.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let n = u.resolution / max($0, 1.0);
    return $a((floor(uv * n) + 0.5) / n);",
    },
    Block {
        id: "tile",
        name: "Tile",
        group: Group::Warp,
        about: "The picture repeated in a grid",
        inputs: PIC,
        settings: &[set("Count", 1.0, 8.0, 2.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "return $a(fract(uv * floor(max($0, 1.0))));",
    },
    Block {
        id: "displace",
        name: "Displace",
        group: Group::Warp,
        about: "Pushes picture A around by the colours of picture B",
        inputs: &["Picture", "Map"],
        settings: &[set("Amount", -0.3, 0.3, 0.05)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let m = $b(uv);
    return $a(uv + (m.rg - 0.5) * $0);",
    },
    // Blur & edges: these read around each point, so their input is drawn first.
    Block {
        id: "blur",
        name: "Blur",
        group: Group::Blur,
        about: "Soft focus",
        inputs: PIC,
        settings: &[set("Size", 0.0, 10.0, 2.0)],
        pass_input: true,
        stages: &[(
            "let s = vec2f($0 / u.resolution.x, 0.0);
    var sum = vec4f(0.0);
    for (var i = -4; i <= 4; i++) {
        sum += $A(uv + s * f32(i));
    }
    return sum / 9.0;",
            1.0,
        )],
        number: false,
        code: "let s = vec2f(0.0, $0 / u.resolution.y);
    var sum = vec4f(0.0);
    for (var i = -4; i <= 4; i++) {
        sum += $S(uv + s * f32(i));
    }
    return sum / 9.0;",
    },
    Block {
        id: "glow",
        name: "Glow",
        group: Group::Blur,
        about: "The bright parts glow",
        inputs: PIC,
        settings: &[set("Threshold", 0.0, 1.0, 0.6), set("Size", 0.0, 10.0, 3.0), set("Amount", 0.0, 3.0, 1.2)],
        pass_input: true,
        stages: &[(
            "let s = vec2f($1 / u.resolution.x, 0.0);
    var sum = vec4f(0.0);
    for (var i = -4; i <= 4; i++) {
        let c = $A(uv + s * f32(i));
        sum += c * smoothstep($0, $0 + 0.2, luma(c.rgb));
    }
    return sum / 9.0;",
            0.5,
        )],
        number: false,
        code: "let s = vec2f(0.0, $1 / u.resolution.y);
    var sum = vec4f(0.0);
    for (var i = -4; i <= 4; i++) {
        sum += $S(uv + s * f32(i));
    }
    let c = $A(uv);
    return vec4f(c.rgb + sum.rgb / 9.0 * $2, c.a);",
    },
    Block {
        id: "edges",
        name: "Edges",
        group: Group::Blur,
        about: "Only the outlines",
        inputs: PIC,
        settings: &[set("Strength", 0.0, 8.0, 2.0)],
        pass_input: true,
        stages: &[],
        number: false,
        code: "let px = 1.0 / u.resolution;
    let l = luma($A(uv - vec2f(px.x, 0.0)).rgb);
    let r = luma($A(uv + vec2f(px.x, 0.0)).rgb);
    let t = luma($A(uv - vec2f(0.0, px.y)).rgb);
    let b = luma($A(uv + vec2f(0.0, px.y)).rgb);
    return vec4f(vec3f(length(vec2f(r - l, b - t)) * $0), $A(uv).a);",
    },
    Block {
        id: "sharpen",
        name: "Sharpen",
        group: Group::Blur,
        about: "Crisper detail",
        inputs: PIC,
        settings: &[set("Amount", 0.0, 3.0, 1.0)],
        pass_input: true,
        stages: &[],
        number: false,
        code: "let px = 1.0 / u.resolution;
    let c = $A(uv);
    let n = $A(uv + vec2f(px.x, 0.0)) + $A(uv - vec2f(px.x, 0.0)) + $A(uv + vec2f(0.0, px.y)) + $A(uv - vec2f(0.0, px.y));
    return vec4f(c.rgb + (c.rgb * 4.0 - n.rgb) * $0, c.a);",
    },
    // Mix two
    Block {
        id: "mix",
        name: "Mix",
        group: Group::Mix,
        about: "A crossfade from A to B",
        inputs: TWO,
        settings: &[set("Amount", 0.0, 1.0, 0.5)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "return mix($a(uv), $b(uv), $0);",
    },
    Block {
        id: "add",
        name: "Add",
        group: Group::Mix,
        about: "B's light added to A (brighter)",
        inputs: TWO,
        settings: &[set("Amount", 0.0, 2.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let a = $a(uv);
    let b = $b(uv);
    return vec4f(a.rgb + b.rgb * $0, max(a.a, b.a));",
    },
    Block {
        id: "multiply",
        name: "Multiply",
        group: Group::Mix,
        about: "A darkened by B (black in B hides A)",
        inputs: TWO,
        settings: &[set("Amount", 0.0, 1.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let a = $a(uv);
    return vec4f(a.rgb * mix(vec3f(1.0), $b(uv).rgb, $0), a.a);",
    },
    Block {
        id: "screen",
        name: "Screen",
        group: Group::Mix,
        about: "A lightened by B, never past white",
        inputs: TWO,
        settings: &[set("Amount", 0.0, 1.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let a = $a(uv);
    let b = $b(uv).rgb * $0;
    return vec4f(1.0 - (1.0 - a.rgb) * (1.0 - b), a.a);",
    },
    Block {
        id: "difference",
        name: "Difference",
        group: Group::Mix,
        about: "How far apart A and B are (psychedelic on movement)",
        inputs: TWO,
        settings: &[set("Amount", 0.0, 1.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let a = $a(uv);
    return vec4f(mix(a.rgb, abs(a.rgb - $b(uv).rgb), $0), a.a);",
    },
    Block {
        id: "lighter",
        name: "Lighter",
        group: Group::Mix,
        about: "Whichever of A and B is brighter",
        inputs: TWO,
        settings: &[],
        pass_input: false,
        stages: &[],
        number: false,
        code: "return max($a(uv), $b(uv));",
    },
    Block {
        id: "mask",
        name: "Mask",
        group: Group::Mix,
        about: "Shows A where the mask is white and B where it's black",
        inputs: &["A", "B", "Mask"],
        settings: &[set("Invert", 0.0, 1.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let m = luma($c(uv).rgb);
    return mix($b(uv), $a(uv), mix(m, 1.0 - m, $0));",
    },
    Block {
        id: "key",
        name: "Key",
        group: Group::Mix,
        about: "A's bright parts over B (dark parts of A become see-through)",
        inputs: TWO,
        settings: &[set("Threshold", 0.0, 1.0, 0.2), set("Softness", 0.0, 0.5, 0.05)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let a = $a(uv);
    return mix($b(uv), a, smoothstep($0 - $1, $0 + $1 + 0.0001, luma(a.rgb)));",
    },
    // More pictures
    Block {
        id: "plasma",
        name: "Plasma",
        group: Group::Source,
        about: "Soft moving colour waves",
        inputs: &[],
        settings: &[set("Scale", 1.0, 20.0, 6.0), set("Speed", 0.0, 4.0, 1.0), set("Hue", 0.0, 1.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let p = centered(uv) * $0;
    let t = u.time * $1;
    let v = sin(p.x + t) + sin(p.y * 1.3 - t) + sin(length(p) - t);
    return vec4f(palette(v * 0.2 + $2), 1.0);",
    },
    Block {
        id: "dots",
        name: "Dots",
        group: Group::Source,
        about: "A grid of dots, each pulsing in its own time",
        inputs: &[],
        settings: &[set("Count", 2.0, 60.0, 12.0), set("Size", 0.0, 1.0, 0.5), set("Speed", 0.0, 8.0, 2.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let g = uv * vec2f(aspect(), 1.0) * $0;
    let d = length(fract(g) - 0.5);
    let pulse = 0.5 + 0.5 * sin(u.time * $2 + hash21(floor(g)) * TAU);
    let r = $1 * 0.5 * pulse;
    let k = 1.0 - smoothstep(r - 0.02, r, d);
    return vec4f(vec3f(k), 1.0);",
    },
    // More colour
    Block {
        id: "levels",
        name: "Levels",
        group: Group::Colour,
        about: "Black point, white point and gamma",
        inputs: PIC,
        settings: &[set("Black", 0.0, 1.0, 0.0), set("White", 0.0, 1.0, 1.0), set("Gamma", 0.2, 3.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    let x = clamp((c.rgb - $0) / max($1 - $0, 0.001), vec3f(0.0), vec3f(1.0));
    return vec4f(pow(x, vec3f(1.0 / $2)), c.a);",
    },
    Block {
        id: "vignette",
        name: "Vignette",
        group: Group::Colour,
        about: "Darkens towards the edges",
        inputs: PIC,
        settings: &[set("Size", 0.0, 1.0, 0.6), set("Softness", 0.0, 1.0, 0.5)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    let v = 1.0 - smoothstep($0, $0 + $1 + 0.001, length(centered(uv)));
    return vec4f(c.rgb * v, c.a);",
    },
    Block {
        id: "scanlines",
        name: "Scanlines",
        group: Group::Colour,
        about: "Dark lines across the picture, like an old screen; they can roll",
        inputs: PIC,
        settings: &[set("Lines", 20.0, 600.0, 200.0), set("Strength", 0.0, 1.0, 0.5), set("Roll", -2.0, 2.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    let k = 0.5 + 0.5 * sin((uv.y + u.time * $2 * 0.1) * $0 * TAU);
    return vec4f(c.rgb * (1.0 - $1 * k), c.a);",
    },
    Block {
        id: "aberration",
        name: "Colour split",
        group: Group::Colour,
        about: "Red and blue pulled apart (chromatic aberration)",
        inputs: PIC,
        settings: &[set("Amount", 0.0, 0.05, 0.01), set("Angle", 0.0, 1.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let a = $1 * TAU;
    let o = vec2f(cos(a), sin(a)) * $0;
    let c = $a(uv);
    return vec4f($a(uv + o).r, c.g, $a(uv - o).b, c.a);",
    },
    Block {
        id: "cycle",
        name: "Colour cycle",
        group: Group::Colour,
        about: "The hues turn round the wheel with time",
        inputs: PIC,
        settings: &[set("Speed", -2.0, 2.0, 0.2), set("Amount", 0.0, 1.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    var hsv = rgb2hsv(c.rgb);
    hsv.x = fract(hsv.x + u.time * $0);
    return vec4f(mix(c.rgb, hsv2rgb(hsv), $1), c.a);",
    },
    Block {
        id: "strobe",
        name: "Strobe",
        group: Group::Colour,
        about: "Flashes on and off at a rate (map Rate to the beat for a strobe on the music)",
        inputs: PIC,
        settings: &[set("Rate", 0.5, 16.0, 4.0), set("On share", 0.05, 1.0, 0.5), set("Depth", 0.0, 1.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let c = $a(uv);
    let on = step(fract(u.time * $0), $1);
    return vec4f(c.rgb * mix(1.0, on, $2), c.a);",
    },
    // More warp
    Block {
        id: "polar",
        name: "Polar",
        group: Group::Warp,
        about: "Wraps the picture round the centre (its width becomes the angle, its height the distance)",
        inputs: PIC,
        settings: &[set("Turns", 0.25, 4.0, 1.0), set("Zoom", 0.2, 3.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let p = centered(uv);
    let r = clamp(length(p) * $1, 0.0, 1.0);
    let a = atan2(p.y, p.x) / TAU + 0.5;
    return $a(vec2f(fract(a * $0), r));",
    },
    Block {
        id: "shake",
        name: "Shake",
        group: Group::Warp,
        about: "The picture jumps about at random, so many times a second",
        inputs: PIC,
        settings: &[set("Amount", 0.0, 0.1, 0.02), set("Speed", 1.0, 30.0, 10.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let t = floor(u.time * $1);
    let o = (vec2f(hash21(vec2f(t, 1.0)), hash21(vec2f(t, 7.0))) - 0.5) * $0;
    return $a(uv + o);",
    },
    Block {
        id: "slices",
        name: "Glitch slices",
        group: Group::Warp,
        about: "Horizontal strips shoved sideways at random, like a broken video",
        inputs: PIC,
        settings: &[set("Strips", 2.0, 40.0, 12.0), set("Shift", 0.0, 0.2, 0.05), set("Speed", 0.0, 20.0, 6.0)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let t = floor(u.time * $2);
    let row = floor(uv.y * $0);
    let o = (hash21(vec2f(row, t)) - 0.5) * $1 * step(0.6, hash21(vec2f(t, row + 3.0)));
    return $a(vec2f(fract(uv.x + o), uv.y));",
    },
    // More blur
    Block {
        id: "zoomblur",
        name: "Zoom blur",
        group: Group::Blur,
        about: "Streaks out from the centre",
        inputs: PIC,
        settings: &[set("Amount", 0.0, 0.3, 0.08)],
        pass_input: true,
        stages: &[],
        number: false,
        code: "var sum = vec4f(0.0);
    let c = vec2f(0.5);
    for (var i = 0; i < 12; i++) {
        sum += $A(c + (uv - c) * (1.0 - $0 * f32(i) / 12.0));
    }
    return sum / 12.0;",
    },
    // More mix
    Block {
        id: "wipe",
        name: "Wipe",
        group: Group::Mix,
        about: "A on one side of a line, B on the other; move Position to wipe between them",
        inputs: TWO,
        settings: &[set("Position", 0.0, 1.0, 0.5), set("Angle", 0.0, 1.0, 0.0), set("Softness", 0.0, 0.5, 0.05)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let a = $1 * TAU;
    let t = dot(uv - 0.5, vec2f(cos(a), sin(a))) + 0.5;
    return mix($a(uv), $b(uv), smoothstep($0 - $2, $0 + $2 + 0.0001, t));",
    },
    Block {
        id: "chroma",
        name: "Chroma key",
        group: Group::Mix,
        about: "A over B, with one colour of A (green, say) made see-through",
        inputs: TWO,
        settings: &[set("Hue", 0.0, 1.0, 0.33), set("Range", 0.0, 0.5, 0.08), set("Softness", 0.0, 0.5, 0.05)],
        pass_input: false,
        stages: &[],
        number: false,
        code: "let a = $a(uv);
    let h = rgb2hsv(a.rgb);
    let d = min(abs(h.x - $0), 1.0 - abs(h.x - $0));
    let keep = max(smoothstep($1, $1 + $2 + 0.0001, d), step(h.y, 0.25));
    return mix($b(uv), a, keep);",
    },
    // Numbers: wire into any setting
    Block {
        id: "value",
        name: "Value",
        group: Group::Number,
        about: "One number for several settings: wire it into each (map or modulate it once)",
        inputs: &[],
        settings: &[set("Value", 0.0, 1.0, 0.5)],
        pass_input: false,
        stages: &[],
        number: true,
        code: "return $0;",
    },
    Block {
        id: "lfo",
        name: "LFO",
        group: Group::Number,
        about: "A number swinging up and down so many times a second: sine, or squarer",
        inputs: &[],
        settings: &[set("Rate", 0.05, 8.0, 0.5), set("Square", 0.0, 1.0, 0.0), set("Phase", 0.0, 1.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: true,
        code: "let x = 0.5 + 0.5 * sin((u.time * $0 + $2) * TAU);
    return mix(x, step(0.5, x), $1);",
    },
    Block {
        id: "pulse",
        name: "Pulse",
        group: Group::Number,
        about: "Jumps to 1 and falls back, so many times a second (a beat-like hit)",
        inputs: &[],
        settings: &[set("Rate", 0.1, 8.0, 1.0), set("Sharpness", 0.0, 1.0, 0.5)],
        pass_input: false,
        stages: &[],
        number: true,
        code: "return pow(1.0 - fract(u.time * $0), 1.0 + $1 * 6.0);",
    },
    Block {
        id: "ramp",
        name: "Ramp",
        group: Group::Number,
        about: "0 at one side of the picture to 1 at the other, in any direction, sliding if you like: a setting that varies across the picture",
        inputs: &[],
        settings: &[set("Angle", 0.0, 1.0, 0.0), set("Slide", -2.0, 2.0, 0.0)],
        pass_input: false,
        stages: &[],
        number: true,
        code: "let a = $0 * TAU;
    return fract(dot(uv - 0.5, vec2f(cos(a), sin(a))) + 0.5 + u.time * $1);",
    },
    Block {
        id: "noisenum",
        name: "Noise number",
        group: Group::Number,
        about: "Smooth random clouds as a number: different everywhere, drifting with time",
        inputs: &[],
        settings: &[set("Scale", 1.0, 20.0, 4.0), set("Speed", 0.0, 4.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: true,
        code: "return value_noise(centered(uv) * $0 + vec2f(u.time * $1, 0.0));",
    },
    Block {
        id: "brightness",
        name: "Brightness of",
        group: Group::Number,
        about: "How bright a picture is at each point, as a number (one picture's light drives another's settings)",
        inputs: PIC,
        settings: &[set("Gain", 0.0, 4.0, 1.0)],
        pass_input: false,
        stages: &[],
        number: true,
        code: "return clamp(luma($a(uv).rgb) * $0, 0.0, 1.0);",
    },
    // Output
    Block {
        id: "output",
        name: "Output",
        group: Group::Output,
        about: "What the effect shows",
        inputs: PIC,
        settings: &[],
        pass_input: false,
        stages: &[],
        number: false,
        code: "return $a(uv);",
    },
];

pub fn block(id: &str) -> Option<&'static Block> {
    BLOCKS.iter().find(|b| b.id == id)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: u32,
    /// `Block::id`.
    pub block: String,
    /// Top-left corner on the patch canvas (points).
    pub pos: [f32; 2],
    /// One per setting (defaults of the shader's parameters).
    pub values: Vec<f32>,
    /// Switched off: its first input passes straight through (a source gives nothing). Its settings take no parameters.
    #[serde(default)]
    pub bypassed: bool,
}

pub use crate::graph::Wire;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Patch {
    pub name: String,
    pub generator: bool,
    pub nodes: Vec<Node>,
    pub wires: Vec<Wire>,
    /// A block looked at while editing: the shader shows its picture instead of the Output's (None = the Output).
    #[serde(default)]
    pub look_at: Option<u32>,
}

/// The line that carries the patch inside its shader source.
const TAG: &str = "// @patch ";

impl Patch {
    /// A new effect patch: Input wired straight to Output. A generator starts from Noise.
    pub fn new(name: &str, generator: bool) -> Patch {
        let mut p = Patch { name: name.to_string(), generator, nodes: Vec::new(), wires: Vec::new(), look_at: None };
        let from = p.add(if generator { "noise" } else { "input" }, [20.0, 40.0]);
        let to = p.add("output", [320.0, 40.0]);
        p.connect(from, to, 0);
        p
    }

    /// A ready-made example: the picture through a kaleidoscope and a glow, mixed with its own last frame for trails.
    pub fn kaleido_glow_trails() -> Patch {
        let mut p = Patch::new("Kaleido Glow Trails", false);
        let (input, out) = (p.nodes[0].id, p.nodes[1].id);
        p.nodes[1].pos = [760.0, 60.0];
        let k = p.add("kaleido", [200.0, 20.0]);
        let g = p.add("glow", [200.0, 120.0]);
        let last = p.add("last", [200.0, 230.0]);
        let mix = p.add("mix", [500.0, 80.0]);
        p.connect(input, k, 0);
        p.connect(k, g, 0);
        p.connect(g, mix, 0);
        p.connect(last, mix, 1);
        p.connect(mix, out, 0);
        p
    }

    /// Ready-made patches to start from (name, what it does, the patch): the Shader Editor's + Patch menu.
    pub fn examples() -> Vec<(&'static str, Patch)> {
        vec![
            ("The picture through a kaleidoscope and a glow, mixed with its own last frame (trails)", Patch::kaleido_glow_trails()),
            ("Edges of the picture, glowing, their colours turning, over the picture", Patch::neon_edges()),
            ("Strips shoved sideways, red and blue apart, scanlines and dark corners: a broken TV", Patch::glitch_tv()),
            ("A generator: plasma wrapped into a tunnel, echoing its last frame", Patch::tunnel_trails()),
            ("A generator: pulsing dots streaked from the centre, colours cycling", Patch::pulse_dots()),
        ]
    }

    fn neon_edges() -> Patch {
        let mut p = Patch::new("Neon Edges", false);
        let (input, out) = (p.nodes[0].id, p.nodes[1].id);
        p.nodes[1].pos = [1060.0, 60.0];
        let e = p.add("edges", [280.0, 20.0]);
        let g = p.add("glow", [540.0, 20.0]);
        let c = p.add("cycle", [800.0, 20.0]);
        let over = p.add("lighter", [800.0, 200.0]);
        p.connect(input, e, 0);
        p.connect(e, g, 0);
        p.connect(g, c, 0);
        p.connect(c, over, 0);
        p.connect(input, over, 1);
        p.connect(over, out, 0);
        p
    }

    fn glitch_tv() -> Patch {
        let mut p = Patch::new("Glitch TV", false);
        let (input, out) = (p.nodes[0].id, p.nodes[1].id);
        p.nodes[1].pos = [1060.0, 40.0];
        let chain = ["slices", "aberration", "scanlines", "vignette"];
        let mut prev = input;
        for (i, b) in chain.iter().enumerate() {
            let n = p.add(b, [280.0 + i as f32 * 195.0, 40.0]);
            p.connect(prev, n, 0);
            prev = n;
        }
        p.connect(prev, out, 0);
        p
    }

    fn tunnel_trails() -> Patch {
        let mut p = Patch::new("Tunnel Trails", true);
        let (plasma, out) = (p.nodes[0].id, p.nodes[1].id);
        p.nodes[0].block = "plasma".into();
        p.nodes[0].values = block("plasma").unwrap().settings.iter().map(|s| s.default).collect();
        p.nodes[1].pos = [800.0, 60.0];
        let polar = p.add("polar", [280.0, 20.0]);
        let last = p.add("last", [280.0, 200.0]);
        let mix = p.add("lighter", [540.0, 80.0]);
        p.connect(plasma, polar, 0);
        p.connect(polar, mix, 0);
        p.connect(last, mix, 1);
        p.connect(mix, out, 0);
        p
    }

    fn pulse_dots() -> Patch {
        let mut p = Patch::new("Pulse Dots", true);
        let (dots, out) = (p.nodes[0].id, p.nodes[1].id);
        p.nodes[0].block = "dots".into();
        p.nodes[0].values = block("dots").unwrap().settings.iter().map(|s| s.default).collect();
        p.nodes[1].pos = [800.0, 40.0];
        let z = p.add("zoomblur", [280.0, 40.0]);
        let c = p.add("cycle", [540.0, 40.0]);
        p.connect(dots, z, 0);
        p.connect(z, c, 0);
        p.connect(c, out, 0);
        p
    }

    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Adds a block (its settings at their defaults) and returns its id.
    pub fn add(&mut self, block_id: &str, pos: [f32; 2]) -> u32 {
        let id = self.nodes.iter().map(|n| n.id + 1).max().unwrap_or(1);
        let values = block(block_id).map(|b| b.settings.iter().map(|s| s.default).collect()).unwrap_or_default();
        self.nodes.push(Node { id, block: block_id.to_string(), pos, values, bypassed: false });
        id
    }

    /// Removes a block and its wires.
    pub fn remove(&mut self, id: u32) {
        self.nodes.retain(|n| n.id != id);
        crate::graph::remove(&mut self.wires, id);
        if self.look_at == Some(id) {
            self.look_at = None;
        }
    }

    /// Wires `from`'s output into input `input` of `to` (replacing what was there). Refused (false) when it would
    /// make a loop, or the input doesn't exist.
    pub fn connect(&mut self, from: u32, to: u32, input: usize) -> bool {
        let Some(b) = self.node(to).and_then(|n| block(&n.block)) else { return false };
        let Some(f) = self.node(from).and_then(|n| block(&n.block)) else { return false };
        // A picture goes into a picture input (the first ones); a number into a setting (the ones after).
        if f.number != (input >= b.inputs.len()) {
            return false;
        }
        crate::graph::connect(&mut self.wires, from, to, input, b.inputs.len() + b.settings.len())
    }

    /// What's wired into setting `i` of `id` (a number block), if anything.
    pub fn setting_source(&self, id: u32, i: usize) -> Option<u32> {
        let inputs = self.node(id).and_then(|n| block(&n.block)).map_or(0, |b| b.inputs.len());
        self.source_of(id, inputs + i)
    }

    pub fn disconnect(&mut self, to: u32, input: usize) {
        crate::graph::disconnect(&mut self.wires, to, input);
    }

    /// Switches a block off (its first input passes through) or on again. Output can't be switched off.
    pub fn set_bypassed(&mut self, id: u32, off: bool) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id && n.block != "output" && block(&n.block).is_some_and(|b| !b.number)) {
            n.bypassed = off;
        }
    }

    /// Puts a new block on the wire into input `input` of `to`: what fed `to` now feeds the new block's first
    /// input, and the new block feeds `to`. Placed halfway between the two. Returns the new block's id (None when
    /// nothing is wired there, or the block has no input).
    pub fn insert(&mut self, block_id: &str, to: u32, input: usize) -> Option<u32> {
        let from = self.source_of(to, input)?;
        let pictures = self.node(to).and_then(|n| block(&n.block)).map_or(0, |b| b.inputs.len());
        if input >= pictures || block(block_id).is_none_or(|b| b.inputs.is_empty() || b.number) {
            return None;
        }
        let (a, b) = (self.node(from)?.pos, self.node(to)?.pos);
        let id = self.add(block_id, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]);
        self.connect(from, id, 0);
        self.connect(id, to, input);
        Some(id)
    }

    /// Lays the blocks out left to right by how far they are from the sources (a column per step), each column
    /// top to bottom in wiring order; `height_of` says how tall a block draws, `col_w` is a column's pitch.
    pub fn tidy(&mut self, col_w: f32, height_of: impl Fn(&Node) -> f32) {
        let ids: Vec<u32> = self.nodes.iter().map(|n| n.id).collect();
        let order = crate::graph::order(&ids, &self.wires);
        // Depth = the longest chain of wires before a block.
        let mut depth: Vec<(u32, usize)> = Vec::new();
        for id in &order {
            let d = self.wires.iter().filter(|w| w.to == *id).filter_map(|w| depth.iter().find(|(n, _)| *n == w.from).map(|(_, d)| d + 1)).max().unwrap_or(0);
            depth.push((*id, d));
        }
        // Output always last, on its own column.
        let last = depth.iter().map(|(_, d)| *d).max().unwrap_or(0);
        for (id, d) in depth.iter_mut() {
            if self.node(*id).is_some_and(|n| n.block == "output") {
                *d = last.max(1);
            }
        }
        let mut next_y: Vec<f32> = vec![20.0; last + 2];
        for (id, d) in &depth {
            let Some(n) = self.nodes.iter_mut().find(|n| n.id == *id) else { continue };
            let h = height_of(n);
            n.pos = [20.0 + *d as f32 * col_w, next_y[*d]];
            next_y[*d] += h + 24.0;
        }
        // Blocks in a loop (none, normally) stay where they were.
    }

    /// What's wired into input `input` of `to`.
    pub fn source_of(&self, to: u32, input: usize) -> Option<u32> {
        crate::graph::source_of(&self.wires, to, input)
    }

    /// The block whose picture the shader shows: the one looked at (a picture block), else Output.
    fn output(&self) -> Option<&Node> {
        self.look_at.and_then(|id| self.node(id)).filter(|n| block(&n.block).is_some_and(|b| !b.number)).or_else(|| self.nodes.iter().find(|n| n.block == "output"))
    }

    /// Looks at a picture block (its picture becomes the shader's while editing), or None for the Output again.
    pub fn set_look_at(&mut self, id: Option<u32>) {
        self.look_at = id.filter(|id| self.node(*id).and_then(|n| block(&n.block)).is_some_and(|b| !b.number && b.id != "output"));
    }

    /// Blocks that reach the output, in patch order: these get the shader's parameters.
    fn used(&self) -> Vec<&Node> {
        let mut reach = Vec::new();
        let mut todo: Vec<u32> = self.output().map(|o| o.id).into_iter().collect();
        while let Some(n) = todo.pop() {
            if !reach.contains(&n) {
                reach.push(n);
                todo.extend(self.wires.iter().filter(|w| w.to == n).map(|w| w.from));
            }
        }
        self.nodes.iter().filter(|n| reach.contains(&n.id)).collect()
    }

    /// (block id, setting index) of each shader parameter, in order: P(0u) is the first. At most 60 (`MAX_PARAMS`); later
    /// settings are written into the code as fixed numbers.
    pub fn param_slots(&self) -> Vec<(u32, usize)> {
        let mut slots = Vec::new();
        for n in self.used().into_iter().filter(|n| !n.bypassed) {
            let count = block(&n.block).map_or(0, |b| b.settings.len());
            slots.extend((0..count).filter(|i| self.setting_source(n.id, *i).is_none()).map(|i| (n.id, i)));
        }
        slots.truncate(crate::shaders::MAX_PARAMS);
        slots
    }

    /// The patch found in a shader source (its `// @patch` line), if it has one.
    pub fn from_source(source: &str) -> Option<Patch> {
        source.lines().find_map(|l| l.trim().strip_prefix(TAG.trim_end())).and_then(|j| serde_json::from_str(j.trim()).ok())
    }

    /// The user shader for this patch (header, the patch line, then the code).
    pub fn compile(&self) -> Result<String, String> {
        let slots = self.param_slots();
        let mut names: Vec<String> = Vec::new();
        let mut header = format!("// @name {}\n// @kind {}\n", self.name.replace('\n', " "), if self.generator { "generator" } else { "effect" });
        for (id, i) in &slots {
            let n = self.node(*id).unwrap();
            let b = block(&n.block).unwrap();
            let s = b.settings[*i];
            // "Blur Size", and "Blur 2 Size" for the second Blur.
            let nth = self.used().iter().filter(|m| m.block == n.block).position(|m| m.id == n.id).unwrap_or(0);
            let name = if nth == 0 { format!("{} {}", b.name, s.name) } else { format!("{} {} {}", b.name, nth + 1, s.name) };
            names.push(name.clone());
            let v = n.values.get(*i).copied().unwrap_or(s.default);
            header.push_str(&format!("// @param {name} {} {} {}\n", s.min, s.max, v.clamp(s.min, s.max)));
        }
        let mut code = Gen { patch: self, slots: &slots, funcs: String::new(), passes: Vec::new(), done: Vec::new(), visiting: Vec::new(), feedback: false };
        let out = match self.output() {
            Some(o) => code.node(o.id)?,
            None => return Err("add an Output block".into()),
        };
        let budget = if code.feedback { crate::custom::MAX_PASSES - 1 } else { crate::custom::MAX_PASSES };
        if code.passes.len() > budget {
            return Err(format!("too many blur / glow / edge blocks: they need {} passes, at most {budget}", code.passes.len()));
        }
        if code.feedback {
            header.push_str("// @feedback\n");
        }
        for (name, scale) in &code.passes {
            header.push_str(&format!("// @pass {name} {scale}\n"));
        }
        let json = serde_json::to_string(self).map_err(|e| e.to_string())?;
        Ok(format!(
            "{header}{TAG}{json}\n\n// Made in the patch editor: edit the blocks there (changes here are replaced).\n\nfn none(uv: vec2f) -> vec4f {{\n    return vec4f(0.0);\n}}\n{}\n@fragment\nfn main(in: VOut) -> @location(0) vec4f {{\n    return {out}(in.uv);\n}}\n",
            code.funcs
        ))
    }
}

/// Code generation state: functions written so far and the passes they need, in drawing order.
struct Gen<'a> {
    patch: &'a Patch,
    slots: &'a [(u32, usize)],
    funcs: String,
    passes: Vec<(String, f32)>,
    done: Vec<u32>,
    visiting: Vec<u32>,
    feedback: bool,
}

impl Gen<'_> {
    /// Writes block `id` (after everything wired into it) and returns its function's name.
    fn node(&mut self, id: u32) -> Result<String, String> {
        let name = format!("n{id}");
        if self.done.contains(&id) {
            return Ok(name);
        }
        if self.visiting.contains(&id) {
            return Err("the blocks are wired in a loop".into());
        }
        self.visiting.push(id);
        let n = self.patch.node(id).ok_or("a wire to a missing block")?;
        let b = block(&n.block).ok_or_else(|| format!("unknown block {}", n.block))?;
        let mut inputs = Vec::new();
        for i in 0..b.inputs.len() {
            inputs.push(match self.patch.source_of(id, i) {
                Some(from) => self.node(from)?,
                None => "none".to_string(),
            });
        }
        // Settings wired to a number block: that block's function, scaled to the setting's range.
        let mut wired: Vec<Option<String>> = Vec::new();
        for i in 0..b.settings.len() {
            wired.push(match self.patch.setting_source(id, i) {
                Some(from) => Some(self.node(from)?),
                None => None,
            });
        }
        let ret = if b.number { "f32" } else { "vec4f" };
        if n.bypassed {
            let through = inputs.first().map_or("none", |s| s.as_str());
            self.funcs.push_str(&format!("\n// {} (switched off)\nfn {name}(uv: vec2f) -> vec4f {{\n    return {through}(uv);\n}}\n", b.name));
            self.visiting.retain(|v| *v != id);
            self.done.push(id);
            return Ok(name);
        }
        if b.id == "last" {
            self.feedback = true;
        }
        let fill = |code: &str, this: &Gen| -> String {
            let mut c = code.to_string();
            for (k, name) in ["$a", "$b", "$c"].iter().zip(&inputs) {
                c = c.replace(k, name);
            }
            for (i, s) in b.settings.iter().enumerate().rev() {
                let v = match (&wired[i], this.slots.iter().position(|(sid, si)| *sid == id && *si == i)) {
                    (Some(from), _) => format!("({:.5} + {:.5} * clamp({from}(uv), 0.0, 1.0))", s.min, s.max - s.min),
                    (None, Some(slot)) => format!("P({slot}u)"),
                    (None, None) => format!("{:.5}", n.values.get(i).copied().unwrap_or(s.default)),
                };
                c = c.replace(&format!("${i}"), &v);
            }
            c
        };
        if b.pass_input {
            let first = format!("p{id}in");
            self.funcs.push_str(&format!("\nfn {first}(in: VOut) -> vec4f {{\n    return {}(in.uv);\n}}\n", inputs.first().map_or("none", |s| s)));
            self.passes.push((first.clone(), 1.0));
            let mut prev = first.clone();
            for (k, (body, scale)) in b.stages.iter().enumerate() {
                let stage = format!("p{id}s{k}");
                let code = fill(body, self).replace("$A", &format!("read_{first}")).replace("$S", &format!("read_{prev}"));
                self.funcs.push_str(&format!("\nfn {stage}(in: VOut) -> vec4f {{\n    let uv = in.uv;\n    {code}\n}}\n"));
                self.passes.push((stage.clone(), *scale));
                prev = stage;
            }
            let code = fill(b.code, self).replace("$A", &format!("read_{first}")).replace("$S", &format!("read_{prev}"));
            self.funcs.push_str(&format!("\n// {}\nfn {name}(uv: vec2f) -> {ret} {{\n    {code}\n}}\n", b.name));
        } else {
            let code = fill(b.code, self);
            self.funcs.push_str(&format!("\n// {}\nfn {name}(uv: vec2f) -> {ret} {{\n    {code}\n}}\n", b.name));
        }
        self.visiting.retain(|v| *v != id);
        self.done.push(id);
        Ok(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(p: &Patch) -> String {
        let src = p.compile().unwrap();
        let (full, lines) = crate::custom::module_source(&src, "fs_test").unwrap();
        crate::custom::validate(&full, lines).unwrap_or_else(|e| panic!("{e}\n{src}"));
        src
    }

    #[test]
    fn every_block_compiles_wired_into_an_output() {
        for b in BLOCKS.iter().filter(|b| b.id != "output") {
            let mut p = Patch::new(b.name, false);
            let input = p.nodes[0].id;
            let out = p.nodes[1].id;
            let x = p.add(b.id, [0.0, 0.0]);
            for i in 0..b.inputs.len() {
                assert!(p.connect(input, x, i));
            }
            if b.number {
                // A number goes into a setting (Hue shift's Shift), never into Output.
                assert!(!p.connect(x, out, 0));
                let hue = p.add("hue", [0.0, 0.0]);
                p.connect(input, hue, 0);
                assert!(p.connect(x, hue, 1), "{}: into Shift", b.name);
                p.connect(hue, out, 0);
                let src = check(&p);
                assert_eq!(crate::custom::parse_header(&src).unwrap().params.len(), b.settings.len() + 1, "{}: its own settings + Hue shift's Spin", b.name);
            } else {
                assert!(p.connect(x, out, 0));
                let src = check(&p);
                assert_eq!(crate::custom::parse_header(&src).unwrap().params.len(), b.settings.len(), "{}", b.name);
            }
        }
    }

    #[test]
    fn a_number_wired_into_a_setting_drives_it_and_kinds_must_match() {
        let mut p = Patch::new("LFO hue", false);
        let (input, out) = (p.nodes[0].id, p.nodes[1].id);
        let hue = p.insert("hue", out, 0).unwrap();
        let lfo = p.add("lfo", [0.0, 0.0]);
        assert!(!p.connect(input, hue, 1), "a picture can't go into a setting");
        assert!(!p.connect(lfo, hue, 0), "a number can't go into a picture input");
        assert!(p.connect(lfo, hue, 1));
        assert_eq!(p.setting_source(hue, 0), Some(lfo));
        assert!(p.insert("blur", hue, 1).is_none(), "nothing goes on a number wire");
        let src = check(&p);
        let names: Vec<&str> = crate::custom::parse_header(&src).unwrap().params.iter().map(|q| q.name).collect();
        assert_eq!(names, ["Hue shift Spin", "LFO Rate", "LFO Square", "LFO Phase"], "Shift follows the LFO: no parameter");
        assert!(src.contains("clamp(n4(uv), 0.0, 1.0)"), "{src}");
        p.set_bypassed(lfo, true);
        assert!(!p.node(lfo).unwrap().bypassed, "number blocks don't switch off");
        // Brightness of: a picture in, a number out.
        let b = p.add("brightness", [0.0, 0.0]);
        p.connect(input, b, 0);
        p.disconnect(hue, 2);
        assert!(p.connect(b, hue, 2));
        check(&p);
    }

    #[test]
    fn every_example_compiles() {
        for (about, p) in Patch::examples() {
            assert!(!about.is_empty());
            check(&p);
        }
        assert_eq!(Patch::examples().len(), 5);
    }

    #[test]
    fn the_patch_rides_along_in_its_source() {
        let mut p = Patch::new("Trails", false);
        let last = p.add("last", [0.0, 200.0]);
        let add = p.add("lighter", [200.0, 100.0]);
        let (input, out) = (p.nodes[0].id, p.nodes[1].id);
        p.connect(input, add, 0);
        p.connect(last, add, 1);
        p.connect(add, out, 0);
        let src = check(&p);
        assert!(crate::custom::parse_header(&src).unwrap().feedback, "Last frame turns on @feedback");
        assert_eq!(Patch::from_source(&src), Some(p));
        assert_eq!(Patch::from_source("// @name plain\n"), None);
    }

    #[test]
    fn blur_and_glow_get_passes_and_feedback_fits_with_them() {
        let mut p = Patch::new("Soft trails", false);
        let (input, out) = (p.nodes[0].id, p.nodes[1].id);
        let blur = p.add("blur", [0.0, 0.0]);
        let glow = p.add("glow", [0.0, 0.0]);
        let last = p.add("last", [0.0, 0.0]);
        let mix = p.add("mix", [0.0, 0.0]);
        p.connect(input, blur, 0);
        p.connect(blur, glow, 0);
        p.connect(glow, mix, 0);
        p.connect(last, mix, 1);
        p.connect(mix, out, 0);
        let src = check(&p);
        let h = crate::custom::parse_header(&src).unwrap();
        assert_eq!(h.passes.len(), 4, "blur: input + across; glow: input + spread");
        assert!(h.feedback);
        assert_eq!(h.passes[3].1, 0.5, "glow spreads at half size");
        // Too many blurs for the passes there are.
        let mut prev = blur;
        for _ in 0..3 {
            let b = p.add("blur", [0.0, 0.0]);
            p.connect(prev, b, 0);
            prev = b;
        }
        p.connect(prev, glow, 0);
        assert!(p.compile().unwrap_err().contains("passes"));
    }

    #[test]
    fn loops_are_refused_and_settings_become_parameters() {
        let mut p = Patch::new("Loop", false);
        let (input, out) = (p.nodes[0].id, p.nodes[1].id);
        let a = p.add("invert", [0.0, 0.0]);
        let b = p.add("hue", [0.0, 0.0]);
        assert!(p.connect(a, b, 0));
        assert!(!p.connect(b, a, 0), "b already reads a");
        assert!(!p.connect(a, a, 0));
        p.connect(input, a, 0);
        p.connect(b, out, 0);
        // A second Hue shift is numbered; unused blocks take no parameters.
        let c = p.add("hue", [0.0, 0.0]);
        p.add("adjust", [0.0, 0.0]);
        p.connect(b, c, 0);
        p.connect(c, out, 0);
        let h = crate::custom::parse_header(&check(&p)).unwrap();
        let names: Vec<&str> = h.params.iter().map(|q| q.name).collect();
        assert_eq!(names, ["Invert Amount", "Hue shift Shift", "Hue shift Spin", "Hue shift 2 Shift", "Hue shift 2 Spin"]);
        assert_eq!(p.param_slots().len(), 5);
    }

    #[test]
    fn a_switched_off_block_passes_its_input_through_and_takes_no_parameters() {
        let mut p = Patch::new("Off", false);
        let (input, out) = (p.nodes[0].id, p.nodes[1].id);
        let blur = p.insert("blur", out, 0).unwrap();
        assert_eq!(p.source_of(blur, 0), Some(input));
        assert_eq!(p.source_of(out, 0), Some(blur));
        assert!(p.insert("colour", out, 0).is_none(), "a source has no input to put on a wire");
        p.set_bypassed(blur, true);
        let src = check(&p);
        let h = crate::custom::parse_header(&src).unwrap();
        assert!(h.params.is_empty() && h.passes.is_empty(), "off: no parameters, no passes");
        assert!(src.contains("(switched off)"));
        p.set_bypassed(out, true);
        assert!(!p.node(out).unwrap().bypassed, "Output stays on");
        p.set_bypassed(blur, false);
        assert_eq!(crate::custom::parse_header(&check(&p)).unwrap().passes.len(), 2);
        // Old saves (no `bypassed`) still open.
        let old = serde_json::json!({ "id": 9, "block": "invert", "pos": [1.0, 2.0], "values": [1.0] });
        let n: Node = serde_json::from_value(old).unwrap();
        assert!(!n.bypassed);
    }

    #[test]
    fn looking_at_a_block_shows_its_picture_instead_of_the_output() {
        let mut p = Patch::kaleido_glow_trails();
        let (k, g) = (p.nodes[2].id, p.nodes[3].id);
        let all = crate::custom::parse_header(&check(&p)).unwrap().params.len();
        p.set_look_at(Some(k));
        let src = check(&p);
        let h = crate::custom::parse_header(&src).unwrap();
        assert!(src.contains(&format!("return n{k}(in.uv);")), "{src}");
        assert!(h.params.len() < all, "only the blocks before the kaleidoscope take parameters");
        assert!(!h.feedback, "Last frame isn't reached");
        p.set_look_at(Some(p.nodes[4].id + 100));
        assert_eq!(p.look_at, None, "an unknown block: the Output");
        p.set_look_at(Some(g));
        p.remove(g);
        assert_eq!(p.look_at, None, "removing the block looked at goes back to the Output");
        assert!(serde_json::from_str::<Patch>(r#"{"name":"old","generator":false,"nodes":[],"wires":[]}"#).is_ok(), "old saves have no look_at");
    }

    #[test]
    fn tidy_puts_each_block_a_column_after_what_feeds_it() {
        let mut p = Patch::kaleido_glow_trails();
        p.tidy(260.0, |_| 60.0);
        let x = |id: u32| p.node(id).unwrap().pos[0];
        let (input, out) = (p.nodes[0].id, p.nodes[1].id);
        let (k, g, last, mix) = (p.nodes[2].id, p.nodes[3].id, p.nodes[4].id, p.nodes[5].id);
        assert_eq!(x(input), 20.0);
        assert_eq!(x(last), 20.0, "a source: first column");
        assert_eq!(x(k), 280.0);
        assert_eq!(x(g), 540.0);
        assert_eq!(x(mix), 800.0);
        assert!(x(out) > x(mix), "Output last");
        let ys: Vec<f32> = [input, last].iter().map(|id| p.node(*id).unwrap().pos[1]).collect();
        assert_ne!(ys[0], ys[1], "two blocks in one column don't overlap");
    }

    #[test]
    fn settings_past_the_parameter_limit_are_written_in_as_numbers() {
        let mut p = Patch::new("Many", true);
        let mut prev = p.nodes[0].id;
        let out = p.nodes[1].id;
        for _ in 0..21 {
            let a = p.add("adjust", [0.0, 0.0]);
            p.connect(prev, a, 0);
            prev = a;
        }
        p.connect(prev, out, 0);
        let src = check(&p);
        assert_eq!(crate::custom::parse_header(&src).unwrap().params.len(), crate::shaders::MAX_PARAMS);
        assert!(src.contains("1.00000"), "a contrast past the last parameter is a plain number");
    }
}
