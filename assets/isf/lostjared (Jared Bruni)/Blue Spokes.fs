/*{
  "DESCRIPTION": "(lostjared/shaders: gem-spoke)",
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
    // 1. Center and normalize coordinates
    vec2 uv = (isf_FragNormCoord * 2.0 - 1.0);
    uv.x *= RENDERSIZE.x / RENDERSIZE.y; // Maintain aspect ratio
    
    // 2. Fractal parameters
    float zoom = sin(TIME * 0.2) * 0.5 + 1.5;
    vec3 fractalCol = vec3(0.0);
    float scale = 1.0;

    // 3. The Iterative Fold (The "Fractal" part)
    // We loop to create layers of self-similarity
    for (int i = 0; i < 6; i++) {
        // Folding space: mirroring the UVs creates symmetrical complexity
        uv = abs(uv) - 0.5; 
        
        // Rotation over time to create a "Kaleidoscope" effect
        float a = TIME * 0.3;
        float s = sin(a), c = cos(a);
        uv *= mat2(c, -s, s, c);
        
        // Scaling space inward
        uv *= 1.2;
        scale *= 1.2;

        // Calculate a "distance" based on the warped coordinates
        float d = length(uv);
        
        // Add glowing edges based on your original 'spoke' logic
        float glow = 0.01 / abs(sin(d * 8.0 - TIME) / 8.0);
        fractalCol += glow * vec3(0.2, 0.5, 1.0) / scale;
    }

    // 4. Texture Mapping
    // Use the warped UVs to sample the texture for a "infinite mirror" look
    vec4 texColor = texture(inputImage, fract(uv + TIME * 0.1));
    
    // 5. Combine original glow logic with fractal structure
    vec3 finalRGB = texColor.rgb * fractalCol;
    
    // Subtle vignette
    finalRGB *= smoothstep(1.5, 0.5, length(isf_FragNormCoord * 2.0 - 1.0));

    gl_FragColor = vec4(finalRGB, 1.0);
}