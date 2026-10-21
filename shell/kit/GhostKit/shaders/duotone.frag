#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec4 shadowColor;
    vec4 lightColor;
};

layout(binding = 1) uniform sampler2D source;

// Rec. 709 luma weights.
const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

void main() {
    vec4 texel = texture(source, qt_TexCoord0);
    // The texture is premultiplied, and the luminance is the colour's own, so its alpha is divided out first.
    float luminance = texel.a > 0.0 ? dot(texel.rgb / texel.a, LUMA) : 0.0;
    // Both tones arrive premultiplied and opaque, so scaling their mix by the texel's alpha keeps its transparency.
    fragColor = mix(shadowColor, lightColor, clamp(luminance, 0.0, 1.0)) * (texel.a * qt_Opacity);
}
