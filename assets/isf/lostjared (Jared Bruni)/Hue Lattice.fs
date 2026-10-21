/*{
  "DESCRIPTION": "Optimized Hue adjustment for smooth transitions (lostjared/shaders: gem-hue-frac)",
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





// Helper to create the back-and-forth motion
float pingPong(float x, float length) {
    float modVal = mod(x, length * 2.0);
    return modVal <= length ? modVal : length * 2.0 - modVal;
}

// Optimized Hue adjustment for smooth transitions
vec3 hueShift(vec3 col, float hue) {
    const vec3 k = vec3(0.57735, 0.57735, 0.57735);
    float cosAngle = cos(hue);
    return col * cosAngle + cross(k, col) * sin(hue) + k * dot(k, col) * (1.0 - cosAngle);
}

void main() {
    // 1. Center and aspect-correct UVs
    vec2 uv = (isf_FragNormCoord - 0.5) * RENDERSIZE / min(RENDERSIZE.x, RENDERSIZE.y);
    vec2 uv0 = uv; // Store original UV for global reference
    
    vec3 finalCol = vec3(0.0);
    float t = TIME * 0.2; // Slow down time for a trippier effect

    // 2. The Fractal Loop
    // Iterating and folding space creates the fractal "repetition"
    for (float i = 0.0; i < 4.0; i++) {
        // Fold space (Kaleidoscopic effect)
        uv = fract(uv * 1.5) - 0.5;

        // Calculate distance with a pulsing variance
        float d = length(uv) * exp(-length(uv0));

        // Create the "neon" edge/ring effect based on time
        vec3 col = vec3(0.5, 0.8, 0.9); // Base tint
        d = sin(d * 8.0 + t) / 8.0;
        d = abs(d);
        d = pow(0.01 / d, 1.2); // Intensify the edges into "glow"

        // Shift hue per iteration for gl_FragColor depth
        finalCol += col * d;
    }

    // 3. Texture Sampling with Distortion
    // We use the fractal math to displace the texture lookup
    float distortion = length(finalCol.rg) * 0.05;
    vec4 sampledColor = texture(inputImage, isf_FragNormCoord + distortion);

    // 4. Final Color Correction
    float shiftAmt = pingPong(TIME, 5.0);
    vec3 shiftedColor = hueShift(sampledColor.rgb + (finalCol * 0.5), shiftAmt);
    
    gl_FragColor = vec4(shiftedColor, 1.0);
}