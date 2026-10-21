/*{
  "DESCRIPTION": "(lostjared/shaders: game_codex_radar_ping)",
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

// Radar ping: green sweep line and expanding detection rings.






void main(void) {
    vec2 p = isf_FragNormCoord - 0.5;
    p.x *= RENDERSIZE.x / max(RENDERSIZE.y, 1.0);
    float r = length(p);
    float a = atan(p.y, p.x);
    float sweepA = TIME * 2.2;
    float sweep = 1.0 - smoothstep(0.0, 0.18, abs(atan(sin(a - sweepA), cos(a - sweepA))));
    float ring = smoothstep(0.015, 0.0, abs(r - mod(TIME * 0.22, 0.55)));
    vec3 c = texture(inputImage, isf_FragNormCoord).rgb * vec3(0.75, 1.0, 0.75);
    c += vec3(0.1, 1.0, 0.25) * (sweep * smoothstep(0.55, 0.02, r) + ring) * 0.65;
    gl_FragColor = vec4(c, 1.0);
}
