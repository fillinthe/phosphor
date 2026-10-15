// Layer compositing, YUV conversion, and the final output pass.

fn sample_source(st: vec2f) -> vec4f {
    if (u.yuv == 1u || u.yuv == 3u) {
        // BT.709; 1 = TV range (16–235), 3 = full range (0–255). 10-bit planes normalise the same way.
        var y = textureSampleLevel(tex0, samp, st, 0.0).r;
        var c = textureSampleLevel(tex2, samp, st, 0.0).rg - vec2f(128.0 / 255.0);
        if (u.yuv == 1u) {
            y = (y - 16.0 / 255.0) * (255.0 / 219.0);
            c = c * (255.0 / 224.0);
        }
        let rgb = vec3f(
            y + 1.5748 * c.y,
            y - 0.1873 * c.x - 0.4681 * c.y,
            y + 1.8556 * c.x,
        );
        return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), 1.0);
    }
    if (u.yuv == 2u) {
        // Hap Q: scaled YCoCg in DXT5 (r = Co, g = Cg, b = scale, a = Y), per the Hap reference shader.
        let c = textureSampleLevel(tex0, samp, st, 0.0) + vec4f(-128.0 / 255.0, -128.0 / 255.0, 0.0, 0.0);
        let scale = c.b * (255.0 / 8.0) + 1.0;
        let co = c.r / scale;
        let cg = c.g / scale;
        let rgb = vec3f(c.a + co - cg, c.a + cg, c.a - co - cg);
        return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), 1.0);
    }
    return textureSampleLevel(tex0, samp, st, 0.0);
}

fn blend(b: vec3f, s: vec3f, mode: u32) -> vec3f {
    switch mode {
        case 1u: { return b + s; }
        case 2u: { return b * s; }
        case 3u: { return 1.0 - (1.0 - b) * (1.0 - s); }
        case 4u: { return mix(2.0 * b * s, 1.0 - 2.0 * (1.0 - b) * (1.0 - s), step(vec3f(0.5), b)); }
        case 5u: { return abs(b - s); }
        case 6u: { return max(b, s); }
        case 7u: { return min(b, s); }
        case 8u: { return max(b - s, vec3f(0.0)); }
        case 9u: { return b + s - 2.0 * b * s; }
        default: { return s; }
    }
}

// tex1 = accumulated layers below, tex0 (+ tex2) = this layer.
@fragment
fn fs_composite(in: VOut) -> @location(0) vec4f {
    let dst = textureSampleLevel(tex1, samp, in.uv, 0.0);
    let st = in.uv * u.uv_xform.xy + u.uv_xform.zw;
    var s = sample_source(st);
    if (any(st < vec2f(0.0)) || any(st > vec2f(1.0))) {
        s = vec4f(0.0);
    }
    if (u.mode >= 10u) {
        // Track matte: this layer's brightness / alpha keeps or cuts what's below; opacity = how much.
        var m = dot(s.rgb, vec3f(0.2126, 0.7152, 0.0722));
        if (u.mode == 11u) { m = 1.0 - m; }
        if (u.mode == 12u) { m = s.a; }
        let k = mix(1.0, clamp(m, 0.0, 1.0), u.opacity);
        return vec4f(dst.rgb * k, dst.a * k);
    }
    let a = s.a * u.opacity;
    let blended = clamp(blend(dst.rgb, s.rgb, u.mode), vec3f(0.0), vec3f(1.0));
    return vec4f(mix(dst.rgb, blended, a), a + dst.a * (1.0 - a));
}

// Converts a YUV source to RGB so the effect chain can work on it.
@fragment
fn fs_to_rgb(in: VOut) -> @location(0) vec4f {
    return sample_source(in.uv);
}

fn fitted(uv: vec2f, xf: vec4f, which: u32) -> vec4f {
    let st = uv * xf.xy + xf.zw;
    var c: vec4f;
    if (which == 0u) {
        c = textureSampleLevel(tex0, samp, st, 0.0);
    } else {
        c = textureSampleLevel(tex1, samp, st, 0.0);
    }
    if (any(st < vec2f(0.0)) || any(st > vec2f(1.0))) {
        c = vec4f(0.0);
    }
    return c;
}

// Clip transition: tex0 = incoming (fit in uv_xform), tex1 = outgoing (fit in p[0]),
// opacity = progress, mode = kind (0 crossfade, 1 dip to black, 2 wipe, 3 blend with mode p[1].x).
@fragment
fn fs_crossfade(in: VOut) -> @location(0) vec4f {
    let a = fitted(in.uv, u.uv_xform, 0u);
    let b = fitted(in.uv, u.p[0], 1u);
    let t = u.opacity;
    if (u.mode == 1u) {
        if (t < 0.5) {
            return b * (1.0 - t * 2.0);
        }
        return a * (t * 2.0 - 1.0);
    }
    if (u.mode == 2u) {
        let edge = smoothstep(t - 0.015, t + 0.015, in.uv.x);
        return mix(a, b, edge);
    }
    if (u.mode == 3u) {
        // First half: the new source blends in over the old; second half: the old one fades away.
        let both = vec4f(clamp(blend(b.rgb, a.rgb, u32(u.p[1].x)), vec3f(0.0), vec3f(1.0)), max(a.a, b.a));
        if (t < 0.5) {
            return mix(b, both, t * 2.0);
        }
        return mix(both, a, t * 2.0 - 1.0);
    }
    return mix(b, a, t);
}

// Dry/wet: tex0 = effect output, tex1 = effect input, opacity = mix.
@fragment
fn fs_mix(in: VOut) -> @location(0) vec4f {
    let wet = textureSampleLevel(tex0, samp, in.uv, 0.0);
    let dry = textureSampleLevel(tex1, samp, in.uv, 0.0);
    return mix(dry, wet, u.opacity);
}

// Accumulator -> output texture, applying the master fader.
@fragment
fn fs_finalize(in: VOut) -> @location(0) vec4f {
    let c = textureSampleLevel(tex0, samp, in.uv, 0.0);
    return vec4f(grade(c.rgb) * u.opacity, 1.0);
}
