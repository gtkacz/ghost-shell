#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

// Lengths are physical pixels; `resolution` is the item's size in physical pixels.
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float strength;
    float period;
    float lineWidth;
    vec2 resolution;
};

void main() {
    float y = qt_TexCoord0.y * resolution.y;
    float onLine = 1.0 - step(lineWidth, mod(y, period));

    // Premultiplied black is all zeros except alpha.
    fragColor = vec4(0.0, 0.0, 0.0, strength * onLine * qt_Opacity);
}
