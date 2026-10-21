/*{
  "DESCRIPTION": "(lostjared/shaders: diamond)",
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





vec3 hash3(vec2 p) {
    vec3 q = vec3(dot(p, vec2(127.1, 311.7)),
                  dot(p, vec2(269.5, 183.3)),
                  dot(p, vec2(419.2, 371.9)));
    return fract(sin(q) * 43758.5453);
}

vec4 xor_RGB(vec4 icolor, vec4 source) {
    ivec3 int_color;
    ivec4 isource = ivec4(source * 255);
    for(int i = 0; i < 3; ++i) {
        int_color[i] = int(255 * icolor[i]);
        int_color[i] = int_color[i]^isource[i];
        if(int_color[i] > 255)
            int_color[i] = int_color[i]%255;
        icolor[i] = float(int_color[i])/255;
    }
    icolor.a = 1.0;
return icolor;
}

void main(void) {
    vec2 uv = isf_FragNormCoord * RENDERSIZE / vec2(RENDERSIZE.y);
    vec2 skewedUv = uv;
    skewedUv.x = (uv.x + uv.y) * 0.5;
    skewedUv.y = (uv.x - uv.y) * 0.5;
    float scale = abs(sin(TIME)) * 20.0 + 1.0;
    skewedUv *= scale;
    vec2 c = floor(skewedUv);
    vec2 checker = mod(c, 2.0);
    bool isEvenDiamond = (checker.x == checker.y);
    vec3 diamondColor = hash3(c);
    vec3 finalColor = isEvenDiamond ? diamondColor * 0.8 : diamondColor * 0.5;
    gl_FragColor = xor_RGB(vec4(finalColor, 1.0), texture(inputImage, isf_FragNormCoord));
    
}
