/*{
  "DESCRIPTION": "Cinematic spectral bloom with multi-scale halos and a sharp protected source image. (lostjared/shaders: rainbow-code-spectral-bloom)",
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


// rainbow-code-spectral-bloom
// Cinematic spectral bloom with multi-scale halos and a sharp protected source image.












const float TAU = 6.28318530718;
vec2 mirrorUV(vec2 p) {
    return 1.0 - abs(mod(p, 2.0) - 1.0);
}
vec3 spectral(float x) {
    return .5 + .5 * cos(TAU * (x + vec3(.01, .34, .67)));
}
vec3 aces(vec3 x) {
    x = max(x, 0.0);
    return clamp((x * (2.51 * x + .03)) / (x * (2.43 * x + .59) + .14), 0.0, 1.0);
}
vec3 blurRing(vec2 uv, vec2 radius) {
    vec3 s = texture(inputImage, uv).rgb * .20;
    for (int i = 0; i < 8; i++) {
        float a = (float(i) + .5) * TAU / 8.0;
        s += texture(inputImage, mirrorUV(uv + vec2(cos(a), sin(a)) * radius)).rgb * .10;
    }
    return s;
}
void main() {
    float aspect = max(RENDERSIZE.x, 1.0) / max(RENDERSIZE.y, 1.0);
    vec2 p = (isf_FragNormCoord - .5) * vec2(aspect, 1.0);
    float r = length(p), a = atan(p.y, p.x), t = TIME * .22;
    vec2 drift = vec2(cos(a * 3.0 - t), sin(a * 2.0 + t)) * sin(r * 20.0 - t * 3.0) *
                 (.003 + amp_low * .006);
    vec2 uv = mirrorUV(isf_FragNormCoord + drift);
    vec3 sharp = texture(inputImage, uv).rgb;
    vec3 nearBlur = blurRing(uv, vec2(.006, .006 * aspect) * (1.0 + amp_mid));
    vec3 farBlur = blurRing(uv, vec2(.018, .018 * aspect) * (1.0 + amp_low * .7));
    float lum = dot(farBlur, vec3(.2126, .7152, .0722));
    float bright = smoothstep(.28, .86, lum);
    float rings = .5 + .5 * sin(r * (28.0 + amp_low * 8.0) - TIME * 2.0);
    vec3 tint = spectral(a / TAU + r * .5 - TIME * .035 + amp_high * .12);
    vec3 bloom = mix(nearBlur, farBlur, .55) * tint * (bright * .72 + .08);
    bloom += tint * pow(rings, 12.0) * (.06 + amp_peak * .34);
    vec3 result = sharp * .84 + bloom * (1.0 + amp_rms * .75);
    result += tint * exp(-r * 5.5) * (.04 + amp_smooth * .16);
    gl_FragColor = vec4(aces(result), texture(inputImage, uv).a);
}
