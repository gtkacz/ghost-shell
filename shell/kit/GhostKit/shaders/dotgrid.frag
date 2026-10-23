#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

// Lengths are physical pixels; `resolution` is the item's size in physical pixels.
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec4 dotColor;
    float spacing;
    float radius;
    vec2 resolution;
    // appearance.dots.motion, numbered as ThemeLib.dotMotion() numbers it.
    int mode;
    // Seconds of motion, already scaled by appearance.dots.speed.
    float time;
    float sizeRange;
    float brightness;
    // Physical pixels per design pixel, times appearance.dots.scale.
    float featureUnit;
    float cover;
    // The pointer in physical pixels, or far off the item when it is elsewhere.
    vec2 pointer;
};

// The reference mockup's radial gradient is solid out to the radius and fades out by 1.4 times it.
const float FADE_END_RATIO = 1.4;
// Below this the fade is narrower than a pixel and a shrunken dot would alias.
const float MIN_FADE_PX = 0.4;
// A grown dot stays clear of its neighbours' cells.
const float MAX_RADIUS_SPACING = 0.45;
// Brightness past this only saturates the colour.
const float MAX_GAIN = 5.0;
const float TAU = 6.2831853;

const int BREATHE = 1;
const int SWEEP = 2;
const int RIPPLE = 3;
const int CLOUDS = 4;
const int RAIN = 5;
const int PACKETS = 6;
const int INTERFERENCE = 7;
const int CURSOR = 8;

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}

// How far into its peak the dot whose cell is `cell` and centre is `centre` is, from 0 (at rest) to 1.
float motion(vec2 cell, vec2 centre) {
    float t = time;
    vec2 grid = resolution / spacing;
    if (mode == BREATHE) {
        return 0.5 + 0.5 * sin(t * TAU / 8.0);
    }
    if (mode == SWEEP) {
        float margin = 200.0 * featureUnit;
        float y = mod(t / 12.0, 1.0) * (resolution.y + 2.0 * margin) - margin;
        float k = (centre.y - y) / (70.0 * featureUnit);
        return exp(-k * k);
    }
    if (mode == RIPPLE) {
        float d = length(centre - 0.5 * resolution);
        return pow(0.5 + 0.5 * sin(d / (60.0 * featureUnit) - t * 1.2), 6.0);
    }
    if (mode == CLOUDS) {
        vec2 p = centre / (260.0 * featureUnit);
        float n = 0.65 * noise(p + vec2(t * 0.04, t * 0.025)) + 0.35 * noise(p * 2.13 - vec2(t * 0.03, -t * 0.05) + 17.0);
        return smoothstep(cover, cover + 0.4, n);
    }
    if (mode == RAIN) {
        float h = hash(vec2(cell.x, 9.0));
        // The cycle runs past the bottom, so a column rests between drops.
        float cycle = grid.y * (1.5 + 2.0 * hash(vec2(cell.x, 4.0)));
        float behind = mod(t * (4.0 + 8.0 * h) + h * 400.0, cycle) - cell.y;
        return behind >= 0.0 ? exp(-behind / (3.0 * featureUnit)) : 0.0;
    }
    if (mode == PACKETS) {
        float hr = hash(vec2(cell.y, 1.7));
        float hc = hash(vec2(cell.x, 5.3));
        float dr = mod(t * (6.0 + 6.0 * hr) + hr * 300.0, grid.x * 3.0) - cell.x;
        float dc = mod(t * (6.0 + 6.0 * hc) + hc * 300.0, grid.y * 3.0) - cell.y;
        float fr = step(0.88, hash(vec2(cell.y, 2.2))) * (dr >= 0.0 ? exp(-dr / (2.0 * featureUnit)) : 0.0);
        float fc = step(0.9, hash(vec2(cell.x, 8.8))) * (dc >= 0.0 ? exp(-dc / (2.0 * featureUnit)) : 0.0);
        return max(fr, fc);
    }
    if (mode == INTERFERENCE) {
        vec2 s1 = resolution * vec2(0.3 + 0.1 * sin(t * 0.13), 0.4 + 0.1 * cos(t * 0.11));
        vec2 s2 = resolution * vec2(0.7 + 0.1 * cos(t * 0.09), 0.6 + 0.1 * sin(t * 0.15));
        float k = 45.0 * featureUnit;
        float w = 0.5 * (sin(length(centre - s1) / k - t) + sin(length(centre - s2) / k - t));
        return pow(0.5 + 0.5 * w, 4.0);
    }
    if (mode == CURSOR) {
        float k = length(centre - pointer) / (160.0 * featureUnit);
        return exp(-k * k);
    }
    return 0.0;
}

void main() {
    vec2 px = qt_TexCoord0 * resolution;
    vec2 cell = floor((px + 0.5 * spacing) / spacing);
    vec2 fromNearestDot = mod(px + 0.5 * spacing, spacing) - 0.5 * spacing;
    float peak = motion(cell, cell * spacing);

    float r = min(radius * mix(1.0, sizeRange, peak), MAX_RADIUS_SPACING * spacing);
    float fadeWidth = max(r * (FADE_END_RATIO - 1.0), MIN_FADE_PX);
    float coverage = 1.0 - clamp((length(fromNearestDot) - r) / fadeWidth, 0.0, 1.0);
    float gain = min(1.0 + brightness * peak, MAX_GAIN);

    // ShaderEffect passes colour properties already premultiplied, which is also what the scene graph
    // blends, so scaling all four channels applies coverage and opacity without a second alpha.
    fragColor = dotColor * (coverage * gain * qt_Opacity);
}
