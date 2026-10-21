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
    float imageWidth;
    vec4 unlitColor;
    vec4 rampColor;
    vec4 hiColor;
    vec4 warnColor;
    vec4 critColor;
};

// Sampled without filtering. Row 0 has a texel per column: red is the lit cell count, green the level (0 ok,
// 1 warn, 2 crit). Row 1 has a texel per graph row: red is the ramp's alpha there. All are bytes.
layout(binding = 1) uniform sampler2D columns;

const float BYTE_MAX = 255.0;
const float TEXEL_CENTRE = 0.5;
const float IMAGE_ROWS = 2.0;
const float LIT_ROW = 0.0;
const float RAMP_ROW = 1.0;
const int LEVEL_WARN = 1;
const int LEVEL_CRIT = 2;

// qsb also bakes GLSL ES 100, which has neither round() nor texelFetch().
float toByte(float unit) {
    return floor(unit * BYTE_MAX + 0.5);
}

vec4 texel(float x, float y) {
    return texture(columns, vec2((x + TEXEL_CENTRE) / imageWidth, (y + TEXEL_CENTRE) / IMAGE_ROWS));
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
    float alpha = texel(row, RAMP_ROW).r;
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
    vec4 packed = texel(column, LIT_ROW);
    int lit = int(toByte(packed.r));
    int level = int(toByte(packed.g));
    vec4 c = int(row) < lit ? litColor(level, row, lit) : unlitColor;
    fragColor = c * qt_Opacity;
}
