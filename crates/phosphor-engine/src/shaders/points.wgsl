// Points: a "point picture" holds one point per texel: its place in rgb (x right, y up, z toward the viewer; a
// shape of size 1 fills -0.5..0.5) and its brightness in alpha (0 = not drawn). The engine keeps these pictures
// 16-bit float (`LayerState::is_data`). Shape makes them, Relief turns a picture into them, Move and Particles
// change them, and Dots / Lines / Mesh draw them with a camera and a light.
//
// The renders read the points as a grid of columns x rows (u.p[14].zw; u.p[14].xy = the point picture's size) with
// textureLoad, so no two points blend into one: a shape made at any size draws the same. "Listed" (P(13) = 1)
// reads every texel in order instead (a model's vertices, three to a triangle).

fn pt_hash3(p: vec3f) -> vec3f {
    let q = vec3f(dot(p, vec3f(127.1, 311.7, 74.7)), dot(p, vec3f(269.5, 183.3, 246.1)), dot(p, vec3f(113.5, 271.9, 124.6)));
    return fract(sin(q) * 43758.5453);
}

// Smooth 3D noise, each part -0.5..0.5.
fn pt_noise3(p: vec3f) -> vec3f {
    let i = floor(p);
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    let c000 = pt_hash3(i);
    let c100 = pt_hash3(i + vec3f(1.0, 0.0, 0.0));
    let c010 = pt_hash3(i + vec3f(0.0, 1.0, 0.0));
    let c110 = pt_hash3(i + vec3f(1.0, 1.0, 0.0));
    let c001 = pt_hash3(i + vec3f(0.0, 0.0, 1.0));
    let c101 = pt_hash3(i + vec3f(1.0, 0.0, 1.0));
    let c011 = pt_hash3(i + vec3f(0.0, 1.0, 1.0));
    let c111 = pt_hash3(i + vec3f(1.0, 1.0, 1.0));
    let x00 = mix(c000, c100, w.x);
    let x10 = mix(c010, c110, w.x);
    let x01 = mix(c001, c101, w.x);
    let x11 = mix(c011, c111, w.x);
    let y0 = mix(x00, x10, w.y);
    let y1 = mix(x01, x11, w.y);
    return mix(y0, y1, w.z) - 0.5;
}

// Turned around x, then y, then z (in turns: 1 = all the way round).
fn pt_turn(p: vec3f, t: vec3f) -> vec3f {
    let a = t * TAU;
    let cx = cos(a.x); let sx = sin(a.x);
    var q = vec3f(p.x, p.y * cx - p.z * sx, p.y * sx + p.z * cx);
    let cy = cos(a.y); let sy = sin(a.y);
    q = vec3f(q.x * cy + q.z * sy, q.y, -q.x * sy + q.z * cy);
    let cz = cos(a.z); let sz = sin(a.z);
    return vec3f(q.x * cz - q.y * sz, q.x * sz + q.y * cz, q.z);
}

// ---- Shape: points on a grid, sphere, cube, ring, helix or at random ----------------------------------------
//   P(0) shape  P(1) size  P(2) scatter  P(3) seed

@fragment
fn gen_pts_shape(in: VOut) -> @location(0) vec4f {
    let uv = in.uv;
    let shape = u32(P(0u) + 0.5);
    let size = P(1u);
    let seed = vec3f(P(3u), P(3u) * 0.37, P(3u) * 0.71);
    var p = vec3f(0.0);
    if (shape == 1u) {
        // Sphere, evenly covered.
        let lon = uv.x * TAU;
        let lat = acos(1.0 - 2.0 * uv.y);
        p = 0.5 * vec3f(sin(lat) * cos(lon), cos(lat), sin(lat) * sin(lon));
    } else if (shape == 2u) {
        // Cube: six faces, a strip each.
        let face = u32(uv.x * 6.0);
        let a = fract(uv.x * 6.0) - 0.5;
        let b = uv.y - 0.5;
        switch face {
            case 0u: { p = vec3f(0.5, a, b); }
            case 1u: { p = vec3f(-0.5, a, b); }
            case 2u: { p = vec3f(a, 0.5, b); }
            case 3u: { p = vec3f(a, -0.5, b); }
            case 4u: { p = vec3f(a, b, 0.5); }
            default: { p = vec3f(a, b, -0.5); }
        }
    } else if (shape == 3u) {
        // Ring (a torus).
        let big = uv.x * TAU;
        let small = uv.y * TAU;
        let r = 0.36 + 0.14 * cos(small);
        p = vec3f(r * cos(big), 0.14 * sin(small), r * sin(big));
    } else if (shape == 4u) {
        // Helix: four turns top to bottom, a little thickness across.
        let ang = uv.y * TAU * 4.0;
        let r = 0.35 + (uv.x - 0.5) * 0.12;
        p = vec3f(r * cos(ang), 0.5 - uv.y, r * sin(ang));
    } else if (shape == 5u) {
        // Random, filling the cube.
        p = pt_hash3(vec3f(uv * 517.0, 3.0) + seed) - 0.5;
    } else {
        // Grid: a flat sheet facing the viewer.
        p = vec3f(uv.x - 0.5, 0.5 - uv.y, 0.0);
    }
    p = p + (pt_hash3(vec3f(uv * 911.0, 7.0) + seed) - 0.5) * P(2u) * 0.5;
    return vec4f(p * size, 1.0);
}

// ---- Relief: a picture as points, raised by brightness ----------------------------------------------------
//   P(0) depth  P(1) threshold (darker points vanish)  P(2) colour as place (rgb -> xyz)

@fragment
fn fx_pts_relief(in: VOut) -> @location(0) vec4f {
    let c = src(in.uv);
    let l = luma(c.rgb);
    let asp = aspect();
    var p = vec3f((in.uv.x - 0.5) * asp, 0.5 - in.uv.y, l * P(0u));
    if (P(2u) > 0.5) {
        p = (c.rgb - 0.5) * vec3f(asp, 1.0, 1.0);
    }
    let bright = select(l, 0.0, l < P(1u));
    return vec4f(p, bright);
}

// ---- Move: moved, turned, sized and wobbled -----------------------------------------------------------------
//   P(0..2) move x y z  P(3..5) turn x y z  P(6) size  P(7) wobble  P(8) wobble size  P(9) wobble speed

@fragment
fn fx_pts_move(in: VOut) -> @location(0) vec4f {
    let d = src(in.uv);
    var p = pt_turn(d.xyz * P(6u), vec3f(P(3u), P(4u), P(5u))) + vec3f(P(0u), P(1u), P(2u));
    if (P(7u) > 0.0) {
        p = p + pt_noise3(p * P(8u) + vec3f(0.0, 0.0, u.time * P(9u))) * P(7u) * 0.5;
    }
    return vec4f(p, d.a);
}

// ---- Particles: every point is born where its source point is, drifts, fades, and is born again --------------
//   P(0) speed  P(1) gravity  P(2) wind  P(3) swirl  P(4) life (seconds)  P(5) spread
//   tex0 = the source points, tex1 = last frame's particles (place in rgb, life left 0..1 in alpha).

@fragment
fn fx_pts_particles(in: VOut) -> @location(0) vec4f {
    let born = src(in.uv);
    let last = textureSampleLevel(tex1, samp, in.uv, 0.0);
    // This frame's step (u.p[14].x from the engine; a frame at 60 when it has none).
    let dt = select(1.0 / 60.0, u.p[14].x, u.p[14].x > 0.0);
    let life = max(P(4u), 0.05);
    let id = vec3f(in.uv * 977.0, 11.0);
    var p = last.xyz;
    var left = last.a - dt / life;
    if (last.a <= 0.0 && all(last.xyz == vec3f(0.0))) {
        // The first frame: start part-way through, so they don't all die together.
        left = pt_hash3(id).x;
        p = born.xyz + (pt_hash3(id + 5.0) - 0.5) * P(5u);
    } else if (left <= 0.0) {
        // Born again where the source point is now (a source point that isn't drawn makes none).
        left = select(1.0, 0.0, born.a <= 0.0);
        p = born.xyz + (pt_hash3(id + u.time) - 0.5) * P(5u);
    } else {
        let flow = pt_noise3(p * 3.0 + vec3f(0.0, u.time * 0.2, 0.0)) * P(3u) * 2.0;
        let v = vec3f(P(2u), P(1u), 0.0) + flow + (pt_hash3(id + 9.0) - 0.5) * 0.3;
        p = p + v * P(0u) * dt;
    }
    return vec4f(p, max(left, 0.0));
}

// ---- The renders: dots, lines and a mesh, through a camera, with a light ------------------------------------
//   P(0) points  P(1) size  P(2) brightness  P(3) distance  P(4) lens  P(5..7) turn x y z  P(8) from the picture
//   P(9) tint  P(10) shade  P(11) fog  P(12) (each its own)  P(13) listed
//   tex0 = the points, tex2 = a picture the points take their colour from (sampled where each point is in the grid).

struct PtOut {
    @builtin(position) pos: vec4f,
    @location(0) color: vec4f,
    @location(1) corner: vec2f,
};

fn pt_grid() -> vec2u {
    return vec2u(max(u32(u.p[14].z), 1u), max(u32(u.p[14].w), 1u));
}

// Point (c, r) of the grid: which texel it is.
fn pt_texel(c: u32, r: u32) -> vec2i {
    let size = vec2u(max(u32(u.p[14].x), 1u), max(u32(u.p[14].y), 1u));
    if (P(13u) > 0.5) {
        return vec2i(i32(min(c, size.x - 1u)), i32(min(r, size.y - 1u)));
    }
    let g = pt_grid();
    return vec2i(i32(((c * 2u + 1u) * size.x) / (g.x * 2u)), i32(((r * 2u + 1u) * size.y) / (g.y * 2u)));
}

fn pt_load(c: u32, r: u32) -> vec4f {
    return textureLoad(tex0, pt_texel(c, r), 0);
}

// Where (c, r) sits in the grid, 0..1 (for the picture's colour).
fn pt_uv(c: u32, r: u32) -> vec2f {
    let g = vec2f(pt_grid());
    return vec2f((f32(c) + 0.5) / g.x, (f32(r) + 0.5) / g.y);
}

fn pt_view(p: vec3f) -> vec3f {
    return pt_turn(p, vec3f(P(5u), P(6u), P(7u)));
}

// The camera: on the z axis at P(3) looking at the middle, with a lens of P(4) degrees.
fn pt_clip(v: vec3f) -> vec4f {
    let f = 1.0 / tan(radians(max(P(4u), 1.0)) * 0.5);
    let depth = max(P(3u) - v.z, 0.05);
    // Depth 0..1 out to a little past the camera's distance (the mesh's depth buffer).
    let z = clamp(depth / (P(3u) + 1.5), 0.0, 0.999);
    return vec4f(v.x * f / aspect(), v.y * f, z * depth, depth);
}

// A point's colour: its brightness, the tint or the picture, lit by the surface it's on, fading into the fog.
fn pt_colour(d: vec4f, v: vec3f, n: vec3f, uv: vec2f) -> vec4f {
    var c = vec3f(1.0);
    if (P(9u) > 0.0) {
        c = hsv2rgb(vec3f(P(9u), 0.75, 1.0));
    }
    let pic = textureSampleLevel(tex2, samp, uv, 0.0);
    c = mix(c, pic.rgb, P(8u));
    let light = normalize(vec3f(-0.5, 0.6, 0.65));
    let shade = mix(1.0, 0.15 + 0.95 * abs(dot(n, light)), P(10u));
    let depth = P(3u) - v.z;
    let fog = 1.0 - P(11u) * clamp((depth - P(3u) + 0.5) * 0.8, 0.0, 1.0);
    return vec4f(c * P(2u) * shade * fog * max(d.a, 0.0), 1.0);
}

// The surface's direction at grid point (c, r): from its neighbours across and down (listed: its triangle).
fn pt_normal(c: u32, r: u32, v: vec3f) -> vec3f {
    let g = pt_grid();
    var b: vec3f;
    var d: vec3f;
    if (P(13u) > 0.5) {
        let i = r * g.x + c;
        let t = i - i % 3u;
        b = pt_view(pt_load((t + 1u) % g.x, (t + 1u) / g.x).xyz);
        d = pt_view(pt_load((t + 2u) % g.x, (t + 2u) / g.x).xyz);
    } else {
        b = pt_view(pt_load(min(c + 1u, g.x - 1u), r).xyz);
        d = pt_view(pt_load(c, min(r + 1u, g.y - 1u)).xyz);
    }
    let n = cross(b - v, d - v);
    if (length(n) < 1e-7) {
        return vec3f(0.0, 0.0, 1.0);
    }
    return normalize(n);
}

fn pt_vertex(c: u32, r: u32) -> PtOut {
    let d = pt_load(c, r);
    let v = pt_view(d.xyz);
    var o: PtOut;
    o.pos = pt_clip(v);
    o.color = pt_colour(d, v, pt_normal(c, r, v), pt_uv(c, r));
    o.corner = vec2f(0.0);
    if (d.a <= 0.0) {
        o.pos = vec4f(4.0, 4.0, 0.0, 1.0);
    }
    return o;
}

// Dots: a soft disc of P(1) pixels per point (six vertices each), added up.  P(12) = soft edge.
@vertex
fn vs_pts_dots(@builtin(vertex_index) vi: u32) -> PtOut {
    let g = pt_grid();
    let i = vi / 6u;
    let k = vi % 6u;
    var o = pt_vertex(i % g.x, i / g.x);
    var corner = vec2f(-1.0, -1.0);
    switch k {
        case 1u: { corner = vec2f(1.0, -1.0); }
        case 2u: { corner = vec2f(1.0, 1.0); }
        case 3u: { corner = vec2f(-1.0, -1.0); }
        case 4u: { corner = vec2f(1.0, 1.0); }
        case 5u: { corner = vec2f(-1.0, 1.0); }
        default: {}
    }
    o.corner = corner;
    o.pos = vec4f(o.pos.xy + corner * P(1u) / u.resolution * o.pos.w, 0.0, o.pos.w);
    return o;
}

@fragment
fn fs_pts_dots(in: PtOut) -> @location(0) vec4f {
    let d = length(in.corner);
    let soft = clamp(P(12u), 0.02, 1.0);
    let a = 1.0 - smoothstep(1.0 - soft, 1.0, d);
    return vec4f(in.color.rgb * a, a);
}

// Segment `seg`'s end `e` (0 or 1): between neighbours across the grid, down it, or both (P(12)); listed: pairs
// in order.
fn pt_segment_end(seg: u32, e: u32) -> PtOut {
    let g = pt_grid();
    if (P(13u) > 0.5) {
        let i = seg * 2u + e;
        return pt_vertex(i % g.x, i / g.x);
    }
    let mode = u32(P(12u) + 0.5);
    let across = select(g.y * (g.x - 1u), 0u, mode == 1u);
    var c = 0u;
    var r = 0u;
    if (seg < across) {
        r = seg / (g.x - 1u);
        c = seg % (g.x - 1u) + e;
    } else {
        let s = seg - across;
        c = s / (g.y - 1u);
        r = s % (g.y - 1u) + e;
    }
    return pt_vertex(c, r);
}

// Lines: each segment a thin quad (six vertices) P(1) pixels wide, added up.
@vertex
fn vs_pts_lines(@builtin(vertex_index) vi: u32) -> PtOut {
    let seg = vi / 6u;
    let k = vi % 6u;
    let a = pt_segment_end(seg, 0u);
    let b = pt_segment_end(seg, 1u);
    // On screen (perspective divided), then pushed sideways by half the width.
    let pa = a.pos.xy / a.pos.w;
    let pb = b.pos.xy / b.pos.w;
    let px = vec2f(2.0) / u.resolution;
    var dir = (pb - pa) / px;
    if (length(dir) < 1e-4) {
        dir = vec2f(1.0, 0.0);
    }
    let n = normalize(vec2f(-dir.y, dir.x)) * px * max(P(1u), 0.5) * 0.5;
    let at_b = (k == 1u || k == 2u || k == 4u);
    let side = select(-1.0, 1.0, k == 2u || k == 4u || k == 5u);
    var o = a;
    var p = pa;
    if (at_b) {
        o = b;
        p = pb;
    }
    p = p + n * side;
    o.pos = vec4f(p, 0.0, 1.0);
    if (a.pos.x > 3.0 || b.pos.x > 3.0) {
        o.pos = vec4f(4.0, 4.0, 0.0, 1.0);
    }
    return o;
}

@fragment
fn fs_pts_lines(in: PtOut) -> @location(0) vec4f {
    return in.color;
}

// Mesh: two triangles per grid cell (listed: every three points), lit; the backs hidden unless P(12).
@vertex
fn vs_pts_mesh(@builtin(vertex_index) vi: u32) -> PtOut {
    let g = pt_grid();
    let t = vi / 3u;
    let k = vi % 3u;
    var c = 0u;
    var r = 0u;
    var cs = array<u32, 3>(0u, 0u, 0u);
    var rs = array<u32, 3>(0u, 0u, 0u);
    if (P(13u) > 0.5) {
        for (var j = 0u; j < 3u; j++) {
            let i = t * 3u + j;
            cs[j] = i % g.x;
            rs[j] = i / g.x;
        }
    } else {
        let cell = t / 2u;
        let half = t % 2u;
        let cr = cell / (g.x - 1u);
        let cc = cell % (g.x - 1u);
        if (half == 0u) {
            cs = array<u32, 3>(cc, cc, cc + 1u);
            rs = array<u32, 3>(cr, cr + 1u, cr);
        } else {
            cs = array<u32, 3>(cc + 1u, cc, cc + 1u);
            rs = array<u32, 3>(cr, cr + 1u, cr + 1u);
        }
    }
    c = cs[k];
    r = rs[k];
    let a = pt_view(pt_load(cs[0], rs[0]).xyz);
    let b = pt_view(pt_load(cs[1], rs[1]).xyz);
    let d = pt_view(pt_load(cs[2], rs[2]).xyz);
    var n = cross(b - a, d - a);
    if (length(n) < 1e-7) {
        n = vec3f(0.0, 0.0, 1.0);
    } else {
        n = normalize(n);
    }
    let pt = pt_load(c, r);
    let v = pt_view(pt.xyz);
    var o: PtOut;
    o.pos = pt_clip(v);
    o.color = pt_colour(pt, v, n, pt_uv(c, r));
    o.corner = vec2f(0.0);
    // Facing away from the camera (which looks down -z from +z): hidden, the whole triangle together.
    if (pt.a <= 0.0 || (P(12u) < 0.5 && n.z < 0.0)) {
        o.pos = vec4f(4.0, 4.0, 0.0, 1.0);
    }
    return o;
}

@fragment
fn fs_pts_mesh(in: PtOut) -> @location(0) vec4f {
    return in.color;
}
