#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float spacing;
    vec2 resolution;
};

const float DOT_ALPHA = 0.16;
const vec3 DOT_COLOR = vec3(0.72, 0.82, 0.80);

void main() {
    vec2 offset = mod(qt_TexCoord0 * resolution, spacing);
    float isDot = step(offset.x, 1.0) * step(offset.y, 1.0);

    // The scene graph blends premultiplied alpha, so the colour has to be scaled by the alpha here.
    float alpha = DOT_ALPHA * isDot * qt_Opacity;
    fragColor = vec4(DOT_COLOR * alpha, alpha);
}
