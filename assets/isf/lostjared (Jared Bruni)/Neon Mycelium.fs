/*{
  "DESCRIPTION": "(lostjared/shaders: code-update-neon-mycelium)",
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


// Branching electric filament field with reactive bioluminescent pulses.





const float TAU = 6.28318530718;
float h(vec2 p) {
    return fract(sin(dot(p, vec2(41.7, 289.3))) * 43758.54);
}
float fil(vec2 p) {
    float f = 0.;
    for (int i = 0; i < 5; i++) {
        p = abs(p) * 1.55 - vec2(.42, .31);
        p += .12 * sin(p.yx * 3. + float(i) + TIME * .35);
        float d = abs(sin(p.x * 4.2) + sin(p.y * 5.1));
        f += exp(-18. * d) / pow(1.45, float(i));
    }
    return f;
}
vec3 pal(float x) {
    return .5 + .5 * cos(TAU * (x + vec3(.46, .12, .78)));
}
void main() {
    float b = lj_spec4( .04).r, m = lj_spec4( .25).r, t = lj_spec4( .64).r,
          a = lj_spec4( .9).r;
    float asp = RENDERSIZE.x / RENDERSIZE.y;
    vec2 p = (isf_FragNormCoord - .5) * vec2(asp, 1);
    p += .06 * vec2(sin(p.y * 8. + TIME), cos(p.x * 7. - TIME * .8)) * m;
    float f = fil(p * (1.25 + b * .25));
    vec2 uv = isf_FragNormCoord + vec2(sin(p.y * 19. + TIME), cos(p.x * 17. - TIME)) * f * .014;
    vec3 c = texture(inputImage, uv).rgb * (.48 + .52 * pal(length(p) + TIME * .03));
    float node = step(.985, h(floor((p + 2.) * 28.))) *
                 (.5 + .5 * sin(TIME * 4. + h(floor(p * 28.)) * 20.));
    c += pal(f * .4 + TIME * .08) * f * (1. + 2. * t) +
         pal(length(p) - TIME * .1) * node * (.5 + 2. * a);
    c += pal(TIME * .06) * pow(f, 3.) * (.35 + b);
    c *= .86 + .35 * amp_smooth;
    c = mix(c, 1. - c, smoothstep(.95, 1., amp_peak));
    gl_FragColor = vec4(1. - exp(-max(c, 0.) * 1.22), 1);
}
