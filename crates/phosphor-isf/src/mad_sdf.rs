//! A clean-room stand-in for MadMapper's `MadSDF.glsl` include: 2-D signed-distance shapes, operators and
//! the fill / stroke / glow colouring its Materials call, with the signatures those calls use. Distances are
//! in the shader's own coordinate units (texture coordinates, 0..1); negative = inside. Anti-aliasing is a
//! fixed width of pixels chosen by the shader's `SDF_ANTIALIASING_*` define (none / medium / high), measured
//! against the output size, so no derivatives are needed (they're not allowed in every branch).

const HELPERS: &str = r#"
float sdf_coverage(float d) { float w = SDF_AA_PIXELS / max(phosphor_u.resolution.y, 1.0); return w > 0.0 ? 1.0 - smoothstep(-w, w, d) : step(d, 0.0); }
float sdf_glow(float d, float width) { return exp(-3.0 * max(d, 0.0) / max(width, 1e-5)); }
float sdf_box(vec2 p, vec2 b) { vec2 d = abs(p) - b; return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0); }
"#;

/// (name, GLSL) for every public function; each may be several overloads.
const FUNCTIONS: &[(&str, &str)] = &[
    ("circle", "float circle(vec2 p, float r) { return length(p) - r; }\nfloat circle(vec2 p, vec2 center, float r) { return length(p - center) - r; }\n"),
    ("rectangle", "float rectangle(vec2 p, vec2 b) { return sdf_box(p, b); }\nfloat rectangle(vec2 p, float b) { return sdf_box(p, vec2(b)); }\nfloat rectangle(vec2 p, vec2 b, float r) { return sdf_box(p, b - vec2(r)) - r; }\n"),
    ("roundedRectangle", "float roundedRectangle(vec2 p, vec2 b, float r) { return sdf_box(p, b - vec2(r)) - r; }\n"),
    ("hexagon", "float hexagon(vec2 p, float r) { const vec3 k = vec3(-0.866025404, 0.5, 0.577350269); p = abs(p); p -= 2.0 * min(dot(k.xy, p), 0.0) * k.xy; p -= vec2(clamp(p.x, -k.z * r, k.z * r), r); return length(p) * sign(p.y); }\n"),
    ("triangle", "float triangle(vec2 p, float r) { const float k = 1.73205081; p.x = abs(p.x) - r; p.y = p.y + r / k; if (p.x + k * p.y > 0.0) { p = vec2(p.x - k * p.y, -k * p.x - p.y) / 2.0; } p.x -= clamp(p.x, -2.0 * r, 0.0); return -length(p) * sign(p.y); }\n"),
    ("line", "float line(vec2 p, vec2 a, vec2 b, float width) { vec2 pa = p - a; vec2 ba = b - a; float h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-8), 0.0, 1.0); return length(pa - ba * h) - width; }\n"),
    ("arc", "float arc(vec2 p, float radius, float angle, float width) { if (angle >= 3.14159265) { return abs(length(p) - radius) - width; } vec2 sc = vec2(sin(angle), cos(angle)); p.x = abs(p.x); float d = (sc.y * p.x > sc.x * p.y) ? length(p - sc * radius) : abs(length(p) - radius); return d - width; }\n"),
    ("blend", "float blend(float a, float b) { return min(a, b); }\n"),
    ("subtract", "float subtract(float a, float b) { return max(a, -b); }\n"),
    ("intersect", "float intersect(float a, float b) { return max(a, b); }\n"),
    ("rotate", "vec2 rotate(vec2 p, float a) { float c = cos(a); float s = sin(a); return vec2(c * p.x - s * p.y, s * p.x + c * p.y); }\n"),
    ("translate", "vec2 translate(vec2 p, vec2 t) { return p - t; }\n"),
    ("scale", "vec2 scale(vec2 p, float s) { return p / s; }\nvec2 scale(vec2 p, vec2 s) { return p / s; }\n"),
    ("repeat", "vec2 repeat(vec2 p, vec2 size) { vec2 id = floor(p / size); return p - (id + 0.5) * size; }\nvec2 repeat(vec2 p, vec2 size, out vec2 cellId) { cellId = floor(p / size); return p - (cellId + 0.5) * size; }\nvec2 repeat(vec2 p, vec2 size, vec2 offset, vec2 period, out vec2 cellId) { float row = floor(p.y / size.y); p.x += offset.x * size.x * mod(row, max(period.y, 1.0)); cellId = floor(p / size); return p - (cellId + 0.5) * size; }\n"),
    ("repeatRadial", "vec2 repeatRadial(vec2 p, float angle, out vec2 cellId) { float a = atan(p.y, p.x); float i = floor(a / angle + 0.5); cellId = vec2(i, 0.0); float c = cos(-i * angle); float s = sin(-i * angle); return vec2(c * p.x - s * p.y, s * p.x + c * p.y); }\n"),
    ("fill", "vec3 fill(vec3 background, vec3 color, float d) { return mix(background, color, sdf_coverage(d)); }\nvec4 fill(vec4 background, vec4 color, float d) { return mix(background, color, sdf_coverage(d)); }\n"),
    ("stroke", "vec3 stroke(vec3 background, vec3 color, float d, float width) { return mix(background, color, sdf_coverage(abs(d) - width)); }\nvec4 stroke(vec4 background, vec4 color, float d, float width) { return mix(background, color, sdf_coverage(abs(d) - width)); }\n"),
    ("glow", "vec3 glow(vec3 color, float d, float width) { return color * sdf_glow(d, width); }\nvec4 glow(vec4 color, float d, float width) { return color * sdf_glow(d, width); }\n"),
];

/// The shim for one shader body: helpers plus every function it calls but doesn't define.
pub(crate) fn shim(body: &str) -> String {
    // The shader's `#define SDF_ANTIALIASING_*` comes after the shim in the module, so read it here.
    let pixels = if body.contains("SDF_ANTIALIASING_NONE") {
        0.0
    } else if body.contains("SDF_ANTIALIASING_HIGH") {
        2.0
    } else {
        1.0
    };
    let helpers = format!("#define SDF_AA_PIXELS {pixels:.1}\n{HELPERS}");
    crate::mad_shim::shim(body, &helpers, FUNCTIONS)
}

#[cfg(test)]
mod tests {
    #[test]
    fn shapes_and_colouring_come_only_when_called() {
        let s = super::shim("vec3 c = fill(vec3(0), vec3(1), circle(p, 0.5));");
        assert!(s.contains("float circle(vec2 p, float r)") && s.contains("vec3 fill("));
        assert!(!s.contains("float hexagon("));
        assert!(super::shim("float hexagon(vec2 p, float r) { return 0.0; } float d = hexagon(p, 1.0);").is_empty());
    }
}
