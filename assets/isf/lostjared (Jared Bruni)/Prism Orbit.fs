/*{
  "DESCRIPTION": "(lostjared/shaders: code-update-temporal-prism-orbit)",
  "CREDIT": "Jared Bruni (lostjared/shaders, MIT)",
  "ISFVSN": "2",
  "CATEGORIES": [
    "lostjared"
  ],
  "INPUTS": [
    {
      "NAME": "inputImage",
      "TYPE": "image"
    },
    {
      "NAME": "audioFFT",
      "TYPE": "audioFFT"
    }
  ]
}*/

float lj_spec(float x) { return IMG_NORM_PIXEL(audioFFT, vec2(clamp(x, 0.0, 1.0), 0.5)).r; }
vec4 lj_spec4(float x) { return vec4(lj_spec(x)); }
float lj_band(float a, float b) { return (lj_spec(a) + lj_spec(mix(a, b, 0.33)) + lj_spec(mix(a, b, 0.66)) + lj_spec(b)) * 0.25; }
#define amp_low lj_band(0.0, 0.08)
#define amp_mid lj_band(0.08, 0.35)
#define amp_high lj_band(0.35, 0.9)
#define amp_rms lj_band(0.0, 0.5)
#define amp_smooth lj_band(0.0, 0.5)
#define amp_peak max(lj_band(0.0, 0.1), lj_band(0.1, 0.5))
#define amp lj_band(0.0, 0.5)
#define iamp lj_band(0.0, 0.5)
#define iMouse vec4(RENDERSIZE * 0.5, 0.0, 0.0)


// Concentric orbital echoes with dispersion and luminous interference rings.





const float TAU = 6.28318530718;
mat2 R(float x) {
    float c = cos(x), s = sin(x);
    return mat2(c, -s, s, c);
}
vec3 pal(float x) {
    return .5 + .5 * cos(TAU * (x + vec3(.0, .31, .63)));
}
void main() {
    float b = lj_spec4( .025).r, m = lj_spec4( .19).r, t = lj_spec4( .57).r,
          a = lj_spec4( .83).r;
    float asp = RENDERSIZE.x / RENDERSIZE.y;
    vec2 p = (isf_FragNormCoord - .5) * vec2(asp, 1);
    float r = length(p), an = atan(p.y, p.x);
    vec3 acc = vec3(0);
    float ws = 0.;
    for (int i = 0; i < 7; i++) {
        float fi = float(i), age = fi / 6.;
        vec2 q = p * R(TIME * (.045 + age * .055) + fi * .62 + b * .3);
        q *= 1. + age * (.08 + .18 * b);
        q += normalize(p + vec2(1e-4)) * sin(r * 24. - TIME * 2. + fi) * .008 * m;
        vec2 uv = q / vec2(asp, 1) + .5;
        float ca = age * (.004 + .024 * t);
        vec3 s = vec3(texture(inputImage, uv + vec2(ca, 0)).r, texture(inputImage, uv).g,
                      texture(inputImage, uv - vec2(ca, 0)).b);
        float w = pow(.78, fi);
        // Keep the tint periodic across atan's -PI/PI boundary.
        float angularPhase = .06 * cos(an) + .04 * sin(an * 2.);
        acc += s * pal(age * .28 + angularPhase + TIME * .025) * w;
        ws += w;
    }
    vec3 c = acc / ws;
    float rings =
        pow(.5 + .5 * cos(r * (54. + m * 20.) - TIME * (2. + b * 3.) + sin(an * 5.) * 2.), 14.);
    float orbit =
        pow(max(cos(an * 6. - TIME + r * 8.), 0.), 18.) * smoothstep(.48, .12, abs(r - .34));
    c += pal(r * 1.5 - TIME * .09) * (rings * (.24 + a * .8) + orbit * (.4 + b));
    c *= .94 + .3 * amp_smooth;
    c = mix(c, 1. - c, smoothstep(.96, 1., amp_peak));
    gl_FragColor = vec4(1. - exp(-max(c, 0.) * 1.2), 1);
}
