/*{
  "DESCRIPTION": "(lostjared/shaders: code-update-oceanic-star-forge)",
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


// Spiral stellar forge submerged beneath rolling spectral caustics.





const float T = 6.28318530718;
float H(vec2 p) {
    return fract(sin(dot(p, vec2(27.1, 113.5))) * 43758.5);
}
vec3 P(float x) {
    return .5 + .5 * cos(T * (x + vec3(.56, .22, .02)));
}
void main() {
    float b = lj_spec4( .03).r, m = lj_spec4( .22).r, t = lj_spec4( .61).r,
          a = lj_spec4( .88).r;
    float A = RENDERSIZE.x / RENDERSIZE.y;
    vec2 p = (isf_FragNormCoord - .5) * vec2(A, 1);
    float r = length(p) + .01, an = atan(p.y, p.x);
    float wave = sin(p.x * 14. + sin(p.y * 7. - TIME) * 2.) + sin(p.y * 17. - TIME * 1.3);
    vec2 uv = isf_FragNormCoord + vec2(wave, sin(wave * T)) * (.006 + .012 * m);
    float ca = .004 + .022 * t;
    vec3 c = vec3(texture(inputImage, uv + ca).r, texture(inputImage, uv).g, texture(inputImage, uv - ca).b) *
             vec3(.45, .7, 1.);
    float arm = pow(.5 + .5 * cos(an * 5. - log(r) * 7. + TIME * (.7 + b)), 14.) * exp(-r * 2.);
    float caust = pow(max(wave * .25 + .5, 0.), 8.);
    float stars = step(.991, H(floor((p + 2.) * 100.)));
    c += P(an / T + r - TIME * .08) * arm * (.7 + 2.5 * b) +
         vec3(.2, .7, 1) * caust * (.15 + a * .6) + P(TIME * .1) * stars * (.4 + 2. * a);
    c *= .9 + .3 * amp_smooth;
    c = mix(c, 1. - c, smoothstep(.95, 1., amp_peak));
    gl_FragColor = vec4(1. - exp(-max(c, 0.) * 1.3), 1);
}
