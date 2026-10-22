#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

// A band of HeatGraph's rows. Lengths are logical pixels, as HeatGraph lays out its cells; `itemSize` is the item's
// size. ShaderEffect hands colour properties over premultiplied, which is also what fragColor must be.
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 itemSize;
    float firstColumnX;
    float columnWidth;
    float gapSize;
    float rowHeight;
    float columnCount;
    float rowCount;
    float drawnColumns;
    float skippedColumns;
    float head;
    float pixelRatio;
    vec4 unlitColor;
    vec4 rampColor;
    vec4 warnColor;
    vec4 critColor;
    // ShaderEffect ignores array uniforms, so the cells travel in named words, the first byte of each component in its
    // lowest bits: a byte per cell, three to a component, each column's rows together, the columns in a ring whose
    // slot `head` holds the oldest column drawn. With the block's other members that is about 145 vec4 uniforms,
    // inside the 256 that OpenGL 3.3 promises a fragment shader (1024 on NVIDIA's driver here, 4096 on Mesa).
    vec4 cellWord0; vec4 cellWord1; vec4 cellWord2; vec4 cellWord3;
    vec4 cellWord4; vec4 cellWord5; vec4 cellWord6; vec4 cellWord7;
    vec4 cellWord8; vec4 cellWord9; vec4 cellWord10; vec4 cellWord11;
    vec4 cellWord12; vec4 cellWord13; vec4 cellWord14; vec4 cellWord15;
    vec4 cellWord16; vec4 cellWord17; vec4 cellWord18; vec4 cellWord19;
    vec4 cellWord20; vec4 cellWord21; vec4 cellWord22; vec4 cellWord23;
    vec4 cellWord24; vec4 cellWord25; vec4 cellWord26; vec4 cellWord27;
    vec4 cellWord28; vec4 cellWord29; vec4 cellWord30; vec4 cellWord31;
    vec4 cellWord32; vec4 cellWord33; vec4 cellWord34; vec4 cellWord35;
    vec4 cellWord36; vec4 cellWord37; vec4 cellWord38; vec4 cellWord39;
    vec4 cellWord40; vec4 cellWord41; vec4 cellWord42; vec4 cellWord43;
    vec4 cellWord44; vec4 cellWord45; vec4 cellWord46; vec4 cellWord47;
    vec4 cellWord48; vec4 cellWord49; vec4 cellWord50; vec4 cellWord51;
    vec4 cellWord52; vec4 cellWord53; vec4 cellWord54; vec4 cellWord55;
    vec4 cellWord56; vec4 cellWord57; vec4 cellWord58; vec4 cellWord59;
    vec4 cellWord60; vec4 cellWord61; vec4 cellWord62; vec4 cellWord63;
    vec4 cellWord64; vec4 cellWord65; vec4 cellWord66; vec4 cellWord67;
    vec4 cellWord68; vec4 cellWord69; vec4 cellWord70; vec4 cellWord71;
    vec4 cellWord72; vec4 cellWord73; vec4 cellWord74; vec4 cellWord75;
    vec4 cellWord76; vec4 cellWord77; vec4 cellWord78; vec4 cellWord79;
    vec4 cellWord80; vec4 cellWord81; vec4 cellWord82; vec4 cellWord83;
    vec4 cellWord84; vec4 cellWord85; vec4 cellWord86; vec4 cellWord87;
    vec4 cellWord88; vec4 cellWord89; vec4 cellWord90; vec4 cellWord91;
    vec4 cellWord92; vec4 cellWord93; vec4 cellWord94; vec4 cellWord95;
    vec4 cellWord96; vec4 cellWord97; vec4 cellWord98; vec4 cellWord99;
    vec4 cellWord100; vec4 cellWord101; vec4 cellWord102; vec4 cellWord103;
    vec4 cellWord104; vec4 cellWord105; vec4 cellWord106; vec4 cellWord107;
    vec4 cellWord108; vec4 cellWord109; vec4 cellWord110; vec4 cellWord111;
    vec4 cellWord112; vec4 cellWord113; vec4 cellWord114; vec4 cellWord115;
    vec4 cellWord116; vec4 cellWord117; vec4 cellWord118; vec4 cellWord119;
    vec4 cellWord120; vec4 cellWord121; vec4 cellWord122; vec4 cellWord123;
    vec4 cellWord124; vec4 cellWord125; vec4 cellWord126; vec4 cellWord127;

};

// As lib/heatgraph.mjs packs them.
const float COMPONENTS_PER_WORD = 4.0;
const float CELLS_PER_COMPONENT = 3.0;
const float CELL_RANGE = 256.0;
const float BYTE_MAX = 255.0;
const float CELL_MISSING = 0.0;
const float CELL_WARN = 254.0;
const float CELL_CRIT = 255.0;
// Half a step of the rasteriser's sub-pixel grid, in physical pixels: the grid is 1/256 with the 8 sub-pixel bits
// that desktop GPUs have.
const float SNAP_TOLERANCE = 1.0 / 512.0;

// The words are separate members, so one is picked by comparison: halving down to eight words and then one at a time,
// at most four halvings and seven comparisons per fragment. GLSL ES 100, which qsb also bakes, does not promise
// indexing a vector's components with a variable either.
vec4 cellWordAt(int i) {
    if (i < 64) {
        if (i < 32) {
            if (i < 16) {
                if (i < 8) {
                    if (i == 0) { return cellWord0; } if (i == 1) { return cellWord1; }
                    if (i == 2) { return cellWord2; } if (i == 3) { return cellWord3; }
                    if (i == 4) { return cellWord4; } if (i == 5) { return cellWord5; }
                    if (i == 6) { return cellWord6; } return cellWord7;
                }
                if (i == 8) { return cellWord8; } if (i == 9) { return cellWord9; }
                if (i == 10) { return cellWord10; } if (i == 11) { return cellWord11; }
                if (i == 12) { return cellWord12; } if (i == 13) { return cellWord13; }
                if (i == 14) { return cellWord14; } return cellWord15;
            }
            if (i < 24) {
                if (i == 16) { return cellWord16; } if (i == 17) { return cellWord17; }
                if (i == 18) { return cellWord18; } if (i == 19) { return cellWord19; }
                if (i == 20) { return cellWord20; } if (i == 21) { return cellWord21; }
                if (i == 22) { return cellWord22; } return cellWord23;
            }
            if (i == 24) { return cellWord24; } if (i == 25) { return cellWord25; }
            if (i == 26) { return cellWord26; } if (i == 27) { return cellWord27; }
            if (i == 28) { return cellWord28; } if (i == 29) { return cellWord29; }
            if (i == 30) { return cellWord30; } return cellWord31;
        }
        if (i < 48) {
            if (i < 40) {
                if (i == 32) { return cellWord32; } if (i == 33) { return cellWord33; }
                if (i == 34) { return cellWord34; } if (i == 35) { return cellWord35; }
                if (i == 36) { return cellWord36; } if (i == 37) { return cellWord37; }
                if (i == 38) { return cellWord38; } return cellWord39;
            }
            if (i == 40) { return cellWord40; } if (i == 41) { return cellWord41; }
            if (i == 42) { return cellWord42; } if (i == 43) { return cellWord43; }
            if (i == 44) { return cellWord44; } if (i == 45) { return cellWord45; }
            if (i == 46) { return cellWord46; } return cellWord47;
        }
        if (i < 56) {
            if (i == 48) { return cellWord48; } if (i == 49) { return cellWord49; }
            if (i == 50) { return cellWord50; } if (i == 51) { return cellWord51; }
            if (i == 52) { return cellWord52; } if (i == 53) { return cellWord53; }
            if (i == 54) { return cellWord54; } return cellWord55;
        }
        if (i == 56) { return cellWord56; } if (i == 57) { return cellWord57; }
        if (i == 58) { return cellWord58; } if (i == 59) { return cellWord59; }
        if (i == 60) { return cellWord60; } if (i == 61) { return cellWord61; }
        if (i == 62) { return cellWord62; } return cellWord63;
    }
    if (i < 96) {
        if (i < 80) {
            if (i < 72) {
                if (i == 64) { return cellWord64; } if (i == 65) { return cellWord65; }
                if (i == 66) { return cellWord66; } if (i == 67) { return cellWord67; }
                if (i == 68) { return cellWord68; } if (i == 69) { return cellWord69; }
                if (i == 70) { return cellWord70; } return cellWord71;
            }
            if (i == 72) { return cellWord72; } if (i == 73) { return cellWord73; }
            if (i == 74) { return cellWord74; } if (i == 75) { return cellWord75; }
            if (i == 76) { return cellWord76; } if (i == 77) { return cellWord77; }
            if (i == 78) { return cellWord78; } return cellWord79;
        }
        if (i < 88) {
            if (i == 80) { return cellWord80; } if (i == 81) { return cellWord81; }
            if (i == 82) { return cellWord82; } if (i == 83) { return cellWord83; }
            if (i == 84) { return cellWord84; } if (i == 85) { return cellWord85; }
            if (i == 86) { return cellWord86; } return cellWord87;
        }
        if (i == 88) { return cellWord88; } if (i == 89) { return cellWord89; }
        if (i == 90) { return cellWord90; } if (i == 91) { return cellWord91; }
        if (i == 92) { return cellWord92; } if (i == 93) { return cellWord93; }
        if (i == 94) { return cellWord94; } return cellWord95;
    }
    if (i < 112) {
        if (i < 104) {
            if (i == 96) { return cellWord96; } if (i == 97) { return cellWord97; }
            if (i == 98) { return cellWord98; } if (i == 99) { return cellWord99; }
            if (i == 100) { return cellWord100; } if (i == 101) { return cellWord101; }
            if (i == 102) { return cellWord102; } return cellWord103;
        }
        if (i == 104) { return cellWord104; } if (i == 105) { return cellWord105; }
        if (i == 106) { return cellWord106; } if (i == 107) { return cellWord107; }
        if (i == 108) { return cellWord108; } if (i == 109) { return cellWord109; }
        if (i == 110) { return cellWord110; } return cellWord111;
    }
    if (i < 120) {
        if (i == 112) { return cellWord112; } if (i == 113) { return cellWord113; }
        if (i == 114) { return cellWord114; } if (i == 115) { return cellWord115; }
        if (i == 116) { return cellWord116; } if (i == 117) { return cellWord117; }
        if (i == 118) { return cellWord118; } return cellWord119;
    }
    if (i == 120) { return cellWord120; } if (i == 121) { return cellWord121; }
    if (i == 122) { return cellWord122; } if (i == 123) { return cellWord123; }
    if (i == 124) { return cellWord124; } if (i == 125) { return cellWord125; }
    if (i == 126) { return cellWord126; } return cellWord127;

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

// Each component holds an integer below 2^24, which a 32-bit float holds exactly, and the range is a power of two,
// so dividing by it is exact and the bytes come back unchanged. The ring, word and component indices divide by the
// drawn columns, 12 or 3, which GLSL may get a few ULP wrong, so they divide the index plus a half: the quotient then
// stays at least 1/768 from a whole number, and no such error can move its floor.
float cellByte(float column, float row) {
    float ringAt = head + column;
    float slot = ringAt - drawnColumns * floor((ringAt + 0.5) / drawnColumns);
    float index = slot * rowCount + row;
    float perWord = CELLS_PER_COMPONENT * COMPONENTS_PER_WORD;
    float word = floor((index + 0.5) / perWord);
    float inWord = index - word * perWord;
    float slotInWord = floor((inWord + 0.5) / CELLS_PER_COMPONENT);
    float shift = inWord - slotInWord * CELLS_PER_COMPONENT;
    float divisor = shift == 0.0 ? 1.0 : (shift == 1.0 ? CELL_RANGE : CELL_RANGE * CELL_RANGE);
    return mod(floor(component(cellWordAt(int(word)), int(slotInWord)) / divisor), CELL_RANGE);
}

// segments.mjs's heatCellColor(), from the byte lib/heatgraph.mjs gives the cell.
vec4 cellColor(float cell) {
    if (cell == CELL_MISSING) {
        return unlitColor;
    }
    if (cell == CELL_CRIT) {
        return critColor;
    }
    if (cell == CELL_WARN) {
        return warnColor;
    }
    float alpha = cell / BYTE_MAX;
    return vec4(rampColor.rgb * alpha, alpha);
}

void main() {
    // blockgraph.frag's edge rule, measured there: looking this far right of and above each pixel centre puts an edge
    // that lands on the centre where a Rectangle's would be. It assumes OpenGL's y-up framebuffer, which is the only
    // backend it was measured on.
    vec2 p = qt_TexCoord0 * itemSize + vec2(SNAP_TOLERANCE, -SNAP_TOLERANCE) / pixelRatio;
    float pitchX = columnWidth + gapSize;
    float pitchY = rowHeight + gapSize;
    float fromLeft = p.x - firstColumnX;
    float column = floor(fromLeft / pitchX);
    // Rows count down from the top, the first row on the top edge.
    float row = floor(p.y / pitchY);
    bool inCell = fromLeft >= 0.0 && column < columnCount && fromLeft - column * pitchX < columnWidth
        && p.y >= 0.0 && row < rowCount && p.y - row * pitchY < rowHeight;
    if (!inCell) {
        fragColor = vec4(0.0);
        return;
    }
    float cell = column < skippedColumns ? CELL_MISSING : cellByte(column - skippedColumns, row);
    fragColor = cellColor(cell) * qt_Opacity;
}
