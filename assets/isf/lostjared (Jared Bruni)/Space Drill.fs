/*{
  "DESCRIPTION": "(lostjared/shaders: twist-code-deep-space-drill)",
  "CREDIT": "Jared Bruni (lostjared/shaders, MIT)",
  "ISFVSN": "2",
  "CATEGORIES": [
    "lostjared"
  ],
  "INPUTS": [
    {
      "NAME": "inputImage",
      "TYPE": "image"
    }
  ]
}*/

#define iMouse vec4(RENDERSIZE * 0.5, 0.0, 0.0)





const float TAU = 6.28318530718;

vec2 mirrorRepeat(vec2 p) {
    return 1.0 - abs(mod(p, 2.0) - 1.0);
}

void main(void) {
    float aspect = max(RENDERSIZE.x, 1.0) / max(RENDERSIZE.y, 1.0);
    vec2 p = (isf_FragNormCoord - 0.5) * vec2(aspect, 1.0);
    float r = length(p) + 0.001;
    float a = atan(p.y, p.x);
    float depth = 1.0 / (r + 0.045) + TIME * 2.0;
    float thread = sin(a * 9.0 - depth * 3.4);
    float thread2 = cos(a * 17.0 + depth * 5.1);
    float drillA = a + depth * 0.72 + thread * 0.16;
    vec2 uv = mirrorRepeat(vec2(drillA / TAU * 4.0 + thread2 * 0.06,
                                depth * 0.34 + log(r) * 0.7 + thread * 0.11));
    vec2 uv2 = mirrorRepeat(uv * vec2(0.83, 1.19) + vec2(thread, thread2) * 0.08);
    vec4 c0 = texture(inputImage, uv);
    vec4 c1 = texture(inputImage, uv2);
    float metal = pow(0.5 + 0.5 * thread, 3.0);
    vec3 rgb = mix(c0.rgb, c1.bgr, 0.32 + metal * 0.3);
    rgb *= 0.58 + metal * 0.9 + abs(thread2) * 0.2;
    gl_FragColor = vec4(rgb, c0.a);
}
