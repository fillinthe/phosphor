/*{
  "DESCRIPTION": "(lostjared/shaders: sine1)",
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






float pingPong(float x, float length) {
    float modVal = mod(x, length * 2.0);
    return modVal <= length ? modVal : length * 2.0 - modVal;
}
vec3 hsv2rgb(vec3 c) {
    vec4 K = vec4(1.0, 2.0/3.0, 1.0/3.0, 3.0);
    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

void main(void) {
    vec2 uv = isf_FragNormCoord * RENDERSIZE.xy / RENDERSIZE.y;
    float pattern = sin(uv.x * 10.0) * cos(uv.y * 10.0);

    pattern = (pattern + 1.0) * 0.5;

    float reveal = pingPong(TIME * 0.5, 1.0);

    float cracks = step(pattern, reveal);

    float hue = pattern * 6.0;

    vec3 colorRGB = hsv2rgb(vec3(hue, 1.0, 1.0));

    gl_FragColor = vec4(colorRGB * cracks, 1.0);
    gl_FragColor = mix(gl_FragColor,texture(inputImage,isf_FragNormCoord), 0.5);
    gl_FragColor.a = 1.0;
    
}

