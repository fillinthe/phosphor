/*{
  "DESCRIPTION": "(lostjared/shaders: crystal)",
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
    float radius = 0.5; // Example hardcoded radius
    vec2 center = vec2(RENDERSIZE.x / 2.0, RENDERSIZE.y / 2.0); // Hardcoded center to the middle of the screen

    vec2 texCoord = isf_FragNormCoord * RENDERSIZE;
    vec2 delta = texCoord - center;
    float dist = length(delta);
    float maxRadius = min(RENDERSIZE.x, RENDERSIZE.y) * radius;

    vec2 newTexCoord = texCoord;

    if (dist < maxRadius) {
        float scaleFactor = 1.0 - sqrt(dist / maxRadius);
        newTexCoord = center + delta * scaleFactor;
    }

    newTexCoord = clamp(newTexCoord / RENDERSIZE, 0.0, 1.0);
    vec4 texColor = texture(inputImage, newTexCoord);
    
    gl_FragColor = texColor;
}
