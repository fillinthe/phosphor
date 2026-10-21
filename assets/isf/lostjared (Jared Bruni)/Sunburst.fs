/*{
  "DESCRIPTION": "(lostjared/shaders: bi_cycle)",
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





void main(void) {
    vec2 uv = isf_FragNormCoord * 2.0 - 1.0;
    float angle = atan(uv.y, uv.x);
    float radius = length(uv);

    int numSpokes = 32;
    float spokeWidth = 0.05;
    float spokeAngle = radians(360.0 / float(numSpokes));
    float spokePattern = mod(angle + TIME * 2.0, spokeAngle) - spokeAngle / 2.0;
    float spoke = smoothstep(-spokeWidth, 0.0, spokePattern) - smoothstep(0.0, spokeWidth, spokePattern);
    float centerGlow = exp(-radius * 20.0);
    vec4 texColor = texture(inputImage, uv * 0.5 + 0.5);
    float glow = max(spoke, centerGlow);
    
    gl_FragColor = texColor * vec4(glow, glow, glow, 1.0);
}
