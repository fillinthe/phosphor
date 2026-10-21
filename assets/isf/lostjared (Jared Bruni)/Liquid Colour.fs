/*{
  "DESCRIPTION": "(lostjared/shaders: sin_delic)",
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

void main(void) {
    vec2 uv = isf_FragNormCoord;
    uv.x += sin(uv.y * 10.0 + TIME) * 0.1;
    uv.y += cos(uv.x * 10.0 + TIME) * 0.1;
    float modFactor = pingPong(TIME * 0.5, 1.0);
    vec4 texColor = texture(inputImage, uv);
    vec3 psychedelicColor = vec3(
        texColor.r * sin(TIME + uv.y * 10.0),
        texColor.g * cos(TIME + uv.x * 10.0),
        texColor.b * sin(TIME + uv.y * 5.0)
    );
    psychedelicColor = mix(texColor.rgb, psychedelicColor, modFactor);
    float time_t =  pingPong(TIME, 10.0) + 2.0;
    gl_FragColor = vec4(sin(psychedelicColor * time_t), texColor.a);
}

