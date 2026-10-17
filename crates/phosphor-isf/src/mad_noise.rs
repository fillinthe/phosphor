//! A clean-room stand-in for MadMapper's `MadNoise.glsl` include: the noise functions its Materials call,
//! with the signatures those calls use. Only functions the shader calls and doesn't define itself are
//! emitted (naga drops anything unused). Looks are in the same family as the originals, not identical.

const HELPERS: &str = r#"
vec2 mn_g2(vec2 p) { p = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3))); return -1.0 + 2.0 * fract(sin(p) * 43758.5453123); }
vec3 mn_g3(vec3 p) { p = vec3(dot(p, vec3(127.1, 311.7, 74.7)), dot(p, vec3(269.5, 183.3, 246.1)), dot(p, vec3(113.5, 271.9, 124.6))); return -1.0 + 2.0 * fract(sin(p) * 43758.5453123); }
float mn_v2(vec2 p) { return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
float mn_v3(vec3 p) { return fract(sin(dot(p, vec3(12.9898, 78.233, 37.719))) * 43758.5453); }
float mn_perlin2(vec2 p) { vec2 i = floor(p); vec2 f = fract(p); vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(dot(mn_g2(i), f), dot(mn_g2(i + vec2(1.0, 0.0)), f - vec2(1.0, 0.0)), u.x),
             mix(dot(mn_g2(i + vec2(0.0, 1.0)), f - vec2(0.0, 1.0)), dot(mn_g2(i + vec2(1.0, 1.0)), f - vec2(1.0, 1.0)), u.x), u.y); }
float mn_perlin3(vec3 p) { vec3 i = floor(p); vec3 f = fract(p); vec3 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(mix(dot(mn_g3(i), f), dot(mn_g3(i + vec3(1.0, 0.0, 0.0)), f - vec3(1.0, 0.0, 0.0)), u.x),
                 mix(dot(mn_g3(i + vec3(0.0, 1.0, 0.0)), f - vec3(0.0, 1.0, 0.0)), dot(mn_g3(i + vec3(1.0, 1.0, 0.0)), f - vec3(1.0, 1.0, 0.0)), u.x), u.y),
             mix(mix(dot(mn_g3(i + vec3(0.0, 0.0, 1.0)), f - vec3(0.0, 0.0, 1.0)), dot(mn_g3(i + vec3(1.0, 0.0, 1.0)), f - vec3(1.0, 0.0, 1.0)), u.x),
                 mix(dot(mn_g3(i + vec3(0.0, 1.0, 1.0)), f - vec3(0.0, 1.0, 1.0)), dot(mn_g3(i + vec3(1.0, 1.0, 1.0)), f - vec3(1.0, 1.0, 1.0)), u.x), u.y), u.z); }
float mn_value2(vec2 p) { vec2 i = floor(p); vec2 f = fract(p); vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(mn_v2(i), mn_v2(i + vec2(1.0, 0.0)), u.x), mix(mn_v2(i + vec2(0.0, 1.0)), mn_v2(i + vec2(1.0, 1.0)), u.x), u.y); }
float mn_value3(vec3 p) { vec3 i = floor(p); vec3 f = fract(p); vec3 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(mix(mn_v3(i), mn_v3(i + vec3(1.0, 0.0, 0.0)), u.x), mix(mn_v3(i + vec3(0.0, 1.0, 0.0)), mn_v3(i + vec3(1.0, 1.0, 0.0)), u.x), u.y),
             mix(mix(mn_v3(i + vec3(0.0, 0.0, 1.0)), mn_v3(i + vec3(1.0, 0.0, 1.0)), u.x), mix(mn_v3(i + vec3(0.0, 1.0, 1.0)), mn_v3(i + vec3(1.0, 1.0, 1.0)), u.x), u.y), u.z); }
float mn_fbm2(vec2 p) { float a = 0.5; float s = 0.0; for (int k = 0; k < 5; k++) { s += a * mn_perlin2(p); p = p * 2.03 + 17.1; a *= 0.5; } return s; }
float mn_fbm3(vec3 p) { float a = 0.5; float s = 0.0; for (int k = 0; k < 5; k++) { s += a * mn_perlin3(p); p = p * 2.03 + 17.1; a *= 0.5; } return s; }
float mn_worley2(vec2 p) { vec2 i = floor(p); vec2 f = fract(p); float d = 8.0;
  for (int y = -1; y <= 1; y++) for (int x = -1; x <= 1; x++) { vec2 g = vec2(float(x), float(y)); vec2 o = fract(sin(vec2(dot(i + g, vec2(127.1, 311.7)), dot(i + g, vec2(269.5, 183.3)))) * 43758.5453); d = min(d, length(g + o - f)); }
  return clamp(d, 0.0, 1.0); }
float mn_worley3(vec3 p) { vec3 i = floor(p); vec3 f = fract(p); float d = 8.0;
  for (int z = -1; z <= 1; z++) for (int y = -1; y <= 1; y++) for (int x = -1; x <= 1; x++) { vec3 g = vec3(float(x), float(y), float(z)); vec3 o = fract(sin(vec3(dot(i + g, vec3(127.1, 311.7, 74.7)), dot(i + g, vec3(269.5, 183.3, 246.1)), dot(i + g, vec3(113.5, 271.9, 124.6)))) * 43758.5453); d = min(d, length(g + o - f)); }
  return clamp(d, 0.0, 1.0); }
float mn_billowed3(vec3 p) { float a = 0.5; float s = 0.0; for (int k = 0; k < 4; k++) { s += a * abs(mn_perlin3(p)); p = p * 2.03 + 17.1; a *= 0.5; } return clamp(s * 1.5, 0.0, 1.0); }
float mn_ridged3(vec3 p) { float a = 0.5; float s = 0.0; float w = 1.0; for (int k = 0; k < 4; k++) { float n = 1.0 - abs(mn_perlin3(p)); n = n * n * w; w = clamp(n * 2.0, 0.0, 1.0); s += a * n; p = p * 2.03 + 17.1; a *= 0.5; } return clamp(s, 0.0, 1.0); }
"#;

/// (name, GLSL) for every public function; each may be several overloads.
const FUNCTIONS: &[(&str, &str)] = &[
    ("noise", "float noise(vec2 p) { return mn_perlin2(p); }\nfloat noise(vec3 p) { return mn_perlin3(p); }\n"),
    ("vnoise", "float vnoise(vec2 p) { return mn_value2(p); }\nfloat vnoise(vec3 p) { return mn_value3(p); }\n"),
    ("fBm", "float fBm(vec2 p) { return mn_fbm2(p); }\nfloat fBm(vec3 p) { return mn_fbm3(p); }\n"),
    ("billowedNoise", "float billowedNoise(vec2 p) { return mn_billowed3(vec3(p, 0.0)); }\nfloat billowedNoise(vec3 p) { return mn_billowed3(p); }\n"),
    ("ridgedNoise", "float ridgedNoise(vec2 p) { return mn_ridged3(vec3(p, 0.0)); }\nfloat ridgedNoise(vec3 p) { return mn_ridged3(p); }\n"),
    ("ridgedMF", "float ridgedMF(vec2 p) { return mn_ridged3(vec3(p, 0.0)); }\nfloat ridgedMF(vec3 p) { return mn_ridged3(p); }\n"),
    ("billowyTurbulence", "float billowyTurbulence(vec3 p) { return mn_billowed3(p); }\n"),
    ("worleyNoise", "float worleyNoise(vec2 p) { return mn_worley2(p); }\nfloat worleyNoise(vec3 p) { return mn_worley3(p); }\n"),
    ("flowNoise", "float flowNoise(vec2 p, float t) { return mn_perlin3(vec3(p, t)); }\n"),
    ("curlNoise", "vec2 curlNoise(vec2 p, float t) { float e = 0.01; float a = mn_perlin3(vec3(p + vec2(0.0, e), t)); float b = mn_perlin3(vec3(p - vec2(0.0, e), t)); float c = mn_perlin3(vec3(p + vec2(e, 0.0), t)); float d = mn_perlin3(vec3(p - vec2(e, 0.0), t)); return vec2(a - b, -(c - d)) / (2.0 * e); }\n"),
    ("dvnoise", "vec4 dvnoise(vec3 p) { float e = 0.01; float v = mn_value3(p); return vec4(v, (mn_value3(p + vec3(e, 0.0, 0.0)) - v) / e, (mn_value3(p + vec3(0.0, e, 0.0)) - v) / e, (mn_value3(p + vec3(0.0, 0.0, e)) - v) / e); }\n"),
    ("dWorleyNoise", "vec4 dWorleyNoise(vec3 p) { float e = 0.01; float v = mn_worley3(p); return vec4(v, (mn_worley3(p + vec3(e, 0.0, 0.0)) - v) / e, (mn_worley3(p + vec3(0.0, e, 0.0)) - v) / e, (mn_worley3(p + vec3(0.0, 0.0, e)) - v) / e); }\n"),
    ("dBillowedNoise", "vec4 dBillowedNoise(vec3 p) { float e = 0.01; float v = mn_billowed3(p); return vec4(v, (mn_billowed3(p + vec3(e, 0.0, 0.0)) - v) / e, (mn_billowed3(p + vec3(0.0, e, 0.0)) - v) / e, (mn_billowed3(p + vec3(0.0, 0.0, e)) - v) / e); }\n"),
    ("dFlowNoise", "vec3 dFlowNoise(vec2 p, float t) { float e = 0.01; float v = mn_perlin3(vec3(p, t)); return vec3(v, (mn_perlin3(vec3(p + vec2(e, 0.0), t)) - v) / e, (mn_perlin3(vec3(p + vec2(0.0, e), t)) - v) / e); }\n"),
    ("hash", "float hash(vec2 p) { return mn_v2(p); }\nfloat hash(float p) { return fract(sin(p * 12.9898) * 43758.5453); }\n"),
    ("hash22", "vec2 hash22(vec2 p) { return fract(sin(vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)))) * 43758.5453); }\n"),
    ("hash13", "float hash13(vec3 p) { return mn_v3(p); }\n"),
    ("hash33", "vec3 hash33(vec3 p) { return fract(sin(vec3(dot(p, vec3(127.1, 311.7, 74.7)), dot(p, vec3(269.5, 183.3, 246.1)), dot(p, vec3(113.5, 271.9, 124.6)))) * 43758.5453); }\n"),
    ("valueHash", "float valueHash(vec3 p) { return mn_v3(p); }\n"),
    ("fx_rand3", "vec3 fx_rand3(vec2 p) { return fract(sin(vec3(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)), dot(p, vec2(419.2, 371.9)))) * 43758.5453); }\n"),
];

/// The shim for one shader body: helpers plus every function it calls but doesn't define.
pub(crate) fn shim(body: &str) -> String {
    crate::mad_shim::shim(body, HELPERS, FUNCTIONS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_called_functions_are_emitted() {
        let body = "float n = noise(uv) + vnoise(uv * 3.0);";
        let s = shim(body);
        assert!(s.contains("float noise(vec2 p)") && s.contains("float vnoise(vec2 p)"));
        assert!(!s.contains("float fBm("));
        let own = "float noise(vec2 p) { return 0.0; } void main() { float n = noise(uv); }";
        assert!(shim(own).is_empty());
    }
}
