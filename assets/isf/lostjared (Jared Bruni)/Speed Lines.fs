/*{
  "DESCRIPTION": "(lostjared/shaders: game_speedlines)",
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

// Anime-style radial speed lines from screen center.






float hash(float n) { return fract(sin(n) * 43758.5453); }

void main(void) {
    vec3 c = texture(inputImage, isf_FragNormCoord).rgb;
    vec2 v = isf_FragNormCoord - 0.5;
    float a = atan(v.y, v.x);
    float r = length(v);
    float spokes = 64.0;
    float idx = floor(a / (6.2831853 / spokes));
    float seed = hash(idx);
    float band = step(0.5, fract(seed * 7.0 + TIME * (0.5 + seed)));
    float reach = smoothstep(0.18, 0.55, r) * band;
    vec3 line = mix(c, vec3(1.0), reach * 0.85);
    gl_FragColor = vec4(line, 1.0);
}
