/*{
  "DESCRIPTION": "(lostjared/shaders: code-update-quantum-ribbon-vault)",
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


// Braided quantum ribbons folded into a mirrored, depth-shifting vault.





const float T = 6.28318530718;
mat2 R(float x) {
    float c = cos(x), s = sin(x);
    return mat2(c, -s, s, c);
}
vec3 P(float x) {
    return .5 + .5 * cos(T * (x + vec3(.0, .28, .66)));
}
void main() {
    float b = lj_spec4( .03).r, m = lj_spec4( .2).r, t = lj_spec4( .59).r,
          a = lj_spec4( .87).r;
    float A = RENDERSIZE.x / RENDERSIZE.y;
    vec2 p = (isf_FragNormCoord - .5) * vec2(A, 1), q = p;
    q *= R(TIME * .09);
    q = abs(q);
    if (q.x < q.y)
        q = q.yx;
    float ribbons = 0.;
    for (int i = 0; i < 5; i++) {
        float fi = float(i);
        float strand =
            sin(q.x * (15. + fi * 3.) + sin(q.y * (7. + fi)) * 2. + TIME * (.7 - fi * .08));
        ribbons += pow(1. - abs(strand), 18.) / pow(1.35, fi);
        q = abs(q * 1.32 - vec2(.31, .22));
        q *= R(.32 + fi * .27);
    }
    vec2 uv = fract(q / vec2(A, 1) + .5);
    float ca = .004 + .023 * t;
    vec3 c = vec3(texture(inputImage, uv + ca).r, texture(inputImage, uv).g, texture(inputImage, uv - ca).b) *
             (.46 + .58 * P(length(q) * .3));
    c += P(ribbons * .15 + atan(p.y, p.x) / T - TIME * .06) * ribbons * (.45 + 1.7 * a);
    c += P(TIME * .08) * exp(-length(p) * 15.) * (.4 + 2. * b);
    c *= .9 + .3 * amp_smooth;
    c = mix(c, 1. - c, smoothstep(.96, 1., amp_peak));
    gl_FragColor = vec4(1. - exp(-max(c, 0.) * 1.26), 1);
}
