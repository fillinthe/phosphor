/*{
  "DESCRIPTION": "(lostjared/shaders: codex_grad_fractal_aurora_mandala)",
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






vec3 palette(float t) {
    return 0.5 + 0.5 * cos(6.28318 * (vec3(0.32, 0.56, 0.82) + t));
}

void main(void) {
    float aspect = RENDERSIZE.x / max(RENDERSIZE.y, 1.0);
    vec2 p = (isf_FragNormCoord - 0.5) * vec2(aspect, 1.0);
    vec2 mouseUV = (iMouse.z > 0.0) ? (iMouse.xy / max(RENDERSIZE, vec2(1.0))) : vec2(0.5);
    vec2 mouseP = mouseUV * 2.0 - 1.0;
    mouseP.x *= aspect;
    p -= mouseP * 0.10;
    float r = length(p);
    float a = atan(p.y, p.x);
    vec2 dir = vec2(cos(a), sin(a));
    float petal = abs(sin(a * 8.0 + sin(r * 14.0 - TIME) * 1.5));
    vec2 z = dir * (r + petal * 0.12);
    for (int i = 0; i < 5; ++i) {
        z = abs(z * 1.52) - 0.48;
        z += sin(z.yx * 4.0 + TIME) * 0.025;
    }
    z += (mouseP - p) * 0.04 * smoothstep(1.35, 0.0, length(p - mouseP));
    float veil = pow(1.0 - petal, 3.0) + pow(max(0.0, 1.0 - r * 1.5), 2.0);
    vec2 uv = abs(fract(isf_FragNormCoord + z * 0.035) * 2.0 - 1.0);
    vec3 tex = texture(inputImage, uv).rgb;
    vec3 grad = palette(dot(dir, vec2(0.16, 0.09)) + r * 0.7 + TIME * 0.04);
    gl_FragColor = vec4(mix(tex, grad, 0.7) + grad * veil * 0.55, 1.0);
}
