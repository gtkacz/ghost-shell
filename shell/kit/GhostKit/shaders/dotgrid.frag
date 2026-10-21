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
};

// The reference mockup's radial gradient is solid out to the radius and fades out by 1.4 times it.
const float FADE_END_RATIO = 1.4;

void main() {
    vec2 fromNearestDot = mod(qt_TexCoord0 * resolution + 0.5 * spacing, spacing) - 0.5 * spacing;
    float fadeWidth = radius * (FADE_END_RATIO - 1.0);
    float coverage = 1.0 - clamp((length(fromNearestDot) - radius) / fadeWidth, 0.0, 1.0);

    // ShaderEffect passes colour properties already premultiplied, which is also what the scene graph
    // blends, so scaling all four channels applies coverage and opacity without a second alpha.
    fragColor = dotColor * (coverage * qt_Opacity);
}
