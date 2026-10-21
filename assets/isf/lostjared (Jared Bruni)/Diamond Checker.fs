/*{
  "DESCRIPTION": "(lostjared/shaders: dmd_gpt)",
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




float diamond(vec2 p){
    p = abs(p);
    float d = p.x + p.y;
    return 1.0 - smoothstep(0.48, 0.5, d);
}

void main(void) {
    float aspect = RENDERSIZE.x / RENDERSIZE.y;
    vec2 ar = vec2(aspect, 1.0);

    vec2 uv = isf_FragNormCoord;
    vec2 uva = uv * ar;

    float tiles = 8.0;
    vec2 gv = uva * tiles;
    vec2 f = fract(gv) - 0.5;

    float mask = diamond(f);

    vec2 mirrored = abs(fract(uv * ar) - 0.5) + 0.5;
    vec2 tuv = mirrored / ar;

    vec4 c1 = texture(inputImage, tuv);
    vec4 c2 = texture(inputImage, tuv * 0.5);
    vec4 c3 = texture(inputImage, tuv * 0.25);
    vec4 c4 = texture(inputImage, tuv * 0.125);
    vec4 mixcol = c1 * 0.4 + c2 * 0.3 + c3 * 0.2 + c4 * 0.1;

    mixcol.rgb *= mask;

    gl_FragColor = vec4(mixcol.rgb, 1.0);
}
