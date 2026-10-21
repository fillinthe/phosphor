/*{
  "DESCRIPTION": "(lostjared/shaders: code-update-event-horizon-crown)",
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


// Gravitational log-polar lens with accretion crown and RGB caustics.





const float TAU = 6.28318530718;
vec3 pal(float x) {
    return .5 + .5 * cos(TAU * (x + vec3(.0, .18, .48)));
}
vec2 lens(vec2 p, float z) {
    float r = length(p) + .018, a = atan(p.y, p.x);
    a += 1.25 / r + z;
    return vec2(cos(a), sin(a)) * abs(log(r) * .19 + .36);
}
void main() {
    float b = lj_spec4( .03).r, m = lj_spec4( .2).r, t = lj_spec4( .6).r,
          a = lj_spec4( .88).r;
    float asp = RENDERSIZE.x / RENDERSIZE.y;
    vec2 p = (isf_FragNormCoord - .5) * vec2(asp, 1);
    float r = length(p), an = atan(p.y, p.x);
    float pulse = .04 * b + .015 * sin(TIME * 1.7);
    vec2 q = lens(p, TIME * .16 + b * .7);
    q.x /= asp;
    q += .5;
    vec2 tangent = normalize(vec2(-p.y, p.x) + vec2(1e-4));
    float ca = .006 + .03 * t;
    vec3 c = vec3(texture(inputImage, fract(q + tangent * ca)).r, texture(inputImage, fract(q)).g,
                  texture(inputImage, fract(q - tangent * ca)).b);
    float disk = exp(-abs(r - (.27 + pulse)) * (48. - m * 12.));
    float crown =
        pow(.5 + .5 * cos(an * 11. - log(r + .03) * 7. + TIME * 2.), 10.) * exp(-r * 2.2);
    float photon = exp(-abs(r - .16 - b * .025) * 95.);
    c *= .58 + .55 * pal(an / TAU + TIME * .04);
    c += pal(an / TAU + r * 1.4 + TIME * .1) *
         (disk * (1.2 + 3. * b) + crown * (.35 + 1.8 * a) + photon * 1.5);
    c *= smoothstep(.055, .13, r);
    c += pal(TIME * .08) * exp(-r * 24.) * .5;
    c = mix(c, c.bgr, .2 * m);
    c *= .9 + .28 * amp_smooth;
    c = mix(c, 1. - c, smoothstep(.965, 1., amp_peak));
    gl_FragColor = vec4(1. - exp(-max(c, 0.) * 1.15), 1);
}
