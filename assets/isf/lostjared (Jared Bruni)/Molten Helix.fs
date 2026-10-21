/*{
  "DESCRIPTION": "(lostjared/shaders: liquid_code_molten_helix)",
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











const float PI = 3.14159265359;
const float TAU = 6.28318530718;

float hash21(vec2 p) {
    p = fract(p * vec2(443.8975, 397.2973));
    p += dot(p, p.yx + 19.19);
    return fract((p.x + p.y) * p.x);
}

float noise21(vec2 p) {
    vec2 cell = floor(p);
    vec2 local = fract(p);
    local = local * local * (3.0 - 2.0 * local);
    return mix(mix(hash21(cell), hash21(cell + vec2(1.0, 0.0)), local.x),
               mix(hash21(cell + vec2(0.0, 1.0)), hash21(cell + vec2(1.0)), local.x),
               local.y);
}

float flowFbm(vec2 p) {
    float result = 0.0;
    float amplitude = 0.52;
    mat2 twist = mat2(0.64, -0.77, 0.77, 0.64);
    for (int i = 0; i < 6; ++i) {
        result += noise21(p) * amplitude;
        p = twist * p * 2.06 + vec2(1.37, 4.11);
        amplitude *= 0.49;
    }
    return result;
}

vec2 mirrorUV(vec2 uv) {
    return 1.0 - abs(mod(uv, 2.0) - 1.0);
}

vec3 heatPalette(float x) {
    vec3 cold = vec3(0.025, 0.055, 0.11);
    vec3 copper = vec3(0.92, 0.20, 0.035);
    vec3 gold = vec3(1.0, 0.68, 0.16);
    vec3 white = vec3(1.0, 0.94, 0.78);
    vec3 result = mix(cold, copper, smoothstep(0.05, 0.46, x));
    result = mix(result, gold, smoothstep(0.42, 0.72, x));
    return mix(result, white, smoothstep(0.72, 1.0, x));
}

vec3 spectral(float x) {
    return 0.5 + 0.5 * cos(TAU * (x + vec3(0.0, 0.31, 0.65)));
}

float helixPotential(vec2 p) {
    float t = TIME;
    vec2 centers[2] = vec2[2](vec2(-0.22, 0.0), vec2(0.22, 0.0));
    float field = 0.0;
    for (int i = 0; i < 2; ++i) {
        vec2 d = p - centers[i];
        float r = max(length(d), 0.025);
        float a = atan(d.y, d.x);
        float handedness = (i == 0) ? 1.0 : -1.0;
        float coil = a * 4.0 * handedness + log(r) * 8.5 - t * (1.8 + float(i) * 0.25);
        field += sin(coil + r * 24.0) * exp(-r * 1.25) * 0.3;
        field += cos(coil * 2.0 - r * 11.0) * exp(-r * 2.3) * 0.1;
    }

    vec2 q = p;
    float warp0 = flowFbm(q * 3.4 + vec2(t * 0.12, -t * 0.1));
    float warp1 = flowFbm(q * 3.4 + vec2(-t * 0.09, t * 0.13) + 9.7);
    q += (vec2(warp0, warp1) - 0.5) * 0.23;
    float viscosity = flowFbm(q * 7.0 + vec2(t * 0.14, -t * 0.11));
    float crossWave = sin(q.y * 32.0 + sin(q.x * 9.0 - t) * 3.0 - t * 4.6);
    return field + viscosity * 0.58 + crossWave * 0.075;
}

vec3 liquidNormal(vec2 p, float e) {
    float left = helixPotential(p - vec2(e, 0.0));
    float right = helixPotential(p + vec2(e, 0.0));
    float down = helixPotential(p - vec2(0.0, e));
    float up = helixPotential(p + vec2(0.0, e));
    vec2 gradient = vec2(right - left, up - down) / (2.0 * e);
    return normalize(vec3(-gradient * 0.1, 1.0));
}

vec2 helixVelocity(vec2 p) {
    vec2 centers[2] = vec2[2](vec2(-0.22, 0.0), vec2(0.22, 0.0));
    vec2 velocity = vec2(0.0);
    for (int i = 0; i < 2; ++i) {
        vec2 d = p - centers[i];
        float r2 = dot(d, d) + 0.025;
        float handedness = (i == 0) ? 1.0 : -1.0;
        velocity += vec2(-d.y, d.x) * handedness / r2;
    }
    return velocity;
}

vec3 advectedTexture(vec2 uv, vec2 velocity, vec2 normalDirection, float split) {
    vec2 direction = normalize(velocity + normalDirection * 0.8 + vec2(0.0001));
    vec2 across = vec2(-direction.y, direction.x);
    vec3 smear = texture(inputImage, mirrorUV(uv)).rgb * 0.4;
    smear += texture(inputImage, mirrorUV(uv + direction * 0.004)).rgb * 0.24;
    smear += texture(inputImage, mirrorUV(uv - direction * 0.004)).rgb * 0.24;
    smear += texture(inputImage, mirrorUV(uv + across * 0.0025)).rgb * 0.06;
    smear += texture(inputImage, mirrorUV(uv - across * 0.0025)).rgb * 0.06;
    return vec3(texture(inputImage, mirrorUV(uv + across * split)).r,
                smear.g,
                texture(inputImage, mirrorUV(uv - across * split)).b);
}

float specularLobe(vec3 n, vec3 v, vec3 l, float power) {
    return pow(max(dot(n, normalize(v + l)), 0.0), power);
}

vec3 moltenLighting(vec3 base, vec3 n, vec2 p, float heat) {
    vec3 view = normalize(vec3(-p * 0.11, 1.4));
    vec3 key = normalize(vec3(-0.62, 0.48, 0.73));
    vec3 fill = normalize(vec3(0.71, 0.34, 0.61));
    vec3 rim = normalize(vec3(0.08, -0.78, 0.62));
    float nv = max(dot(n, view), 0.0);
    vec3 f0 = mix(vec3(0.66), base, 0.55);
    vec3 fresnel = f0 + (1.0 - f0) * pow(1.0 - nv, 5.0);
    vec3 reflected = reflect(-view, n);
    vec3 environment = mix(vec3(0.018, 0.028, 0.06),
                           spectral(reflected.y * 0.2 + heat * 0.08 + TIME * 0.018),
                           smoothstep(-0.55, 0.8, reflected.z));

    vec3 result = base * (0.035 + 0.08 * max(dot(n, key), 0.0));
    result += fresnel * environment * 1.2;
    result += specularLobe(n, view, key, 150.0) * vec3(1.0, 0.7, 0.36) * 2.6;
    result += specularLobe(n, view, fill, 55.0) * vec3(0.35, 0.62, 1.0) * 1.2;
    result += specularLobe(n, view, rim, 24.0) * vec3(0.9, 0.2, 0.08) * 0.75;
    return result;
}

vec3 toneMap(vec3 value) {
    value = max(value, 0.0);
    return 1.0 - exp(-value * (1.03 + value * 0.1));
}

void main(void) {
    float aspect = max(RENDERSIZE.x, 1.0) / max(RENDERSIZE.y, 1.0);
    vec2 p = (isf_FragNormCoord - 0.5) * vec2(aspect, 1.0);
    float e = 1.6 / max(max(RENDERSIZE.x, RENDERSIZE.y), 320.0);
    float height = helixPotential(p);
    vec3 n = liquidNormal(p, e);
    vec2 velocity = helixVelocity(p);
    velocity = velocity / (1.0 + length(velocity) * 2.2);

    float flowStrength = 0.042 + amp_low * 0.025;
    vec2 flow = velocity * flowStrength + n.xy * (0.017 + amp_smooth * 0.016);
    flow += vec2(sin(p.y * 12.0 - TIME * 1.8), cos(p.x * 11.0 + TIME * 1.5)) * 0.004;
    vec2 uv = mirrorUV(isf_FragNormCoord + flow / vec2(aspect, 1.0));

    float split = 0.0018 + amp_high * 0.006;
    vec3 source = advectedTexture(uv, velocity, n.xy, split);
    float luminance = dot(source, vec3(0.299, 0.587, 0.114));
    vec3 alloy = mix(vec3(luminance) * vec3(0.86, 0.79, 0.72), source, 0.42);
    float heat = smoothstep(0.36, 0.92, flowFbm(p * 5.0 - vec2(TIME * 0.1, 0.0)));
    vec3 result = moltenLighting(alloy, n, p, heat);

    float vein = pow(0.5 + 0.5 * sin(height * 12.0 - TIME * 2.8), 12.0);
    vec3 emission = heatPalette(clamp(heat + vein * 0.35, 0.0, 1.0));
    result += emission * vein * (0.28 + amp_peak * 1.35);
    result += heatPalette(heat) * heat * heat * 0.13;
    result = mix(result, result * (0.6 + source), 0.2);
    gl_FragColor = vec4(toneMap(result * (1.0 + amp_smooth * 0.27)), texture(inputImage, uv).a);
}
