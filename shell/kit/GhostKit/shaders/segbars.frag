#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

// Lengths are physical pixels; `resolution` is the item's size in physical pixels. levelsN holds the levels of
// columns 4N to 4N + 3, as fractions; rows count from the bottom.
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec4 unlitColor;
    vec4 signalColor;
    vec4 hiColor;
    vec4 levels0;
    vec4 levels1;
    vec4 levels2;
    vec4 levels3;
    vec4 levels4;
    vec4 levels5;
    vec4 levels6;
    vec4 levels7;
    float columns;
    float rows;
    float columnStep;
    float columnWidth;
    float rowStep;
    float cellHeight;
    float litAlphaFloor;
    float litAlphaSpan;
    vec2 resolution;
};

const float LEVELS_PER_VECTOR = 4.0;

// The GLSL that Qt picks for a desktop OpenGL context may be older than 1.20, which has no array constructors or
// integer remainders, so the column's level is found with float arithmetic and selection instead.
float levelOf(float column) {
    float vector = floor(column / LEVELS_PER_VECTOR);
    float component = column - vector * LEVELS_PER_VECTOR;
    vec4 levels = vector < 1.0 ? levels0
        : vector < 2.0 ? levels1
        : vector < 3.0 ? levels2
        : vector < 4.0 ? levels3
        : vector < 5.0 ? levels4
        : vector < 6.0 ? levels5
        : vector < 7.0 ? levels6
        : levels7;
    return component < 1.0 ? levels.x : component < 2.0 ? levels.y : component < 3.0 ? levels.z : levels.w;
}

void main() {
    // Each pixel is in or out of a cell by its centre, so cell edges are whole pixels with no antialiasing.
    vec2 centre = floor(qt_TexCoord0 * resolution) + 0.5;
    float column = floor(centre.x / columnStep);
    float fromBottom = resolution.y - centre.y;
    float row = floor(fromBottom / rowStep);
    bool inCell = column < columns && row < rows
        && centre.x - column * columnStep < columnWidth
        && fromBottom - row * rowStep < cellHeight;
    if (!inCell) {
        fragColor = vec4(0.0);
        return;
    }

    // Rounded half up, as segments.mjs's litCells() rounds.
    float lit = floor(clamp(levelOf(column), 0.0, 1.0) * rows + 0.5);

    // segments.mjs's cellColor() at the ok level: unlit cells faint, the top lit cell hi, the rest of the column in
    // signal, stronger towards the top. ShaderEffect passes colours premultiplied, as the scene graph blends, so
    // scaling all four channels sets the strength.
    vec4 color;
    if (row >= lit) {
        color = unlitColor;
    } else if (row == lit - 1.0) {
        color = hiColor;
    } else {
        color = signalColor * (litAlphaFloor + litAlphaSpan * row / rows);
    }
    fragColor = color * qt_Opacity;
}
