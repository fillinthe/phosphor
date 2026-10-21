/*{
  "DESCRIPTION": "(lostjared/shaders: sac-geo)",
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






vec3 hsv2rgb(vec3 c) {
    vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

vec3 sacredGeometry(vec2 uv, float time) {
    float angle = atan(uv.y, uv.x);
    float radius = length(uv);
    float frequency = 5.0;
    float amplitude = 0.5;

    float sacredPattern = sin(radius * frequency + time) * amplitude + 0.5;
    float hue = fract(sacredPattern + time * 0.1);
    float saturation = 1.0;
    float value = 1.0;

    return hsv2rgb(vec3(hue, saturation, value));
}

void main() {
    vec2 uv = isf_FragNormCoord * 2.0 - 1.0;
    uv *= vec2(RENDERSIZE.x / RENDERSIZE.y, 1.0);
    float time = TIME * 0.1;

    vec3 sacredColor = sacredGeometry(uv, time);

    vec2 complexCoord = uv * vec2(cos(time), sin(time));
    vec3 complexColor = sacredGeometry(complexCoord, time);

    vec4 textureColor = texture(inputImage, isf_FragNormCoord);
    vec3 combinedColor = mix(sacredColor, complexColor, 0.5);

    gl_FragColor = vec4(mix(textureColor.rgb, combinedColor, 0.5), 1.0);
}
