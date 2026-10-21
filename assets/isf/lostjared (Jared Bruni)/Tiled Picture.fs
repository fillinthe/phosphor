/*{
  "DESCRIPTION": "(lostjared/shaders: spiral_square)",
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







vec2 rotate(vec2 v, float a) {
    float s = sin(a);
    float c = cos(a);
    mat2 m = mat2(c, -s, s, c);
    return m * v;
}

void main() {
    float time_t = mod(TIME, 15.0);
    vec2 uv = isf_FragNormCoord;
    uv = uv * 2.0 - 1.0;
    uv = rotate(uv, time_t);
    uv += 2.0;
    uv = fract(uv * time_t);
    gl_FragColor = texture(inputImage, uv);
}
