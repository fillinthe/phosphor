/*{
  "DESCRIPTION": "(lostjared/shaders: geometric3)",
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
    vec2 uv = isf_FragNormCoord * RENDERSIZE / vec2(RENDERSIZE.y);
    float time = TIME * 0.5;
    float time_t = mod(TIME, 15.0);
    float angle = time;
    vec2 center = vec2(0.5, 0.5) * RENDERSIZE / vec2(RENDERSIZE.y);
    vec2 toCenter = uv - center;
    float radius = length(toCenter);
    float theta = atan(toCenter.y, toCenter.x) + time;
    float pattern = abs(sin(12.0 * theta) * cos(12.0 * radius));
    vec3 colorShift = vec3(0.5 * sin(time) + 0.5, 0.5 * cos(time) + 0.5, sin(radius - time));
    vec3 finalColor = (0.2 + 0.8 * pattern) + colorShift * pattern;
    vec4 text_color = texture(inputImage, isf_FragNormCoord);
    vec4 fc = vec4(finalColor, 1.0);
    gl_FragColor = sin(mix(text_color, fc, 0.3) * length(time_t));
    gl_FragColor.a = 1.0;
}
