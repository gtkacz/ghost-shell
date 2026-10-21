#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float level;
};

layout(binding = 1) uniform sampler2D source;

void main() {
    // The layer texture is premultiplied, as the scene graph blends, so scaling all four channels fades it.
    fragColor = texture(source, qt_TexCoord0) * (level * qt_Opacity);
}
