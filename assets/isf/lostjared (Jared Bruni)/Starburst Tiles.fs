/*{
  "DESCRIPTION": "(lostjared/shaders: mirror_x_audio31)",
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













vec2 mirror(vec2 uv) {
    return abs(mod(uv, 2.0) - 1.0);
}

void main(void) {
    float aLow = clamp(amp_low, 0.0, 1.0);
    float aMid = clamp(amp_mid, 0.0, 1.0);
    float aHigh = clamp(amp_high, 0.0, 1.0);
    float t = TIME;
    vec2 uv = 1.0 - abs(1.0 - 2.0 * isf_FragNormCoord);
    float aspect = RENDERSIZE.x / RENDERSIZE.y;
    vec2 p = (uv - 0.5) * vec2(aspect, 1.0);
    float r = length(p);
    float a = atan(p.y, p.x);
    float segments = 12.0;
    float step_ = 6.28318 / segments;
    a = mod(a, step_);
    a = abs(a - step_ * 0.5);
    vec2 kp = vec2(cos(a), sin(a)) * r;
    kp.x /= aspect;
    kp += 0.5;
    float breathe = 1.0 + 0.15 * sin(t * 1.5) * aLow;
    kp = (kp - 0.5) * breathe + 0.5;
    kp = mirror(kp);
    vec4 tex = texture(inputImage, kp);
    float vign = 1.0 - smoothstep(0.3, 0.8, r);
    tex.rgb *= 0.7 + 0.5 * vign;
    tex.rgb *= 1.0 + amp_peak * 0.5;
    tex.rgb = mix(tex.rgb, tex.rgb * vec3(1.0 + aMid * 0.2, 1.0, 1.0 + aHigh * 0.3), 0.5);
    gl_FragColor = tex;
}
