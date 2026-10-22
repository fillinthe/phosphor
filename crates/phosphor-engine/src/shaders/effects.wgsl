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

// A face hidden inside an ellipse: X, Y (up = 1) and Size (its height, as a share of the picture's) come from face
// tracking; Found 0 = nothing to hide. Style: 0 pixelate, 1 blur, 2 black.
@fragment
fn fx_hide_face(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    if (P(3u) < 0.5) {
        return c;
    }
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    let d = (in.uv - vec2f(P(0u), 1.0 - P(1u))) * vec2f(aspect, 1.0);
    let r = max(P(2u), 0.02) * 0.65;
    let inside = 1.0 - smoothstep(r * 0.85, r, length(d));
    if (inside <= 0.0) {
        return c;
    }
    let style = u32(round(P(4u)));
    var hidden = vec4f(0.0, 0.0, 0.0, c.a);
    if (style == 0u) {
        let cell = vec2f(r * 2.0 / max(P(5u), 1.0)) / vec2f(aspect, 1.0);
        hidden = src((floor(in.uv / cell) + 0.5) * cell);
    } else if (style == 1u) {
        var sum = vec4f(0.0);
        let step = vec2f(r * 0.12) / vec2f(aspect, 1.0);
        for (var y = -3; y <= 3; y = y + 1) {
            for (var x = -3; x <= 3; x = x + 1) {
                sum = sum + src(in.uv + vec2f(f32(x), f32(y)) * step);
            }
        }
        hidden = sum / 49.0;
    }
    return mix(c, hidden, inside);
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

// ---------------------------------------------------------------------------------------------
// Blur family (separable passes share the same uniforms: P(0) = radius in pixels).

fn blur_dir(uv: vec2f, dir: vec2f, radius: f32) -> vec4f {
    let step = dir * max(radius, 0.01) / (4.0 * u.resolution);
    var w = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    var acc = src(uv) * w[0];
    for (var i = 1; i < 5; i = i + 1) {
        let o = step * f32(i);
        acc = acc + (src(uv + o) + src(uv - o)) * w[i];
    }
    return acc;
}

@fragment
fn fx_blur_h(in: VOut) -> @location(0) vec4f {
    return blur_dir(in.uv, vec2f(1.0, 0.0), P(0u));
}

@fragment
fn fx_blur_v(in: VOut) -> @location(0) vec4f {
    return blur_dir(in.uv, vec2f(0.0, 1.0), P(0u));
}

@fragment
fn fx_zoom_blur(in: VOut) -> @location(0) vec4f {
    let c = vec2f(P(1u), P(2u));
    var acc = vec4f(0.0);
    for (var i = 0; i < 16; i = i + 1) {
        let t = f32(i) / 15.0 * P(0u);
        acc = acc + src(mix(in.uv, c, t));
    }
    return acc / 16.0;
}

@fragment
fn fx_motion_blur(in: VOut) -> @location(0) vec4f {
    let a = P(1u) * TAU;
    let dir = vec2f(cos(a), sin(a) / aspect()) * P(0u);
    var acc = vec4f(0.0);
    for (var i = 0; i < 16; i = i + 1) {
        let t = f32(i) / 15.0 - 0.5;
        acc = acc + src(in.uv + dir * t);
    }
    return acc / 16.0;
}

// Glow = bright pass -> blur h -> blur v -> add to the dry input. P(0) radius, P(1) threshold, P(2) intensity.
@fragment
fn fx_bright(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let k = smoothstep(P(1u) - 0.1, P(1u) + 0.1, luma(c.rgb));
    return vec4f(c.rgb * k, c.a);
}

@fragment
fn fx_glow_add(in: VOut) -> @location(0) vec4f {
    let dry = textureSampleLevel(tex1, samp, in.uv, 0.0);
    let glow = src(in.uv).rgb * P(2u);
    return vec4f(min(dry.rgb + glow, vec3f(1.0)), dry.a);
}

// Trails / feedback: tex1 = this effect's previous output. P(0) decay, P(1) zoom, P(2) rotate, P(3) mode.
@fragment
fn fx_trails(in: VOut) -> @location(0) vec4f {
    let asp = aspect();
    var p = (in.uv - 0.5) * vec2f(asp, 1.0);
    let a = P(2u) * TAU;
    let cs = cos(a); let sn = sin(a);
    p = vec2f(p.x * cs - p.y * sn, p.x * sn + p.y * cs) / max(P(1u), 0.2);
    let puv = p / vec2f(asp, 1.0) + 0.5;
    var prev = textureSampleLevel(tex1, samp, puv, 0.0) * P(0u);
    if (any(puv < vec2f(0.0)) || any(puv > vec2f(1.0))) {
        prev = vec4f(0.0);
    }
    let cur = src(in.uv);
    let mode = u32(round(P(3u)));
    var out = max(cur, prev);
    if (mode == 1u) { out = min(cur + prev, vec4f(1.0)); }
    if (mode == 2u) { out = mix(cur, prev, 0.5 + 0.5 * P(0u)); }
    return vec4f(out.rgb, 1.0);
}

// ---------------------------------------------------------------------------------------------
// Film looks

@fragment
fn fx_technicolor(in: VOut) -> @location(0) vec4f {
    let c0 = src(in.uv);
    var c = c0.rgb;
    let l = luma(c);
    // Warm highlights, cyan shadows, pushed saturation and contrast: the two-strip feel.
    c = mix(c, c * vec3f(1.15, 0.95, 0.8), smoothstep(0.3, 0.9, l));
    c = mix(c, c * vec3f(0.8, 1.0, 1.2), smoothstep(0.6, 0.0, l));
    var hsv = rgb2hsv(clamp(c, vec3f(0.0), vec3f(1.0)));
    hsv.y = min(hsv.y * 1.5, 1.0);
    c = (hsv2rgb(hsv) - 0.5) * 1.15 + 0.5;
    return vec4f(mix(c0.rgb, clamp(c, vec3f(0.0), vec3f(1.0)), P(0u)), c0.a);
}

@fragment
fn fx_bleach(in: VOut) -> @location(0) vec4f {
    let c0 = src(in.uv);
    let c = c0.rgb;
    let lum = luma(c);
    let blend = vec3f(lum);
    let l = clamp(10.0 * (lum - 0.45), 0.0, 1.0);
    let r1 = 2.0 * c * blend;
    let r2 = 1.0 - 2.0 * (1.0 - blend) * (1.0 - c);
    let res = mix(r1, r2, l);
    return vec4f(mix(c, res, P(0u)), c0.a);
}

@fragment
fn fx_vignette(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let d = length((in.uv - 0.5) * vec2f(aspect(), 1.0));
    let v = smoothstep(P(0u), P(0u) - max(P(1u), 0.01), d);
    return vec4f(c.rgb * mix(1.0, v, P(2u)), c.a);
}

@fragment
fn fx_grain(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let n = hash21(in.uv * u.resolution + vec2f(fract(u.time * 7.0) * 100.0, fract(u.time * 3.0) * 50.0)) - 0.5;
    let flicker = 1.0 + (hash21(vec2f(floor(u.time * 24.0), 1.0)) - 0.5) * P(1u);
    let g = n * P(0u) * (1.0 - luma(c.rgb) * 0.6);
    return vec4f(clamp(c.rgb * flicker + g, vec3f(0.0), vec3f(1.0)), c.a);
}

@fragment
fn fx_crt(in: VOut) -> @location(0) vec4f {
    // Slight barrel distortion, scanlines, RGB aperture mask, corner darkening.
    var uv = in.uv - 0.5;
    uv = uv * (1.0 + P(2u) * 0.3 * dot(uv, uv));
    uv = uv + 0.5;
    var c = src(uv);
    if (any(uv < vec2f(0.0)) || any(uv > vec2f(1.0))) {
        c = vec4f(0.0, 0.0, 0.0, 1.0);
    }
    let line = 0.5 + 0.5 * sin(uv.y * u.resolution.y * 3.14159 * P(3u));
    let scan = mix(1.0, line, P(0u));
    let px = u32(floor(uv.x * u.resolution.x)) % 3u;
    var mask = vec3f(1.0);
    if (px == 0u) { mask = vec3f(1.0, 0.7, 0.7); }
    if (px == 1u) { mask = vec3f(0.7, 1.0, 0.7); }
    if (px == 2u) { mask = vec3f(0.7, 0.7, 1.0); }
    mask = mix(vec3f(1.0), mask, P(1u));
    let d = length((uv - 0.5) * vec2f(aspect(), 1.0));
    let vig = 1.0 - smoothstep(0.55, 0.95, d) * 0.6;
    return vec4f(c.rgb * scan * mask * vig * 1.1, 1.0);
}

@fragment
fn fx_glitch(in: VOut) -> @location(0) vec4f {
    let t = floor(u.time * P(2u));
    let band = floor(in.uv.y * P(1u));
    let r = hash21(vec2f(band, t));
    let hit = step(1.0 - P(3u), r);
    let shift = (hash21(vec2f(band + 7.0, t)) - 0.5) * P(0u) * hit;
    let uv = vec2f(in.uv.x + shift, in.uv.y);
    let split = P(0u) * 0.3 * hit;
    let rr = textureSampleLevel(tex0, samp_mirror, uv + vec2f(split, 0.0), 0.0).r;
    let gg = textureSampleLevel(tex0, samp_mirror, uv, 0.0).g;
    let bb = textureSampleLevel(tex0, samp_mirror, uv - vec2f(split, 0.0), 0.0).b;
    let noise = (hash21(vec2f(in.uv.y * 400.0, t)) - 0.5) * 0.3 * hit;
    let roll = step(0.97, hash21(vec2f(floor(in.uv.y * 200.0), t * 0.5))) * hit;
    return vec4f(clamp(vec3f(rr, gg, bb) + noise + roll, vec3f(0.0), vec3f(1.0)), 1.0);
}

@fragment
fn fx_halftone(in: VOut) -> @location(0) vec4f {
    let cell = max(P(0u), 2.0);
    let g = in.uv * u.resolution / cell;
    let center = (floor(g) + 0.5) * cell / u.resolution;
    let c = src(center);
    let l = luma(c.rgb);
    let d = length(fract(g) - 0.5);
    let radius = sqrt(1.0 - l) * 0.6;
    let aa = 1.5 / cell;
    let ink = smoothstep(radius + aa, radius - aa, d);
    let paper = mix(vec3f(1.0), c.rgb * 0.5 + 0.5, P(1u));
    let dot_col = mix(vec3f(0.05), c.rgb, P(1u));
    return vec4f(mix(paper, dot_col, ink), 1.0);
}

@fragment
fn fx_sharpen(in: VOut) -> @location(0) vec4f {
    let t = 1.0 / u.resolution;
    let c = src(in.uv);
    let blur = (src(in.uv + vec2f(t.x, 0.0)) + src(in.uv - vec2f(t.x, 0.0)) + src(in.uv + vec2f(0.0, t.y)) + src(in.uv - vec2f(0.0, t.y))) * 0.25;
    return vec4f(clamp(c.rgb + (c.rgb - blur.rgb) * P(0u), vec3f(0.0), vec3f(1.0)), c.a);
}

@fragment
fn fx_emboss(in: VOut) -> @location(0) vec4f {
    let a = P(1u) * TAU;
    let o = vec2f(cos(a), sin(a)) * P(2u) / u.resolution;
    let d = luma(src(in.uv + o).rgb) - luma(src(in.uv - o).rgb);
    let relief = clamp(0.5 + d * P(0u), 0.0, 1.0);
    let c = src(in.uv);
    return vec4f(mix(vec3f(relief), c.rgb * (0.5 + relief), P(3u)), c.a);
}

@fragment
fn fx_tile(in: VOut) -> @location(0) vec4f {
    let n = max(P(0u), 1.0);
    let g = in.uv * n;
    var f = fract(g);
    if (P(1u) > 0.5) {
        let odd = (vec2f(floor(g.x), floor(g.y)) % 2.0);
        f = mix(f, 1.0 - f, odd);
    }
    return src(f);
}

// ---------------------------------------------------------------------------------------------
// Keying, tone and time effects

// Chroma key: pixels near the key hue become transparent. P(0) hue, P(1) tolerance, P(2) softness.
@fragment
fn fx_chroma_key(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let hsv = rgb2hsv(c.rgb);
    let dh = min(abs(hsv.x - P(0u)), 1.0 - abs(hsv.x - P(0u)));
    let by_hue = smoothstep(P(1u), P(1u) + max(P(2u), 0.001), dh);
    let saturated = smoothstep(0.15, 0.4, hsv.y) * smoothstep(0.1, 0.3, hsv.z);
    let keep = max(by_hue, 1.0 - saturated);
    return vec4f(c.rgb, c.a * keep);
}

// Remove background: keeps what the matte on tex2 says is a person (Apple Vision, sent by the app). P(0) edge,
// P(1) softness, P(2) invert. p[14].w = 1 once a matte exists (until then the picture passes through).
@fragment
fn fx_cutout(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    var m = textureSampleLevel(tex2, samp, in.uv, 0.0).r;
    m = smoothstep(P(0u) - P(1u), P(0u) + P(1u) + 0.001, m);
    m = mix(m, 1.0 - m, step(0.5, P(2u)));
    return vec4f(c.rgb, c.a * mix(1.0, m, step(0.5, u.p[14].w)));
}

@fragment
fn fx_gamma(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    var v = pow(max(c.rgb, vec3f(0.0)), vec3f(1.0 / max(P(0u), 0.05)));
    v = (v + P(1u)) * P(2u);
    return vec4f(clamp(v, vec3f(0.0), vec3f(1.0)), c.a);
}

@fragment
fn fx_solarize(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let flipped = select(c.rgb, 1.0 - c.rgb, c.rgb > vec3f(P(0u)));
    return vec4f(mix(c.rgb, flipped, P(1u)), c.a);
}

// Slit-scan: columns left of the slit are yesterday's picture shifted by `shift` pixels, so time
// smears across the frame. tex1 = previous output. P(0) slit, P(1) shift px, P(2) vertical.
@fragment
fn fx_slitscan(in: VOut) -> @location(0) vec4f {
    let vertical = P(2u) > 0.5;
    let coord = select(in.uv.x, in.uv.y, vertical);
    let step = P(1u) / select(u.resolution.x, u.resolution.y, vertical);
    let off = select(vec2f(step, 0.0), vec2f(0.0, step), vertical);
    let prev = textureSampleLevel(tex1, samp, in.uv + off, 0.0);
    let cur = src(in.uv);
    return select(cur, prev, coord < P(0u));
}

// Edge glow = coloured edges -> blur -> add. P(0) radius, P(1) edge strength, P(2) intensity.
@fragment
fn fx_edges_only(in: VOut) -> @location(0) vec4f {
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
    let e = clamp(length(vec2f(gx, gy)) * P(1u), 0.0, 1.0);
    let c = src(in.uv);
    let tint = mix(vec3f(1.0), normalize(c.rgb + 0.05), 0.7);
    return vec4f(tint * e, 1.0);
}

// Motion flow (two passes). fx_flow_keep copies this frame's picture into the history slot; fx_flow then has the
// picture on tex0 and the previous one on tex1 and estimates where each pixel moved (normal optical flow from the
// brightness gradient and the change between frames), then pushes the picture along that motion.
@fragment
fn fx_flow_keep(in: VOut) -> @location(0) vec4f {
    return src(in.uv);
}

fn flow_now(uv: vec2f) -> f32 { return luma(textureSampleLevel(tex0, samp, uv, 0.0).rgb); }
fn flow_prev(uv: vec2f) -> f32 { return luma(textureSampleLevel(tex1, samp, uv, 0.0).rgb); }

@fragment
fn fx_flow(in: VOut) -> @location(0) vec4f {
    let px = vec2f(P(1u)) / u.resolution;
    let c = in.uv;
    let dx = vec2f(px.x, 0.0);
    let dy = vec2f(0.0, px.y);
    // Gradients from the average of both frames (steadier than one frame's), change over time from the difference.
    let gx = ((flow_now(c + dx) - flow_now(c - dx)) + (flow_prev(c + dx) - flow_prev(c - dx))) * 0.5;
    let gy = ((flow_now(c + dy) - flow_now(c - dy)) + (flow_prev(c + dy) - flow_prev(c - dy))) * 0.5;
    let it = flow_now(c) - flow_prev(c);
    let g = vec2f(gx, gy);
    let g2 = dot(g, g);
    // Normal flow (in units of the gradient step), capped so flat areas don't explode.
    var flow = -it * g / (g2 + 0.0025);
    let m = length(flow);
    if (m > 3.0) { flow = flow * (3.0 / m); }
    let mode = u32(round(P(2u)));
    if (mode == 1u) {
        // The motion itself: hue = direction, brightness = speed.
        let ang = atan2(flow.y, flow.x) / TAU + 0.5;
        let col = hsv2rgb(vec3f(ang, 0.9, clamp(length(flow) * P(0u) * 0.7, 0.0, 1.0)));
        return vec4f(col, 1.0);
    }
    if (mode == 2u) {
        // Only what moves.
        let a = clamp(length(flow) * P(0u) * 0.7, 0.0, 1.0);
        return src(in.uv) * a;
    }
    // Push the picture along its motion.
    let d = flow * px * P(0u) * 4.0;
    return textureSampleLevel(tex0, samp, clamp(c - d, vec2f(0.0), vec2f(1.0)), 0.0);
}
