/*{
  "DESCRIPTION": "(lostjared/shaders: code-update-electric-reef)",
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


// Animated coral branches with electric polyps and deep-ocean iridescence.





const float T = 6.28318530718;
vec3 P(float x) {
    return .5 + .5 * cos(T * (x + vec3(.55, .2, .88)));
}
float H(vec2 p) {
    return fract(sin(dot(p, vec2(27.1, 91.7))) * 43758.5);
}
float F(vec2 p) {
    float v = 0.;
    for (int i = 0; i < 6; i++) {
        p = abs(p) * 1.42 - vec2(.38, .28);
        p += .08 * sin(p.yx * 5. + TIME * .4 + float(i));
        v += exp(-16. * abs(p.x + sin(p.y * 4.) * .08)) / pow(1.35, float(i));
    }
    return v;
}
void main() {
    float b = lj_spec4( .035).r, m = lj_spec4( .22).r, t = lj_spec4( .64).r,
          a = lj_spec4( .9).r;
    float A = RENDERSIZE.x / RENDERSIZE.y;
    vec2 p = (isf_FragNormCoord - .5) * vec2(A, 1);
    p.y += .28;
    float f = F(p * (1.2 + b * .25));
    vec2 uv = isf_FragNormCoord + vec2(sin(p.y * 12. + TIME), cos(p.x * 10. - TIME)) * .012 * m;
    vec3 c = texture(inputImage, uv).rgb * (.4 + .58 * P(p.y * .3 + TIME * .02));
    float polyp =
        step(.975, H(floor((p + 2.) * 35.))) * (.5 + .5 * sin(TIME * 5. + H(floor(p * 35.)) * T));
    c += P(f * .2 + p.y - TIME * .06) * f * (.65 + 1.7 * t) +
         P(TIME * .1 + length(p)) * polyp * (.6 + 2. * a);
    c += vec3(.03, .12, .2) * (1. - smoothstep(-.5, .5, p.y));
    c *= .9 + .32 * amp_smooth;
    c = mix(c, 1. - c, smoothstep(.95, 1., amp_peak));
    gl_FragColor = vec4(1. - exp(-max(c, 0.) * 1.25), 1);
}
