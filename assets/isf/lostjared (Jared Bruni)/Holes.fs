/*{
  "DESCRIPTION": "(lostjared/shaders: dmdc)",
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






float pingPong(float t, float l) {
    return abs(mod(t, 2.0 * l) - l);
}

mat2 rotate(float angle) {
    float c = cos(angle);
    float s = sin(angle);
    return mat2(c, -s, s, c);
}

void main() {
    vec2 uv = isf_FragNormCoord * RENDERSIZE.xy;
    vec2 centerUV = (floor(uv / 50.0) + 0.5) * 50.0;
    vec2 localUV = uv - centerUV;
    float t = pingPong(TIME, 4.0) * 2.0;
    float rotation = atan(localUV.y, localUV.x) + length(localUV) * 0.1 + t;
    localUV = rotate(rotation) * localUV;
    vec2 diamondMask = abs(mod(localUV, 100.0) - 50.0);
    float mask = smoothstep(25.0, 30.0, length(diamondMask - vec2(25.0)));
    vec2 texCoord = (centerUV + localUV) / RENDERSIZE.xy;
    vec4 sampledColor = texture(inputImage, texCoord);
    gl_FragColor = mix(sampledColor, vec4(0.0), mask);
}
