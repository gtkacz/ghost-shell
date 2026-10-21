import QtQuick
import Quickshell
import "lib/segments.mjs" as Segments

// Columns of stacked cells, one column per level, stretched across the width and lit from the bottom like a
// BlockGraph at the ok level. One shader draws every cell: the spectrum redraws at 30 frames/s, and a Rectangle per
// cell would be some 300 items for the scene graph to update each frame.
ShaderEffect {
    id: root

    // The shader holds four levels in each of its eight vec4 uniforms.
    readonly property int maxLevels: 32
    readonly property int levelsPerVector: 4

    // Fractions, 0..1, lowest column first. A level that is not a finite number lights nothing.
    property list<real> levels: []
    // Design pixels.
    property real cellH: 3
    property real gap: 2

    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1
    // Whole physical pixels, so every cell is the same height and every gap the same size.
    readonly property real cellHeight: Math.max(1, Math.round(Theme.d(cellH) * pixelRatio))
    readonly property real rowGap: Math.round(Theme.d(gap) * pixelRatio)
    readonly property real columnGap: rowGap
    readonly property real rowStep: cellHeight + rowGap
    // A whole number, but real: ShaderEffect copies an int property's bits into the shader's float as they are, which
    // reads as almost zero and draws nothing (measured).
    readonly property real rows: Segments.cellCount(height * pixelRatio, cellHeight, rowGap)
    readonly property real columns: Math.min(levels.length, maxLevels)
    readonly property real columnStep: columns > 0 ? (width * pixelRatio + columnGap) / columns : 0
    readonly property real columnWidth: columnStep - columnGap

    readonly property color unlitColor: Segments.cellColor("ok", 0, 0, 1, Theme)
    readonly property color signalColor: Theme.signal
    readonly property color hiColor: Theme.hi
    readonly property real litAlphaFloor: Segments.LIT_ALPHA_FLOOR_PCT / Theme.percent
    readonly property real litAlphaSpan: Segments.LIT_ALPHA_SPAN_PCT / Theme.percent
    readonly property size resolution: Qt.size(width * pixelRatio, height * pixelRatio)

    // The uniforms, worked out from one read of the levels: each read of a list<real> from JavaScript reads the
    // property again, and the levels change every frame.
    readonly property var packed: pack(Array.from(levels))
    readonly property vector4d levels0: packed[0]
    readonly property vector4d levels1: packed[1]
    readonly property vector4d levels2: packed[2]
    readonly property vector4d levels3: packed[3]
    readonly property vector4d levels4: packed[4]
    readonly property vector4d levels5: packed[5]
    readonly property vector4d levels6: packed[6]
    readonly property vector4d levels7: packed[7]

    property bool warnedTooMany: false

    function pack(values) {
        const at = column => Number.isFinite(values[column]) ? values[column] : 0;
        const vectors = [];
        for (let first = 0; first < maxLevels; first += levelsPerVector) {
            vectors.push(Qt.vector4d(at(first), at(first + 1), at(first + 2), at(first + 3)));
        }
        return vectors;
    }

    // Once, since the levels may change every frame.
    onLevelsChanged: {
        if (levels.length > maxLevels && !warnedTooMany) {
            warnedTooMany = true;
            console.warn("SegBars draws at most " + maxLevels + " levels, got " + levels.length);
        }
    }

    fragmentShader: Qt.resolvedUrl("shaders/segbars.frag.qsb")
}
