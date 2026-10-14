// Layer compositing, YUV conversion, and the final output pass.

fn sample_source(st: vec2f) -> vec4f {
    if (u.yuv == 1u) {
        // BT.709, video range
        let y = (textureSampleLevel(tex0, samp, st, 0.0).r - 16.0 / 255.0) * (255.0 / 219.0);
        let c = (textureSampleLevel(tex2, samp, st, 0.0).rg - vec2f(128.0 / 255.0)) * (255.0 / 224.0);
        let rgb = vec3f(
            y + 1.5748 * c.y,
            y - 0.1873 * c.x - 0.4681 * c.y,
            y + 1.8556 * c.x,
        );
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
    let a = s.a * u.opacity;
    let blended = clamp(blend(dst.rgb, s.rgb, u.mode), vec3f(0.0), vec3f(1.0));
    return vec4f(mix(dst.rgb, blended, a), a + dst.a * (1.0 - a));
}

// Converts a YUV source to RGB so the effect chain can work on it.
@fragment
fn fs_to_rgb(in: VOut) -> @location(0) vec4f {
    return sample_source(in.uv);
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
    return vec4f(c.rgb * u.opacity, 1.0);
}
