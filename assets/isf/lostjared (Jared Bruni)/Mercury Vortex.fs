/*{
  "DESCRIPTION": "Spectrum drives: swirl strength (bass), metallic specular (mid), refraction index (treble) (lostjared/shaders: random_spectrum_mercury_vortex)",
  "CREDIT": "Jared Bruni (lostjared/shaders, MIT)",
  "ISFVSN": "2",
  "CATEGORIES": [
    "lostjared"
  ],
  "INPUTS": [
    {
      "NAME": "inputImage",
      "TYPE": "image"
    },
    {
      "NAME": "audioFFT",
      "TYPE": "audioFFT"
    }
  ]
}*/

float lj_spec(float x) { return IMG_NORM_PIXEL(audioFFT, vec2(clamp(x, 0.0, 1.0), 0.5)).r; }
vec4 lj_spec4(float x) { return vec4(lj_spec(x)); }
float lj_band(float a, float b) { return (lj_spec(a) + lj_spec(mix(a, b, 0.33)) + lj_spec(mix(a, b, 0.66)) + lj_spec(b)) * 0.25; }
#define amp_low lj_band(0.0, 0.08)
#define amp_mid lj_band(0.08, 0.35)
#define amp_high lj_band(0.35, 0.9)
#define amp_rms lj_band(0.0, 0.5)
#define amp_smooth lj_band(0.0, 0.5)
#define amp_peak max(lj_band(0.0, 0.1), lj_band(0.1, 0.5))
#define amp lj_band(0.0, 0.5)
#define iamp lj_band(0.0, 0.5)
#define iMouse vec4(RENDERSIZE * 0.5, 0.0, 0.0)


// Remix: gem-ripple (swirl/vortex) + gem_metal (metallic sheen) + gem_glass (refraction normals)
// Spectrum drives: swirl strength (bass), metallic specular (mid), refraction index (treble)
// Creates a liquid mercury vortex that bends video through itself








const float PI = 3.14159265;

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
               mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
}

void main() {
    float bass   = lj_spec4( 0.03).r;
    float lowMid = lj_spec4( 0.10).r;
    float mid    = lj_spec4( 0.22).r;
    float hiMid  = lj_spec4( 0.40).r;
    float treble = lj_spec4( 0.58).r;

    vec2 uv = (isf_FragNormCoord - 0.5) * 2.0;
    uv.x *= RENDERSIZE.x / RENDERSIZE.y;

    float r = length(uv);
    float angle = atan(uv.y, uv.x);

    // === Vortex swirl from gem-ripple: bass drives strength ===
    float swirlStrength = (1.0 + bass * 4.0) * exp(-r * 0.8);
    float swirlAngle = angle + swirlStrength * sin(r * 8.0 - TIME * 3.0);
    vec2 swirlUV = vec2(cos(swirlAngle), sin(swirlAngle)) * r;

    // === Refraction from gem_glass: treble drives index ===
    // Fake surface normal from noise derivatives
    float eps = 0.002;
    float h = noise(swirlUV * 6.0 + TIME * 0.5);
    float hx = noise((swirlUV + vec2(eps, 0.0)) * 6.0 + TIME * 0.5);
    float hy = noise((swirlUV + vec2(0.0, eps)) * 6.0 + TIME * 0.5);
    vec2 normal = vec2(hx - h, hy - h) / eps;

    float eta = 0.7 + treble * 0.6;  // 0.7 to 1.3 IOR range
    vec2 refracted = normalize(vec2(uv) + normal * eta);

    // Convert back to texture coordinates
    vec2 texUV = refracted * 0.5 + 0.5;

    // === Metallic specular from gem_metal ===
    float ringDensity = 10.0 + mid * 20.0;
    float jagged = sin(angle * (6.0 + lowMid * 10.0) + TIME) * 0.04;
    float metalRing = sin((r + jagged) * ringDensity - TIME * 2.0);
    float specular = pow(max(0.0, metalRing), 4.0 + hiMid * 4.0);

    // Fresnel reflection
    float fresnel = pow(1.0 - abs(dot(normalize(vec3(uv, 1.0)), vec3(0.0, 0.0, 1.0))), 3.0);

    // Chromatic aberration through refraction
    float abr = 0.015 + treble * 0.02;
    vec3 tex;
    tex.r = texture(inputImage, texUV + abr * normal).r;
    tex.g = texture(inputImage, texUV).g;
    tex.b = texture(inputImage, texUV - abr * normal).b;

    // Mercury/chrome gl_FragColor
    vec3 metalColor = vec3(0.8, 0.85, 0.9);
    metalColor += vec3(bass * 0.3, mid * 0.2, treble * 0.4);

    // === Compose ===
    vec3 refractedTex = tex * (1.0 - fresnel * 0.6);
    vec3 reflection = metalColor * specular * (0.3 + mid * 0.5);
    vec3 result = refractedTex + reflection;

    // Hot center glow (like a liquid metal pool center)
    float poolGlow = exp(-r * (2.0 - bass * 1.2)) * (0.5 + bass * 0.6);
    result += poolGlow * vec3(0.9 + mid * 0.1, 0.7 + treble * 0.3, 0.5);

    // Edge vortex darkening
    result *= smoothstep(2.0, 0.5, r);

    // Peak flash — bright specular burst
    result += amp_peak * specular * 0.5;

    gl_FragColor = vec4(result, 1.0);
}
