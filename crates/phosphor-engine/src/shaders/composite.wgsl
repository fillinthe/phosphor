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
    if (u.yuv == 5u) {
        // A, Y, Cb, Cr (VideoToolbox 'y416', ProRes 4444): BT.709 TV range, full-range alpha, straight like RGBA.
        let v = textureSampleLevel(tex0, samp, st, 0.0);
        let y = (v.g - 16.0 / 255.0) * (255.0 / 219.0);
        let c = (vec2f(v.b, v.a) - vec2f(128.0 / 255.0)) * (255.0 / 224.0);
        let rgb = vec3f(
            y + 1.5748 * c.y,
            y - 0.1873 * c.x - 0.4681 * c.y,
            y + 1.8556 * c.x,
        );
        return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), v.r);
    }
    if (u.yuv == 2u || u.yuv == 4u) {
        // Hap Q: scaled YCoCg in DXT5 (r = Co, g = Cg, b = scale, a = Y), per the Hap reference shader.
        // Hap Q Alpha (4): the alpha plane is a BC4 texture on tex2.
        let c = textureSampleLevel(tex0, samp, st, 0.0) + vec4f(-128.0 / 255.0, -128.0 / 255.0, 0.0, 0.0);
        let scale = c.b * (255.0 / 8.0) + 1.0;
        let co = c.r / scale;
        let cg = c.g / scale;
        let rgb = vec3f(c.a + co - cg, c.a + cg, c.a - co - cg);
        var a = 1.0;
        if (u.yuv == 4u) { a = textureSampleLevel(tex2, samp, st, 0.0).r; }
        // Straight alpha, like every other source (compositing multiplies by alpha itself; multiplying here too
        // darkened soft edges).
        return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), a);
    }
    return textureSampleLevel(tex0, samp, st, 0.0);
}

// Brightness and saturation as the Hue / Saturation / Colour / Luminosity modes use them (W3C compositing spec).
fn blend_lum(c: vec3f) -> f32 { return dot(c, vec3f(0.3, 0.59, 0.11)); }
fn blend_set_lum(c: vec3f, l: f32) -> vec3f {
    let d = c + vec3f(l - blend_lum(c));
    let y = blend_lum(d);
    let n = min(d.r, min(d.g, d.b));
    let x = max(d.r, max(d.g, d.b));
    var o = d;
    if (n < 0.0) { o = vec3f(y) + (o - vec3f(y)) * y / max(y - n, 1e-6); }
    if (x > 1.0) { o = vec3f(y) + (o - vec3f(y)) * (1.0 - y) / max(x - y, 1e-6); }
    return o;
}
fn blend_sat(c: vec3f) -> f32 { return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b)); }
fn blend_set_sat(c: vec3f, s: f32) -> vec3f {
    let n = min(c.r, min(c.g, c.b));
    let range = max(c.r, max(c.g, c.b)) - n;
    if (range <= 1e-6) { return vec3f(0.0); }
    return (c - vec3f(n)) * s / range;
}
fn color_burn(b: vec3f, s: vec3f) -> vec3f { return select(1.0 - min(vec3f(1.0), (1.0 - b) / max(s, vec3f(1e-6))), vec3f(1.0), b >= vec3f(1.0)); }
fn color_dodge(b: vec3f, s: vec3f) -> vec3f { return select(min(vec3f(1.0), b / max(1.0 - s, vec3f(1e-6))), vec3f(0.0), b <= vec3f(0.0)); }

fn blend(b: vec3f, s: vec3f, mode: u32) -> vec3f {
    let low = s <= vec3f(0.5);
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
        case 13u: {
            // Soft light (W3C).
            let d = select(sqrt(b), ((16.0 * b - 12.0) * b + 4.0) * b, b <= vec3f(0.25));
            return select(b + (2.0 * s - 1.0) * (d - b), b - (1.0 - 2.0 * s) * b * (1.0 - b), low);
        }
        case 14u: { return select(1.0 - (1.0 - b) * (1.0 - (2.0 * s - 1.0)), b * 2.0 * s, low); }
        case 15u: { return select(color_dodge(b, 2.0 * s - 1.0), color_burn(b, 2.0 * s), low); }
        case 16u: { return b + 2.0 * s - 1.0; }
        case 17u: { return select(max(b, 2.0 * s - 1.0), min(b, 2.0 * s), low); }
        case 18u: { return step(vec3f(1.0), b + s); }
        case 19u: { return color_burn(b, s); }
        case 20u: { return color_dodge(b, s); }
        case 21u: { return min(vec3f(1.0), b / max(s, vec3f(1e-6))); }
        case 22u: { return blend_set_lum(blend_set_sat(s, blend_sat(b)), blend_lum(b)); }
        case 23u: { return blend_set_lum(blend_set_sat(b, blend_sat(s)), blend_lum(b)); }
        case 24u: { return blend_set_lum(s, blend_lum(b)); }
        case 25u: { return blend_set_lum(b, blend_lum(s)); }
        default: { return s; }
    }
}

// tex1 = accumulated layers below, tex0 (+ tex2) = this layer.
@fragment
fn fs_composite(in: VOut) -> @location(0) vec4f {
    let dst = textureSampleLevel(tex1, samp, in.uv, 0.0);
    // Corner pin (u.p[14].w = on): the inverse homography in p[12..14] takes this pixel back into the picture,
    // which fills the quad. Otherwise the layer transform (u.p[10].w = on): p[9] = (x, y offset, x scale, y scale),
    // p[10] = (cos, sin, aspect).
    var uv = in.uv;
    let pinned = u.p[14].w > 0.5;
    if (pinned) {
        let h = mat3x3f(
            vec3f(u.p[12].x, u.p[12].w, u.p[13].z),
            vec3f(u.p[12].y, u.p[13].x, u.p[13].w),
            vec3f(u.p[12].z, u.p[13].y, u.p[14].x),
        );
        let r = h * vec3f(in.pos.xy / u.resolution, 1.0);
        uv = r.xy / r.z;
    } else if (u.p[10].w > 0.5) {
        var q = uv - vec2f(0.5) - u.p[9].xy;
        q.x = q.x * u.p[10].z;
        q = vec2f(u.p[10].x * q.x + u.p[10].y * q.y, -u.p[10].y * q.x + u.p[10].x * q.y);
        q.x = q.x / u.p[10].z;
        uv = q / u.p[9].zw + vec2f(0.5);
    }
    let st = select(uv * u.uv_xform.xy + u.uv_xform.zw, uv, pinned);
    var s = sample_source(st);
    // Outside the picture, or cropped off (p[11] = left, right, top, bottom).
    let c = u.p[11];
    if (any(st < vec2f(c.x, c.z)) || any(st > vec2f(1.0 - c.y, 1.0 - c.w))) {
        s = vec4f(0.0);
    }
    if (u.mode >= 10u && u.mode <= 12u) {
        // Track matte: this layer's brightness / alpha keeps or cuts what's below; opacity = how much.
        var m = dot(s.rgb, vec3f(0.2126, 0.7152, 0.0722));
        if (u.mode == 11u) { m = 1.0 - m; }
        if (u.mode == 12u) { m = s.a; }
        let k = mix(1.0, clamp(m, 0.0, 1.0), u.opacity);
        return vec4f(dst.rgb * k, dst.a * k);
    }
    // Wipes (26–31): opacity is how far the layer has come in, by a shape; where it's in, it's fully there.
    if (u.mode >= 26u && u.mode <= 31u) {
        let q = in.pos.xy / u.resolution;
        var m = q.x;
        var soft = 0.04;
        switch u.mode {
            case 27u: { m = q.y; }
            case 28u: { m = length((q - vec2f(0.5)) * vec2f(u.resolution.x / max(u.resolution.y, 1.0), 1.0)) / 0.9; }
            case 29u: { m = (abs(q.x - 0.5) + abs(q.y - 0.5)); }
            case 30u: { m = hash21(floor(q * vec2f(16.0, 9.0))); soft = 0.001; }
            case 31u: { m = hash21(floor(in.pos.xy)); soft = 0.001; }
            default: {}
        }
        let shown = clamp((u.opacity * (1.0 + soft) - m) / soft, 0.0, 1.0);
        let wa = s.a * shown;
        return vec4f(mix(dst.rgb, s.rgb, wa), wa + dst.a * (1.0 - wa));
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
    // An effect with a blend mode: its result blended onto the dry picture, then dry / wet.
    if (u.mode != 0u) {
        let blended = clamp(blend(dry.rgb, wet.rgb, u.mode), vec3f(0.0), vec3f(1.0));
        return vec4f(mix(dry.rgb, blended, u.opacity * wet.a), dry.a);
    }
    return mix(dry, wet, u.opacity);
}

// LED / DMX pixel mapping: tex1 holds one point per pixel (x, y as 16-bit, packed in RGBA8);
// sample the program (tex0) there.
@fragment
fn fs_sample(in: VOut) -> @location(0) vec4f {
    let q = textureLoad(tex1, vec2i(in.pos.xy), 0) * 255.0;
    let uv = vec2f(round(q.r) * 256.0 + round(q.g), round(q.b) * 256.0 + round(q.a)) / 65535.0;
    return textureSampleLevel(tex0, samp, uv, 0.0);
}

// Accumulator -> output texture, applying the master fader.
@fragment
fn fs_finalize(in: VOut) -> @location(0) vec4f {
    let c = textureSampleLevel(tex0, samp, in.uv, 0.0);
    return vec4f(grade(c.rgb) * u.opacity, 1.0);
}
