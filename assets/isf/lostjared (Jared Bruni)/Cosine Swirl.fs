/*{
  "DESCRIPTION": "(lostjared/shaders: cosine_swirl_two)",
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








vec3 rainbow(float t){
    float hue = mod(t, 1.0) * 6.0;
    float c = 1.0;
    float x = 1.0 - abs(mod(hue, 2.0) - 1.0);
    vec3 rgb = (hue < 1.0) ? vec3(c, x, 0.0) :
               (hue < 2.0) ? vec3(x, c, 0.0) :
               (hue < 3.0) ? vec3(0.0, c, x) :
               (hue < 4.0) ? vec3(0.0, x, c) :
               (hue < 5.0) ? vec3(x, 0.0, c) :
                             vec3(c, 0.0, x);
    return rgb;
}

void main(void){
    float aspect = RENDERSIZE.x / RENDERSIZE.y;
    vec2 ar = vec2(aspect, 1.0);
    vec2 m = (iMouse.z > 0.5) ? (iMouse.xy / RENDERSIZE) : vec2(0.5);

    vec2 p = (isf_FragNormCoord - m) * ar;
    float base = 1.68;
    float period = log(base);
    float t = TIME * 0.5;
    float r = length(p) + 1e-6;
    float a = atan(p.y, p.x) + t * 0.35;
    float k = fract((log(r) - t) / period);
    float rw = exp(k * period);
    vec2 z = vec2(cos(a), sin(a)) * rw;
    vec2 uv = fract(z / ar + m);

    vec2 dir = (uv - m);
    dir.x *= aspect;
    float dist = length(dir);
    float angle = atan(dir.y, dir.x);
    float frequency = 1.0;
    float amplitude = 3.0;
    float spiralTime = sin(TIME * frequency) * amplitude;
    float scale = 1.0;
    float gradient_pos = mod(dist * scale + (angle + spiralTime) / (2.0 * 3.14159), 1.0);
    vec3 color_gradient = rainbow(gradient_pos);

    vec4 ctx = texture(inputImage, uv);
    float time_t = mod(TIME, 10.0) + 2.0;
    gl_FragColor = mix(cos(ctx * time_t), vec4(color_gradient, 1.0), 0.5);
    gl_FragColor.a = 1.0;
}
