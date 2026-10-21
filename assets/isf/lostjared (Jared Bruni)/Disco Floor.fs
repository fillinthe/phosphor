/*{
  "DESCRIPTION": "(lostjared/shaders: game_disco_floor)",
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

// Disco floor: animated colored tile overlay, multiplied with the source.






float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

vec3 hue(float h) {
    return clamp(abs(mod(h * 6.0 + vec3(0.0, 4.0, 2.0), 6.0) - 3.0) - 1.0, 0.0, 1.0);
}

void main(void) {
    vec3 src = texture(inputImage, isf_FragNormCoord).rgb;
    vec2 cell = floor(isf_FragNormCoord * vec2(16.0, 9.0));
    float seed = hash(cell);
    float beat = step(0.5, fract(TIME * 1.5 + seed));
    vec3 tile = hue(seed + TIME * 0.2);
    vec3 mixed = src * mix(vec3(0.7), tile * 1.6, beat * 0.7);
    gl_FragColor = vec4(mixed, 1.0);
}
