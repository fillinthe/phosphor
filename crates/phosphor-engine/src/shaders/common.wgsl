// Shared by every pass. Must match `Uniforms` in gpu.rs (128 bytes).
struct U {
    p: array<vec4f, 5>,   // up to 16 shader parameters (mapping passes use all 20 floats)
    resolution: vec2f,    // target size in pixels
    time: f32,            // seconds
    opacity: f32,
    uv_xform: vec4f,      // source uv = uv * xy + zw
    mode: u32,            // blend mode
    yuv: u32,             // 1 = tex0 is luma (R8), tex2 is chroma (RG8), BT.709 video range
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var samp: sampler;          // linear, clamp to edge
@group(0) @binding(2) var samp_mirror: sampler;   // linear, mirrored repeat
@group(0) @binding(3) var tex0: texture_2d<f32>;  // source
@group(0) @binding(4) var tex1: texture_2d<f32>;  // destination / accumulator
@group(0) @binding(5) var tex2: texture_2d<f32>;  // chroma plane

struct VOut {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
};

// One oversized triangle covers the target; no vertex buffer needed.
@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> VOut {
    let p = vec2f(f32((vi << 1u) & 2u), f32(vi & 2u));
    var o: VOut;
    o.pos = vec4f(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2f(p.x, 1.0 - p.y);
    return o;
}

const TAU: f32 = 6.283185307;

fn P(i: u32) -> f32 {
    return u.p[i / 4u][i % 4u];
}

fn src(uv: vec2f) -> vec4f {
    return textureSampleLevel(tex0, samp, uv, 0.0);
}

fn luma(c: vec3f) -> f32 {
    return dot(c, vec3f(0.2126, 0.7152, 0.0722));
}

fn aspect() -> f32 {
    return u.resolution.x / max(u.resolution.y, 1.0);
}

fn rgb2hsv(c: vec3f) -> vec3f {
    let K = vec4f(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4f(c.bg, K.wz), vec4f(c.gb, K.xy), step(c.b, c.g));
    let q = mix(vec4f(p.xyw, c.r), vec4f(c.r, p.yzx), step(p.x, c.r));
    let d = q.x - min(q.w, q.y);
    let e = 1.0e-10;
    return vec3f(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

fn hsv2rgb(c: vec3f) -> vec3f {
    let K = vec4f(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, vec3f(0.0), vec3f(1.0)), c.y);
}

fn palette(t: f32) -> vec3f {
    return 0.5 + 0.5 * cos(TAU * (t + vec3f(0.0, 0.33, 0.67)));
}
