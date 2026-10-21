/*{
  "DESCRIPTION": "(lostjared/shaders: frac_mir)",
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














vec2 kaleidoscope(vec2 uv, float segments) {
    float angle = atan(uv.y, uv.x);
    float radius = length(uv);
    angle = mod(angle, 2.0 * 3.14159265359 / segments);
    return vec2(cos(angle), sin(angle)) * radius;
}

float fractalSpiral(vec2 uv) {
    float scale = 1.0;
    float intensity = 0.0;
    float rotation = TIME * 0.5;
    for (int i = 0; i < 8; i++) {
        uv = mat2(cos(rotation), -sin(rotation), sin(rotation), cos(rotation)) * uv;
        uv = abs(uv) / dot(uv, uv) - 0.5;
        intensity += 1.0 / scale * sin(uv.x * 10.0 + uv.y * 10.0 + TIME);
        scale *= 1.8;
    }
    return intensity;
}

vec3 blendMaterials(vec3 texColor, vec3 fractalColor, vec2 uv) {
    float fresnel = pow(1.0 - dot(normalize(uv), vec2(0.0, 0.0)), 2.0);
    vec3 specularHighlight = vec3(1.0) * pow(max(0.0, fractalColor.r), 20.0);
    return mix(texColor, fractalColor + specularHighlight, fresnel);
}

float pingPong(float x, float length) {
    return length - abs(mod(x, 2.0 * length) - length);
}

void main() {
    vec2 uv = isf_FragNormCoord * 2.0 - 1.0;
    uv.x *= RENDERSIZE.x / RENDERSIZE.y;
    float segments = 6.0 + mod(TIME, 10.0);
    uv = kaleidoscope(uv, segments);
    float fractalValue = fractalSpiral(uv * 2.0);
    vec3 fractalColor = vec3(fractalValue * 0.8, fractalValue * 0.6, fractalValue * 0.4);
    vec3 texColor = texture(inputImage, uv * 0.5 + 0.5).rgb;
    vec3 blendedColor = blendMaterials(texColor, fractalColor, uv);
    blendedColor += vec3(0.3);
    blendedColor.r += sin(uv.x * 2.0 + TIME) * 0.2;
    blendedColor.g += sin(uv.y * 2.0 + TIME * 0.5) * 0.2;
    blendedColor.b += sin((uv.x + uv.y) * 0.5 + TIME) * 0.2;

    // --- Audio Reactivity: direct output modulation ---
    float _ab = clamp(amp_peak, 0.0, 1.0);
    float _abass = clamp(amp_low, 0.0, 1.0);
    blendedColor *= 1.0 + _ab * 0.6;
    blendedColor = mix(blendedColor, blendedColor * vec3(1.0 + _abass * 0.3, 1.0 - _abass * 0.15, 1.0 + clamp(amp_high, 0.0, 1.0) * 0.25), _ab);
    // --- End Audio Reactivity ---

    gl_FragColor = vec4(blendedColor, 1.0);
    gl_FragColor = mix(gl_FragColor, texture(inputImage, isf_FragNormCoord), 0.5);
    float time_t = pingPong(TIME, 5.0) + 1.5;
    gl_FragColor = gl_FragColor * sin(vec4(blendedColor, 1.0) * time_t);
    gl_FragColor.a = 1.0;
}
