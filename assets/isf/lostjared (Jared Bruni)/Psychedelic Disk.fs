/*{
  "DESCRIPTION": "(lostjared/shaders: disk2)",
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
    vec2 uv = isf_FragNormCoord * 2.0 - 1.0;
    float angle = atan(uv.y, uv.x);
    float radius = length(uv);
    
    float spinningSpeed = mix(5.0, 1.0, radius);
    float spinningAngle = angle + TIME * spinningSpeed;
    
    vec2 spinningUV = vec2(cos(spinningAngle), sin(spinningAngle)) * radius * 0.5 + 0.5;
    vec4 texColor = texture(inputImage, spinningUV);
    
    float centerGlow = exp(-radius * 10.0);
    float outerGlow = smoothstep(0.4, 0.5, radius) * 0.5 + 0.5;
    float glow = max(centerGlow, outerGlow);
    
    gl_FragColor = texColor * vec4(glow, glow, glow, 1.0);
    gl_FragColor = xor_RGB(gl_FragColor, texture(inputImage, isf_FragNormCoord));
}
