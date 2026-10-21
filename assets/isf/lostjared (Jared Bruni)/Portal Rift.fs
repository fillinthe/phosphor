/*{
  "DESCRIPTION": "(lostjared/shaders: game_codex_portal_rift)",
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

// Portal rift: twisting center tear with cyan-magenta energy.






mat2 rot(float a) {
    float s = sin(a), c = cos(a);
    return mat2(c, -s, s, c);
}

void main(void) {
    vec2 p = isf_FragNormCoord - 0.5;
    p.x *= RENDERSIZE.x / max(RENDERSIZE.y, 1.0);
    float r = length(p);
    float twist = (0.6 - r) * 4.0 * smoothstep(0.55, 0.0, r);
    vec2 q = rot(twist + TIME * 0.8) * p;
    vec2 uv = q / max(0.35 + r, 0.1) + 0.5;
    vec3 c = texture(inputImage, clamp(uv, 0.0, 1.0)).rgb;
    float rim = smoothstep(0.035, 0.0, abs(r - 0.28));
    vec3 energy = 0.55 + 0.45 * cos(vec3(0.0, 2.2, 4.1) + atan(p.y, p.x) * 3.0 + TIME * 4.0);
    c += energy * rim * 1.2;
    gl_FragColor = vec4(c, 1.0);
}
