/*{
  "DESCRIPTION": "(lostjared/shaders: twist-code-turbine-rift)",
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

void main(void) {
    float aspect = max(RENDERSIZE.x, 1.0) / max(RENDERSIZE.y, 1.0);
    vec2 ar = vec2(aspect, 1.0);
    vec2 p = (isf_FragNormCoord - 0.5) * ar;
    float r = length(p) + 0.001;
    float a = atan(p.y, p.x) + TIME * 1.8 + 0.8 / r;
    float bladeCount = 18.0;
    float blade = fract(a / TAU * bladeCount + r * 7.0);
    float edge = smoothstep(0.02, 0.22, blade) * (1.0 - smoothstep(0.68, 0.98, blade));
    float bend = sin(blade * TAU + r * 65.0 - TIME * 15.0);
    float qa = a + (blade - 0.5) * 0.5 + bend * 0.08;
    float qr = r + bend * 0.045;
    vec2 uv = fract(vec2(cos(qa), sin(qa)) * qr / ar + 0.5);
    vec4 c0 = texture(inputImage, uv);
    vec4 c1 = texture(inputImage, fract(uv.yx + vec2(blade, r) * 0.15));
    vec3 rgb = mix(c0.rgb, c1.bgr, (1.0 - edge) * 0.5);
    rgb *= 0.48 + edge * 0.9;
    gl_FragColor = vec4(rgb, c0.a);
}
