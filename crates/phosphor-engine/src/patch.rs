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
    Output,
}

impl Group {
    pub const ALL: [Group; 6] = [Group::Source, Group::Colour, Group::Warp, Group::Blur, Group::Mix, Group::Output];

    pub fn name(self) -> &'static str {
        match self {
            Group::Source => "Pictures",
            Group::Colour => "Colour",
            Group::Warp => "Warp",
            Group::Blur => "Blur & edges",
            Group::Mix => "Mix two",
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
        code: "let a = $a(uv);
    return mix($b(uv), a, smoothstep($0 - $1, $0 + $1 + 0.0001, luma(a.rgb)));",
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
}

pub use crate::graph::Wire;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Patch {
    pub name: String,
    pub generator: bool,
    pub nodes: Vec<Node>,
    pub wires: Vec<Wire>,
}

/// The line that carries the patch inside its shader source.
const TAG: &str = "// @patch ";

impl Patch {
    /// A new effect patch: Input wired straight to Output. A generator starts from Noise.
    pub fn new(name: &str, generator: bool) -> Patch {
        let mut p = Patch { name: name.to_string(), generator, nodes: Vec::new(), wires: Vec::new() };
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

    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Adds a block (its settings at their defaults) and returns its id.
    pub fn add(&mut self, block_id: &str, pos: [f32; 2]) -> u32 {
        let id = self.nodes.iter().map(|n| n.id + 1).max().unwrap_or(1);
        let values = block(block_id).map(|b| b.settings.iter().map(|s| s.default).collect()).unwrap_or_default();
        self.nodes.push(Node { id, block: block_id.to_string(), pos, values });
        id
    }

    /// Removes a block and its wires.
    pub fn remove(&mut self, id: u32) {
        self.nodes.retain(|n| n.id != id);
        crate::graph::remove(&mut self.wires, id);
    }

    /// Wires `from`'s output into input `input` of `to` (replacing what was there). Refused (false) when it would
    /// make a loop, or the input doesn't exist.
    pub fn connect(&mut self, from: u32, to: u32, input: usize) -> bool {
        let inputs = self.node(to).and_then(|n| block(&n.block)).map_or(0, |b| b.inputs.len());
        self.node(from).is_some() && crate::graph::connect(&mut self.wires, from, to, input, inputs)
    }

    pub fn disconnect(&mut self, to: u32, input: usize) {
        crate::graph::disconnect(&mut self.wires, to, input);
    }

    /// What's wired into input `input` of `to`.
    pub fn source_of(&self, to: u32, input: usize) -> Option<u32> {
        crate::graph::source_of(&self.wires, to, input)
    }

    fn output(&self) -> Option<&Node> {
        self.nodes.iter().find(|n| n.block == "output")
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
        for n in self.used() {
            let count = block(&n.block).map_or(0, |b| b.settings.len());
            slots.extend((0..count).map(|i| (n.id, i)));
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
        if b.id == "last" {
            self.feedback = true;
        }
        let fill = |code: &str, this: &Gen| -> String {
            let mut c = code.to_string();
            for (k, name) in ["$a", "$b", "$c"].iter().zip(&inputs) {
                c = c.replace(k, name);
            }
            for (i, s) in b.settings.iter().enumerate().rev() {
                let v = match this.slots.iter().position(|(sid, si)| *sid == id && *si == i) {
                    Some(slot) => format!("P({slot}u)"),
                    None => format!("{:.5}", n.values.get(i).copied().unwrap_or(s.default)),
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
            self.funcs.push_str(&format!("\n// {}\nfn {name}(uv: vec2f) -> vec4f {{\n    {code}\n}}\n", b.name));
        } else {
            let code = fill(b.code, self);
            self.funcs.push_str(&format!("\n// {}\nfn {name}(uv: vec2f) -> vec4f {{\n    {code}\n}}\n", b.name));
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
            assert!(p.connect(x, out, 0));
            let src = check(&p);
            assert_eq!(crate::custom::parse_header(&src).unwrap().params.len(), b.settings.len(), "{}", b.name);
        }
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
