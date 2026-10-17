// Built-in generators (sources with no input). Parameters: P(0..15).

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

@fragment
fn gen_tunnel(in: VOut) -> @location(0) vec4f {
    let p = centered(in.uv);
    let r = max(length(p), 0.002);
    let a = atan2(p.y, p.x);
    let depth = P(0u) / r + u.time * P(1u);
    let around = a / TAU * P(2u);
    let cell = step(0.5, fract(depth)) + step(0.5, fract(around));
    let checker = abs(cell - 1.0);
    let fog = smoothstep(0.0, 0.5, r);
    return vec4f(palette(depth * 0.05 + P(3u)) * (0.35 + 0.65 * checker) * fog, 1.0);
}

@fragment
fn gen_starfield(in: VOut) -> @location(0) vec4f {
    var col = vec3f(0.0);
    let p = centered(in.uv);
    for (var layer = 0; layer < 3; layer = layer + 1) {
        let z = fract(u.time * P(1u) * 0.1 + f32(layer) / 3.0);
        let scale = mix(P(0u) * 2.0, P(0u) * 0.25, z);
        let q = p * scale;
        let cell = floor(q);
        let h = hash21(cell + f32(layer) * 17.0);
        let pos = fract(q) - (vec2f(hash21(cell + 3.1), hash21(cell + 7.7)));
        let d = length(pos);
        let twinkle = 0.7 + 0.3 * sin(u.time * 6.0 + h * TAU);
        let star = smoothstep(0.08 * (1.0 - z) + 0.01, 0.0, d) * step(1.0 - P(2u) * 0.5, h) * twinkle;
        col = col + mix(vec3f(1.0), palette(h), 0.35) * star * smoothstep(0.0, 0.2, z) * smoothstep(1.0, 0.8, z);
    }
    return vec4f(clamp(col, vec3f(0.0), vec3f(1.0)), 1.0);
}

@fragment
fn gen_spiral(in: VOut) -> @location(0) vec4f {
    let p = centered(in.uv);
    let r = max(length(p), 0.001);
    let a = atan2(p.y, p.x);
    let v = sin(a * P(0u) + log(r) * P(1u) - u.time * P(2u));
    let band = smoothstep(-0.1, 0.1, v);
    return vec4f(palette(a / TAU + r * 0.5 + u.time * 0.05) * (0.15 + 0.85 * band), 1.0);
}

@fragment
fn gen_voronoi(in: VOut) -> @location(0) vec4f {
    let p = centered(in.uv) * P(0u);
    let cell = floor(p);
    var best = 10.0;
    var second = 10.0;
    var best_id = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let n = cell + vec2f(f32(x), f32(y));
            let h = hash21(n);
            let site = n + 0.5 + 0.4 * vec2f(sin(u.time * P(1u) + h * TAU), cos(u.time * P(1u) * 1.3 + h * 9.0));
            let d = length(p - site);
            if (d < best) {
                second = best;
                best = d;
                best_id = h;
            } else if (d < second) {
                second = d;
            }
        }
    }
    let edge = smoothstep(0.0, 0.08, second - best);
    let col = palette(best_id + P(2u)) * (0.3 + 0.7 * edge);
    return vec4f(col, 1.0);
}

@fragment
fn gen_lines(in: VOut) -> @location(0) vec4f {
    var col = vec3f(0.0);
    let n = u32(max(P(0u), 1.0));
    for (var i = 0u; i < 12u; i = i + 1u) {
        if (i >= n) { break; }
        let f = f32(i) / f32(max(n, 2u) - 1u);
        let y = 0.5 + P(1u) * sin(in.uv.x * P(2u) * TAU + u.time * P(3u) + f * TAU) * (0.3 + 0.7 * f);
        let d = abs(in.uv.y - y);
        let line = smoothstep(0.012, 0.0, d);
        col = col + palette(f + u.time * 0.05) * line;
    }
    return vec4f(clamp(col, vec3f(0.0), vec3f(1.0)), 1.0);
}

// Audio visualisers: tex1 is the live spectrum (one row of magnitudes 0..1, low to high) for the bars and the
// live waveform (one row, 0.5 = silence) for the oscilloscope; the app refreshes them every frame.
@fragment
fn gen_spectrum(in: VOut) -> @location(0) vec4f {
    let bars = max(P(0u), 1.0);
    let mirror = P(3u) > 0.5;
    let x = select(in.uv.x, abs(in.uv.x - 0.5) * 2.0, mirror);
    let i = floor(x * bars);
    let level = textureSampleLevel(tex1, samp, vec2f((i + 0.5) / bars, 0.5), 0.0).r;
    let h = clamp(level * P(1u), 0.0, 1.0);
    let y = 1.0 - in.uv.y;
    // A gap between bars, and a soft top.
    let gap = smoothstep(0.0, 0.08, fract(x * bars)) * (1.0 - smoothstep(0.92, 1.0, fract(x * bars)));
    let top = 1.0 - smoothstep(h - 0.01, h + 0.005, y);
    let v = gap * top;
    let col = hsv2rgb(vec3f(fract(P(2u) + x * 0.4), 0.85, 1.0)) * (0.35 + 0.65 * y / max(h, 0.001));
    return vec4f(col * v, v);
}

@fragment
fn gen_waveform(in: VOut) -> @location(0) vec4f {
    let s = textureSampleLevel(tex1, samp, vec2f(in.uv.x, 0.5), 0.0).r * 2.0 - 1.0;
    let y = 0.5 - s * P(1u) * 0.4;
    let d = abs(in.uv.y - y);
    let line = 1.0 - smoothstep(0.0, P(0u), d);
    let glow = P(3u) * exp(-d / (P(0u) * 4.0 + 0.001)) * 0.5;
    let v = clamp(line + glow, 0.0, 1.0);
    let col = hsv2rgb(vec3f(fract(P(2u) + in.uv.x * 0.2), 0.7, 1.0));
    return vec4f(col * v, v);
}
