// Built-in generators (sources with no input). Parameters: P(0..15).

fn hash21(p: vec2f) -> f32 {
    return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453);
}

fn value_noise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2f(1.0, 0.0));
    let c = hash21(i + vec2f(0.0, 1.0));
    let d = hash21(i + vec2f(1.0, 1.0));
    return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}

fn centered(uv: vec2f) -> vec2f {
    return (uv - 0.5) * vec2f(aspect(), 1.0);
}

@fragment
fn gen_plasma(in: VOut) -> @location(0) vec4f {
    let p = centered(in.uv) * P(1u);
    let t = u.time * P(0u);
    var v = sin(p.x + t);
    v = v + sin((p.y + t) * 0.5);
    v = v + sin((p.x + p.y + t) * 0.5);
    v = v + sin(length(p + vec2f(sin(t / 3.0), cos(t / 2.0)) * 3.0) + t);
    return vec4f(palette(v * 0.25 + P(2u)), 1.0);
}

@fragment
fn gen_rings(in: VOut) -> @location(0) vec4f {
    let d = length(centered(in.uv));
    let f = fract(d * P(0u) - u.time * P(1u));
    let w = P(2u) * 0.5;
    let v = smoothstep(0.5 - w - 0.02, 0.5 - w, f) * (1.0 - smoothstep(0.5 + w, 0.5 + w + 0.02, f));
    return vec4f(vec3f(v), 1.0);
}

@fragment
fn gen_gradient(in: VOut) -> @location(0) vec4f {
    let a = P(1u) * TAU;
    let v = dot(in.uv - 0.5, vec2f(cos(a), sin(a))) + u.time * P(0u);
    return vec4f(palette(v), 1.0);
}

@fragment
fn gen_checker(in: VOut) -> @location(0) vec4f {
    let p = centered(in.uv) * P(0u) + vec2f(u.time * P(1u), 0.0);
    let c = (i32(floor(p.x)) + i32(floor(p.y))) & 1;
    return vec4f(vec3f(f32(c)), 1.0);
}

@fragment
fn gen_noise(in: VOut) -> @location(0) vec4f {
    var p = centered(in.uv) * P(0u) + vec2f(u.time * P(1u));
    var v = 0.0;
    var amp = 0.5;
    for (var i = 0; i < 5; i = i + 1) {
        v = v + value_noise(p) * amp;
        p = p * 2.03;
        amp = amp * 0.5;
    }
    return vec4f(vec3f(v), 1.0);
}

@fragment
fn gen_color_bars(in: VOut) -> @location(0) vec4f {
    let i = u32(clamp(floor(in.uv.x * 8.0), 0.0, 7.0));
    var bars = array<vec3f, 8>(
        vec3f(1.0, 1.0, 1.0), vec3f(1.0, 1.0, 0.0), vec3f(0.0, 1.0, 1.0), vec3f(0.0, 1.0, 0.0),
        vec3f(1.0, 0.0, 1.0), vec3f(1.0, 0.0, 0.0), vec3f(0.0, 0.0, 1.0), vec3f(0.0, 0.0, 0.0),
    );
    return vec4f(bars[i] * 0.75, 1.0);
}

@fragment
fn gen_solid(in: VOut) -> @location(0) vec4f {
    return vec4f(P(0u), P(1u), P(2u), 1.0);
}
