#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

// Lengths are logical pixels, as MirrorGraph lays out its cells; `itemSize` is the item's size. ShaderEffect hands
// colour properties over premultiplied, which is also what fragColor must be.
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 itemSize;
    float firstColumnX;
    float columnWidth;
    float gapSize;
    float cellHeight;
    float axisY;
    float columnCount;
    float halfRowCount;
    float skippedColumns;
    float pixelRatio;
    vec4 unlitColor;
    vec4 upColor;
    vec4 downColor;
    // ShaderEffect ignores array uniforms, so the data travels in named words, the first field of each component in
    // its lowest bits. A column's field is its upper lit count times HALF_FIELD_RANGE plus its lower lit count, two to
    // a component, from the oldest column drawn; the row fields are the upper ramp's alpha bytes, then from field
    // MAX_HALF_ROWS the lower ramp's, three to a component, each from the axis out.
    vec4 columnWord0; vec4 columnWord1; vec4 columnWord2; vec4 columnWord3;
    vec4 columnWord4; vec4 columnWord5; vec4 columnWord6; vec4 columnWord7;
    vec4 columnWord8; vec4 columnWord9; vec4 columnWord10; vec4 columnWord11;
    vec4 columnWord12; vec4 columnWord13; vec4 columnWord14; vec4 columnWord15;
    vec4 columnWord16; vec4 columnWord17; vec4 columnWord18; vec4 columnWord19;
    vec4 columnWord20; vec4 columnWord21; vec4 columnWord22; vec4 columnWord23;
    vec4 columnWord24; vec4 columnWord25; vec4 columnWord26; vec4 columnWord27;
    vec4 columnWord28; vec4 columnWord29; vec4 columnWord30; vec4 columnWord31;
    vec4 columnWord32; vec4 columnWord33; vec4 columnWord34; vec4 columnWord35;
    vec4 columnWord36; vec4 columnWord37; vec4 columnWord38; vec4 columnWord39;
    vec4 columnWord40; vec4 columnWord41; vec4 columnWord42; vec4 columnWord43;
    vec4 columnWord44; vec4 columnWord45; vec4 columnWord46; vec4 columnWord47;
    vec4 rowWord0; vec4 rowWord1; vec4 rowWord2; vec4 rowWord3;
    vec4 rowWord4; vec4 rowWord5; vec4 rowWord6; vec4 rowWord7;
    vec4 rowWord8; vec4 rowWord9; vec4 rowWord10;
};

// As lib/mirrorgraph.mjs packs them.
const float COMPONENTS_PER_WORD = 4.0;
const float COLUMN_FIELDS_PER_COMPONENT = 2.0;
const float COLUMN_FIELD_RANGE = 4096.0;
const float HALF_FIELD_RANGE = 64.0;
const float MAX_HALF_ROWS = 63.0;
const float ROW_FIELDS_PER_COMPONENT = 3.0;
const float ROW_FIELD_RANGE = 256.0;
const float BYTE_MAX = 255.0;
// Half a step of the rasteriser's sub-pixel grid, in physical pixels: the grid is 1/256 with the 8 sub-pixel bits
// that desktop GPUs have.
const float SNAP_TOLERANCE = 1.0 / 512.0;

// The words are separate members, so one is picked by comparison. GLSL ES 100, which qsb also bakes, does not
// promise indexing a vector's components with a variable either.
vec4 columnWordAt(int i) {
    if (i < 16) {
        if (i < 8) {
            if (i == 0) { return columnWord0; } if (i == 1) { return columnWord1; }
            if (i == 2) { return columnWord2; } if (i == 3) { return columnWord3; }
            if (i == 4) { return columnWord4; } if (i == 5) { return columnWord5; }
            if (i == 6) { return columnWord6; } return columnWord7;
        }
        if (i == 8) { return columnWord8; } if (i == 9) { return columnWord9; }
        if (i == 10) { return columnWord10; } if (i == 11) { return columnWord11; }
        if (i == 12) { return columnWord12; } if (i == 13) { return columnWord13; }
        if (i == 14) { return columnWord14; } return columnWord15;
    }
    if (i < 32) {
        if (i < 24) {
            if (i == 16) { return columnWord16; } if (i == 17) { return columnWord17; }
            if (i == 18) { return columnWord18; } if (i == 19) { return columnWord19; }
            if (i == 20) { return columnWord20; } if (i == 21) { return columnWord21; }
            if (i == 22) { return columnWord22; } return columnWord23;
        }
        if (i == 24) { return columnWord24; } if (i == 25) { return columnWord25; }
        if (i == 26) { return columnWord26; } if (i == 27) { return columnWord27; }
        if (i == 28) { return columnWord28; } if (i == 29) { return columnWord29; }
        if (i == 30) { return columnWord30; } return columnWord31;
    }
    if (i < 40) {
        if (i == 32) { return columnWord32; } if (i == 33) { return columnWord33; }
        if (i == 34) { return columnWord34; } if (i == 35) { return columnWord35; }
        if (i == 36) { return columnWord36; } if (i == 37) { return columnWord37; }
        if (i == 38) { return columnWord38; } return columnWord39;
    }
    if (i == 40) { return columnWord40; } if (i == 41) { return columnWord41; }
    if (i == 42) { return columnWord42; } if (i == 43) { return columnWord43; }
    if (i == 44) { return columnWord44; } if (i == 45) { return columnWord45; }
    if (i == 46) { return columnWord46; } return columnWord47;
}

vec4 rowWordAt(int i) {
    if (i == 0) { return rowWord0; } if (i == 1) { return rowWord1; } if (i == 2) { return rowWord2; }
    if (i == 3) { return rowWord3; } if (i == 4) { return rowWord4; } if (i == 5) { return rowWord5; }
    if (i == 6) { return rowWord6; } if (i == 7) { return rowWord7; } if (i == 8) { return rowWord8; }
    if (i == 9) { return rowWord9; } return rowWord10;
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

// Each component holds an integer below 2^24, which a 32-bit float holds exactly, and each range is a power of two,
// so dividing by it is exact and the fields come back unchanged. The slot and word indices divide by 2, 3, 8 or 12,
// which GLSL may get a few ULP wrong, so they divide the index plus a half: the quotient then stays at least 1/24 from
// a whole number, and no such error can move its floor.
float fieldIn(vec4 word, float index, float fieldsPerComponent, float range) {
    float slot = floor((index + 0.5) / fieldsPerComponent);
    float shift = index - slot * fieldsPerComponent;
    float divisor = shift == 0.0 ? 1.0 : (shift == 1.0 ? range : range * range);
    return mod(floor(component(word, int(slot)) / divisor), range);
}

float columnField(float column) {
    float perWord = COLUMN_FIELDS_PER_COMPONENT * COMPONENTS_PER_WORD;
    float word = floor((column + 0.5) / perWord);
    return fieldIn(columnWordAt(int(word)), column - word * perWord, COLUMN_FIELDS_PER_COMPONENT, COLUMN_FIELD_RANGE);
}

float rowAlpha(float field) {
    float perWord = ROW_FIELDS_PER_COMPONENT * COMPONENTS_PER_WORD;
    float word = floor((field + 0.5) / perWord);
    return fieldIn(rowWordAt(int(word)), field - word * perWord, ROW_FIELDS_PER_COMPONENT, ROW_FIELD_RANGE) / BYTE_MAX;
}

void main() {
    // blockgraph.frag's edge rule, measured there: looking this far right of and above each pixel centre puts an edge
    // that lands on the centre where a Rectangle's would be. It assumes OpenGL's y-up framebuffer, which is the only
    // backend it was measured on.
    vec2 p = qt_TexCoord0 * itemSize + vec2(SNAP_TOLERANCE, -SNAP_TOLERANCE) / pixelRatio;
    float pitchX = columnWidth + gapSize;
    float pitchY = cellHeight + gapSize;
    float fromLeft = p.x - firstColumnX;
    float column = floor(fromLeft / pitchX);
    // Each half measures down from the top edge of its own top row: the upper half's outermost row, the lower half's
    // first. A gap separates each half from the axis.
    bool upper = p.y < axisY;
    float fromTop = upper ? p.y - (axisY - halfRowCount * pitchY) : p.y - (axisY + gapSize);
    float rowFromTop = floor(fromTop / pitchY);
    bool inCell = fromLeft >= 0.0 && column < columnCount && fromLeft - column * pitchX < columnWidth
        && fromTop >= 0.0 && rowFromTop < halfRowCount && fromTop - rowFromTop * pitchY < cellHeight;
    if (!inCell) {
        fragColor = vec4(0.0);
        return;
    }
    // Rows count out from the axis, as segments.mjs counts them.
    float row = upper ? halfRowCount - 1.0 - rowFromTop : rowFromTop;
    float packed = column < skippedColumns ? 0.0 : columnField(column - skippedColumns);
    float upLit = floor(packed / HALF_FIELD_RANGE);
    float lit = upper ? upLit : packed - upLit * HALF_FIELD_RANGE;
    vec4 color = unlitColor;
    if (row < lit) {
        float alpha = rowAlpha(upper ? row : MAX_HALF_ROWS + row);
        color = vec4((upper ? upColor.rgb : downColor.rgb) * alpha, alpha);
    }
    fragColor = color * qt_Opacity;
}
