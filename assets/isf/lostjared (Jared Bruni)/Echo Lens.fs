/*{
  "DESCRIPTION": "(lostjared/shaders: codex_polar_echo_lens)",
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






vec2 mirrorWrap(vec2 p) {
    return abs(fract(p) * 2.0 - 1.0);
}

void main(void) {
    float t = TIME * 0.45;
    vec2 p = isf_FragNormCoord - 0.5;
    p.x *= RENDERSIZE.x / max(RENDERSIZE.y, 1.0);
    vec2 mouseUV = (iMouse.z > 0.0) ? (iMouse.xy / max(RENDERSIZE, vec2(1.0))) : vec2(0.5);
    vec2 mouseP = mouseUV * 2.0 - 1.0;
    mouseP.x *= RENDERSIZE.x / max(RENDERSIZE.y, 1.0);
    p -= mouseP * 0.08;
    float r = length(p);
    float a = atan(p.y, p.x);
    float lens = 1.0 / max(0.18, r + 0.08 * sin(a * 5.0 + t));
    vec2 q = vec2(a / 6.283185 + 0.5 + t * 0.05, fract(lens * 0.23 - t * 0.08));
    vec3 acc = vec3(0.0);
    acc += texture(inputImage, mirrorWrap(q)).rgb * 0.50;
    acc += texture(inputImage, mirrorWrap(q + vec2(0.033, 0.061))).rgb * 0.25;
    acc += texture(inputImage, mirrorWrap(q - vec2(0.071, 0.027))).rgb * 0.25;
    acc += texture(inputImage, mirrorWrap(q + (mouseP - p) * 0.04)).rgb * smoothstep(1.25, 0.0, length(p - mouseP)) * 0.18;
    float rings = smoothstep(0.96, 1.0, sin(lens * 10.0 + t * 4.0) * 0.5 + 0.5);
    vec3 tint = 0.55 + 0.45 * cos(vec3(0.4, 2.5, 4.6) + a * 2.0 - t);
    gl_FragColor = vec4(acc * (0.82 + rings * 0.55) + tint * rings * 0.25, 1.0);
}
