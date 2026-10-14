fn main() {
    let src = r#"#version 450
layout(std140, binding=0) uniform U_isf { vec4 p[5]; vec2 resolution; float time; float opacity; vec4 uv_xform; uint mode; uint yuv; uint _pad0; uint _pad1; } u;
layout(binding=1) uniform sampler samp;
layout(binding=3) uniform texture2D tex0;
layout(binding=4) uniform texture2D tex1;
layout(location=0) in vec2 phosphor_uv;
layout(location=0) out vec4 gl_FragColor_out;
#define gl_FragColor gl_FragColor_out
#define noiseLevel (u.p[0].x)
#define invert (u.p[0].y != 0.0)
#define mode1 (int(u.p[0].z))
#define col (vec4(u.p[1].zw, u.p[2].xy))
#define TIME (u.time)
#define RENDERSIZE (u.resolution)
#define isf_FragNormCoord (vec2(phosphor_uv.x, 1.0 - phosphor_uv.y))
#define IMG_NORM_PIXEL(img, nuv) textureLod(sampler2D(img, samp), isf_flip(nuv), 0.0)
#define IMG_SIZE(img) vec2(textureSize(img, 0))
vec2 isf_flip(vec2 v) { return vec2(v.x, 1.0 - v.y); }
const float tau = 6.28;
float gcount = 0.0;
vec3 mod289(vec3 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec2 mod289(vec2 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
struct Foo { float a; };
void main() {
    vec2 p = isf_FragNormCoord;
    Foo f; f.a = 2 * noiseLevel;
    float n = 1 - f.a;
    if (invert) n = -n;
    gcount += 1.0f;
    vec4 c = IMG_NORM_PIXEL(tex0, p + gl_FragCoord.xy / RENDERSIZE) * col;
    if (mode1 == 1) c += IMG_NORM_PIXEL(tex1, p);
    c.rgb += mod289(c.rgb) + vec3(mod289(c.xy), tau) + IMG_SIZE(tex0).x;
    gl_FragColor = c * n;
}
"#;
    println!("{}", phosphor_isf::proto(src));
}
