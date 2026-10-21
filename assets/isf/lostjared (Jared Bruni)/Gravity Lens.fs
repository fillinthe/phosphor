/*{
  "DESCRIPTION": "(lostjared/shaders: code-update-chromatic-gravity-lens)",
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


// Multi-source gravity lens splitting the image into orbiting spectral Einstein rings.





const float T = 6.28318530718;
vec3 P(float x) {
    return .5 + .5 * cos(T * (x + vec3(.0, .33, .67)));
}
vec2 warp(vec2 p, vec2 o, float k) {
    vec2 d = p - o;
    return d * k / (dot(d, d) + .025);
}
void main() {
    float b = lj_spec4( .03).r, m = lj_spec4( .2).r, t = lj_spec4( .59).r,
          a = lj_spec4( .86).r;
    float A = RENDERSIZE.x / RENDERSIZE.y;
    vec2 p = (isf_FragNormCoord - .5) * vec2(A, 1);
    vec2 o1 = .2 * vec2(cos(TIME * .31), sin(TIME * .43)),
         o2 = .28 * vec2(cos(TIME * .21 + 2.), sin(TIME * .27 + 2.));
    vec2 q = p + warp(p, o1, .018 + .025 * b) + warp(p, o2, .012 + .02 * m);
    vec2 uv = q / vec2(A, 1) + .5;
    vec2 dir = normalize(q - p + vec2(1e-4));
    float ca = .006 + .032 * t;
    vec3 c =
        vec3(texture(inputImage, uv + dir * ca).r, texture(inputImage, uv).g, texture(inputImage, uv - dir * ca).b);
    float d1 = length(p - o1), d2 = length(p - o2);
    float ring = exp(-abs(d1 - (.13 + .03 * b)) * 80.) + exp(-abs(d2 - (.1 + .025 * m)) * 90.);
    float arc = ring * pow(.5 + .5 * cos(atan(p.y, p.x) * 12. - TIME * 1.5), 8.);
    c *= .55 + .55 * P(length(q) * .6 + TIME * .025);
    c += P(d1 - d2 + TIME * .08) * (ring * (.45 + b) + arc * (.4 + 2. * a));
    c *= .9 + .3 * amp_smooth;
    c = mix(c, 1. - c, smoothstep(.95, 1., amp_peak));
    gl_FragColor = vec4(1. - exp(-max(c, 0.) * 1.28), 1);
}
