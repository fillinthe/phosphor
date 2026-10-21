/*{
  "DESCRIPTION": "(lostjared/shaders: gem-light-frac)",
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






// Helper for smoother gl_FragColor cycling
vec3 palette(float t) {
    vec3 a = vec3(0.5, 0.5, 0.5);
    vec3 b = vec3(0.5, 0.5, 0.5);
    vec3 c = vec3(1.0, 1.0, 1.0);
    vec3 d = vec3(0.263, 0.416, 0.557);
    return a + b * cos(6.28318 * (c * t + d));
}

void main(void) {
    // 1. Setup coordinates (centered and aspect-corrected)
    vec2 uv = (isf_FragNormCoord * 2.0 - 1.0) * vec2(RENDERSIZE.x / RENDERSIZE.y, 1.0);
    vec2 uv0 = uv; // Store original UVs for global glow
    vec3 finalColor = vec3(0.0);

    // 2. The Fractal Loop
    // This is where the magic happens. We iterate to create layers.
    for (float i = 0.0; i < 4.0; i++) {
        
        // FRACTAL FOLDING: 
        // This 'fract' creates the repetition. The '-0.5' re-centers each "tile".
        uv = fract(uv * 1.5) - 0.5;

        // Calculate distance from center of the current fold
        float d = length(uv) * exp(-length(uv0));

        // Create a pulsing spectral gl_FragColor based on depth (i) and time
        vec3 col = palette(length(uv0) + i * 0.4 + TIME * 0.4);

        // This equation creates the "neon" thin lines
        // d starts as distance, sin makes it a wave, abs makes it a sharp line
        d = sin(d * 8.0 + TIME) / 8.0;
        d = abs(d);

        // Intensify the glow (inverse relationship)
        d = pow(0.01 / d, 1.2);

        finalColor += col * d;
    }

    // 3. Texture Integration
    // We distort the texture lookup using the fractal coordinates
    vec2 distortion = uv * 0.1;
    vec4 texColor = texture(inputImage, isf_FragNormCoord + distortion);

    // Mix the fractal neon with the texture
    vec3 composite = mix(texColor.rgb, finalColor, 0.6);
    
    // Add a bit of your original "sin" madness for the finish
    gl_FragColor = vec4(composite, texColor.a);
}