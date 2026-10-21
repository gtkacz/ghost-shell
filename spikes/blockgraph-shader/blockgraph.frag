#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

// ShaderEffect hands colour properties over premultiplied, which is also what fragColor must be.
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    // Logical pixels, as BlockGraph lays out its Rectangles.
    vec2 itemSize;
    float firstColumnX;
    float columnWidth;
    float gapSize;
    float cellHeight;
    float columnCount;
    float rowCount;
    float skippedColumns;
    vec4 unlitColor;
    vec4 rampColor;
    vec4 hiColor;
    vec4 warnColor;
    vec4 critColor;
    // ShaderEffect ignores array uniforms, so the bytes travel in named words: three bytes per component,
    // the first in the lowest bits. A column's byte is its lit cell count times LEVELS plus its level; a
    // row's byte is the ramp's alpha there.
    vec4 c0; vec4 c1; vec4 c2; vec4 c3; vec4 c4; vec4 c5; vec4 c6; vec4 c7;
    vec4 c8; vec4 c9; vec4 c10; vec4 c11; vec4 c12; vec4 c13; vec4 c14; vec4 c15;
    vec4 c16; vec4 c17; vec4 c18; vec4 c19; vec4 c20; vec4 c21; vec4 c22; vec4 c23;
    vec4 c24; vec4 c25; vec4 c26; vec4 c27; vec4 c28; vec4 c29; vec4 c30; vec4 c31;
    vec4 r0; vec4 r1; vec4 r2; vec4 r3; vec4 r4; vec4 r5;
};

const float BYTE_MAX = 255.0;
const float BYTE_RANGE = 256.0;
const float BYTES_PER_COMPONENT = 3.0;
const float BYTES_PER_WORD = 12.0;
const float LEVELS = 4.0;
const int LEVEL_WARN = 1;
const int LEVEL_CRIT = 2;

// The words are separate members, so one is picked by comparison. GLSL ES 100, which qsb also bakes, does not
// promise indexing a vector's components with a variable either.
vec4 columnWord(int i) {
    if (i < 8) {
        if (i == 0) { return c0; } if (i == 1) { return c1; } if (i == 2) { return c2; } if (i == 3) { return c3; }
        if (i == 4) { return c4; } if (i == 5) { return c5; } if (i == 6) { return c6; } return c7;
    }
    if (i < 16) {
        if (i == 8) { return c8; } if (i == 9) { return c9; } if (i == 10) { return c10; } if (i == 11) { return c11; }
        if (i == 12) { return c12; } if (i == 13) { return c13; } if (i == 14) { return c14; } return c15;
    }
    if (i < 24) {
        if (i == 16) { return c16; } if (i == 17) { return c17; } if (i == 18) { return c18; } if (i == 19) { return c19; }
        if (i == 20) { return c20; } if (i == 21) { return c21; } if (i == 22) { return c22; } return c23;
    }
    if (i == 24) { return c24; } if (i == 25) { return c25; } if (i == 26) { return c26; } if (i == 27) { return c27; }
    if (i == 28) { return c28; } if (i == 29) { return c29; } if (i == 30) { return c30; } return c31;
}

vec4 rowWord(int i) {
    if (i == 0) { return r0; } if (i == 1) { return r1; } if (i == 2) { return r2; } if (i == 3) { return r3; }
    if (i == 4) { return r4; } return r5;
}

float component(vec4 word, int i) {
    if (i == 0) {
        return word.x;
    }
    if (i == 1) {
        return word.y;
    }
    if (i == 2) {
        return word.z;
    }
    return word.w;
}

// Each component holds an integer below 2^24, which a float holds exactly, and dividing by a power of two
// is exact, so the bytes come back unchanged.
float byteIn(vec4 word, float index) {
    float slot = floor(index / BYTES_PER_COMPONENT);
    float shift = index - slot * BYTES_PER_COMPONENT;
    float value = component(word, int(slot));
    float divisor = shift == 0.0 ? 1.0 : (shift == 1.0 ? BYTE_RANGE : BYTE_RANGE * BYTE_RANGE);
    return mod(floor(value / divisor), BYTE_RANGE);
}

float columnByte(float column) {
    float word = floor(column / BYTES_PER_WORD);
    return byteIn(columnWord(int(word)), column - word * BYTES_PER_WORD);
}

float rowByte(float row) {
    float word = floor(row / BYTES_PER_WORD);
    return byteIn(rowWord(int(word)), row - word * BYTES_PER_WORD);
}

vec4 litColor(int level, float row, int lit) {
    if (level == LEVEL_CRIT) {
        return critColor;
    }
    if (level == LEVEL_WARN) {
        return warnColor;
    }
    if (int(row) == lit - 1) {
        return hiColor;
    }
    float alpha = rowByte(row) / BYTE_MAX;
    return vec4(rampColor.rgb * alpha, alpha);
}

void main() {
    vec2 p = qt_TexCoord0 * itemSize;
    float pitchX = columnWidth + gapSize;
    float pitchY = cellHeight + gapSize;
    float fromLeft = p.x - firstColumnX;
    // Measured down from the top row's top edge, so a pixel centre on a cell's edge falls on the side the
    // rasteriser puts it for a Rectangle: top and left edges are inside, bottom and right ones are not.
    float fromTop = p.y - (itemSize.y - rowCount * pitchY + gapSize);
    float column = floor(fromLeft / pitchX);
    float rowFromTop = floor(fromTop / pitchY);
    bool inCell = fromLeft >= 0.0 && column < columnCount && fromLeft - column * pitchX < columnWidth
        && fromTop >= 0.0 && fromTop - rowFromTop * pitchY < cellHeight;
    if (!inCell) {
        fragColor = vec4(0.0);
        return;
    }
    // BlockGraph counts rows up from the bottom edge.
    float row = rowCount - 1.0 - rowFromTop;
    float packed = column < skippedColumns ? 0.0 : columnByte(column - skippedColumns);
    float litCells = floor(packed / LEVELS);
    int lit = int(litCells);
    int level = int(packed - litCells * LEVELS);
    vec4 c = int(row) < lit ? litColor(level, row, lit) : unlitColor;
    fragColor = c * qt_Opacity;
}
