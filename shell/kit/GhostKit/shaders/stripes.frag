#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

// Lengths are physical pixels; `resolution` is the item's size in physical pixels.
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec4 stripeColor;
    float tile;
    float phase;
    vec2 resolution;
};

void main() {
    // Whole pixels, so every stripe edge is the same staircase.
    vec2 pixel = floor(qt_TexCoord0 * resolution);
    // x + y is constant along the stripes' "/" diagonals; subtracting the phase moves them right.
    float alongTile = mod(pixel.x + pixel.y - phase, tile);
    // The tile is half stripe, half gap, as in the reference mockup's 25 % gradient stops.
    float onStripe = step(0.5 * tile, alongTile);

    // ShaderEffect passes colour properties already premultiplied, which is also what the scene graph
    // blends, so scaling all four channels applies coverage and opacity without a second alpha.
    fragColor = stripeColor * (onStripe * qt_Opacity);
}
