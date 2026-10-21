/*{
  "DESCRIPTION": "(lostjared/shaders: random_x_audio_freq_kaleidoscope)",
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














void main(void) {
    float aspect = RENDERSIZE.x / RENDERSIZE.y;
    vec2 uv = (isf_FragNormCoord - 0.5) * vec2(aspect, 1.0);

    float dist = length(uv);
    float angle = atan(uv.y, uv.x);

    // RMS controls segment count (3 to 12)
    float segments = floor(mix(3.0, 12.0, clamp(amp_rms * 3.0, 0.0, 1.0)));
    float segAngle = 6.28318 / segments;
    angle = mod(angle, segAngle);
    angle = abs(angle - segAngle * 0.5);

    // Mids rotate the kaleidoscope
    float rot = TIME * (0.4 + amp_mid * 3.0);
    angle += rot;

    // Bass pulses the zoom
    float zoom = 1.0 + amp_low * 0.4 * sin(TIME * 2.0);
    dist *= zoom;

    vec2 kaleUV = vec2(cos(angle), sin(angle)) * dist;
    kaleUV.x /= aspect;
    kaleUV += 0.5;

    vec4 tex = texture(inputImage, clamp(kaleUV, 0.0, 1.0));

    // Treble adds subtle hue shift
    float hueShift = amp_high * 0.15;
    tex.r += hueShift;
    tex.b -= hueShift * 0.5;

    // Peak flash
    tex.rgb += smoothstep(0.6, 1.0, amp_peak) * 0.2;

    gl_FragColor = vec4(clamp(tex.rgb, 0.0, 1.0), 1.0);
}
