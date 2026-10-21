/*{
  "DESCRIPTION": "(lostjared/shaders: code-update-celestial-circuit)",
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


// Radial circuit mandala with pulsing traces, data sparks, and mirrored silicon facets.





const float T = 6.28318530718;
float H(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5);
}
vec3 P(float x) {
    return .5 + .5 * cos(T * (x + vec3(.15, .48, .83)));
}
void main() {
    float b = lj_spec4( .03).r, m = lj_spec4( .2).r, t = lj_spec4( .58).r,
          a = lj_spec4( .86).r;
    float A = RENDERSIZE.x / RENDERSIZE.y;
    vec2 p = (isf_FragNormCoord - .5) * vec2(A, 1);
    float r = length(p), an = atan(p.y, p.x);
    float seg = 12. + floor(b * 8.), w = T / seg, k = abs(mod(an + w * .5, w) - w * .5);
    vec2 q = vec2(k * seg, r * 14.);
    vec2 cell = floor(q);
    float trace = max(pow(.5 + .5 * cos(q.x * T), 32.), pow(.5 + .5 * cos(q.y * T), 32.));
    float gate = step(.35, H(cell));
    trace *= mix(.3, 1., gate);
    vec2 uv = fract(vec2(k / w, r * 2. + sin(k * seg) * .08 * m));
    float ca = .004 + .018 * t;
    vec3 c = vec3(texture(inputImage, uv + ca).r, texture(inputImage, uv).g, texture(inputImage, uv - ca).b) *
             (.48 + .48 * P(r));
    float spark = step(.975, H(cell + 17.)) * (.5 + .5 * sin(TIME * 6. + H(cell) * 20.));
    c += P(r * .7 + an / T - TIME * .06) * (trace * (.35 + 1.5 * a) + spark * (.55 + 2. * t));
    c += P(TIME * .08) * exp(-abs(r - .24 - b * .04) * 70.) * (.5 + b);
    c *= .9 + .3 * amp_smooth;
    c = mix(c, 1. - c, smoothstep(.95, 1., amp_peak));
    gl_FragColor = vec4(1. - exp(-max(c, 0.) * 1.3), 1);
}
