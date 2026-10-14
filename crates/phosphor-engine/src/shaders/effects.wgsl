// Built-in effects. Input: tex0. Parameters: P(0..15). See shaders.rs for ranges.

@fragment
fn fx_invert(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    return vec4f(mix(c.rgb, 1.0 - c.rgb, P(0u)), c.a);
}

@fragment
fn fx_color_adjust(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    var hsv = rgb2hsv(c.rgb);
    hsv.x = fract(hsv.x + P(0u));
    hsv.y = hsv.y * P(1u);
    var rgb = hsv2rgb(hsv);
    rgb = (rgb - 0.5) * P(3u) + 0.5 + P(2u);
    return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), c.a);
}

@fragment
fn fx_rgb_shift(in: VOut) -> @location(0) vec4f {
    let a = P(1u) * TAU;
    let off = vec2f(cos(a), sin(a)) * P(0u);
    let r = src(in.uv + off).r;
    let g = src(in.uv);
    let b = src(in.uv - off).b;
    return vec4f(r, g.g, b, g.a);
}

@fragment
fn fx_kaleidoscope(in: VOut) -> @location(0) vec4f {
    let segments = max(P(0u), 2.0);
    let asp = aspect();
    var p = in.uv - 0.5;
    p.x = p.x * asp;
    let r = length(p) / P(2u);
    var a = atan2(p.y, p.x) + P(1u) * TAU;
    let seg = TAU / segments;
    a = a - seg * floor(a / seg);
    a = abs(a - seg * 0.5);
    var q = vec2f(cos(a), sin(a)) * r;
    q.x = q.x / asp;
    return textureSampleLevel(tex0, samp_mirror, q + 0.5, 0.0);
}

@fragment
fn fx_pixelate(in: VOut) -> @location(0) vec4f {
    let cell = max(P(0u), 1.0) / u.resolution;
    return src((floor(in.uv / cell) + 0.5) * cell);
}

@fragment
fn fx_mirror(in: VOut) -> @location(0) vec4f {
    let mode = u32(round(P(0u)));
    var uv = in.uv;
    if (mode == 0u || mode == 2u) {
        uv.x = select(uv.x, 1.0 - uv.x, uv.x > 0.5);
    }
    if (mode == 1u || mode == 2u) {
        uv.y = select(uv.y, 1.0 - uv.y, uv.y > 0.5);
    }
    return src(uv);
}

@fragment
fn fx_posterize(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let levels = max(P(0u), 2.0) - 1.0;
    return vec4f(floor(c.rgb * levels + 0.5) / levels, c.a);
}

@fragment
fn fx_edges(in: VOut) -> @location(0) vec4f {
    let t = 1.0 / u.resolution;
    let tl = luma(src(in.uv + vec2f(-t.x, -t.y)).rgb);
    let tc = luma(src(in.uv + vec2f(0.0, -t.y)).rgb);
    let tr = luma(src(in.uv + vec2f(t.x, -t.y)).rgb);
    let ml = luma(src(in.uv + vec2f(-t.x, 0.0)).rgb);
    let mr = luma(src(in.uv + vec2f(t.x, 0.0)).rgb);
    let bl = luma(src(in.uv + vec2f(-t.x, t.y)).rgb);
    let bc = luma(src(in.uv + vec2f(0.0, t.y)).rgb);
    let br = luma(src(in.uv + vec2f(t.x, t.y)).rgb);
    let gx = -tl - 2.0 * ml - bl + tr + 2.0 * mr + br;
    let gy = -tl - 2.0 * tc - tr + bl + 2.0 * bc + br;
    let e = clamp(length(vec2f(gx, gy)) * P(0u), 0.0, 1.0);
    let c = src(in.uv);
    return vec4f(mix(c.rgb, vec3f(e), P(1u)), c.a);
}

@fragment
fn fx_zoom_rotate(in: VOut) -> @location(0) vec4f {
    let asp = aspect();
    var p = in.uv - 0.5;
    p.x = p.x * asp;
    let a = (P(1u) + P(2u) * u.time) * TAU * 0.5;
    let cs = cos(a);
    let sn = sin(a);
    p = vec2f(p.x * cs - p.y * sn, p.x * sn + p.y * cs) / P(0u);
    p.x = p.x / asp;
    return textureSampleLevel(tex0, samp_mirror, p + 0.5, 0.0);
}

@fragment
fn fx_wave(in: VOut) -> @location(0) vec4f {
    var uv = in.uv;
    uv.x = uv.x + sin(uv.y * P(1u) + u.time * P(2u)) * P(0u);
    return textureSampleLevel(tex0, samp_mirror, uv, 0.0);
}

@fragment
fn fx_threshold(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let v = smoothstep(P(0u) - P(1u), P(0u) + P(1u) + 1e-4, luma(c.rgb));
    return vec4f(vec3f(v), c.a);
}

@fragment
fn fx_strobe(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let on = fract(u.time * P(0u)) < P(1u);
    return select(vec4f(0.0, 0.0, 0.0, c.a), c, on);
}
