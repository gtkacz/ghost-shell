import QtQuick
import Quickshell
import "lib/mirrorgraph.mjs" as Packing
import "lib/segments.mjs" as Segments

// Two histories on one axis, the first above it and the second below, such as upload and download: one column of
// cells per sample, the newest on the right, each half lit out from the axis on a square-root scale so idle traffic
// stays visible next to bursts. One shader draws every cell, as in BlockGraph, and a new sample repacks the uniforms
// in JavaScript and renders one frame.
ShaderEffect {
    id: root

    property var up: []
    property var down: []
    // The smallest the scale may be, in the samples' own unit, so near-idle traffic does not fill the graph.
    property real floor: 0
    // Design pixels.
    property real cellW: 4
    property real cellH: 2
    property real gap: 1
    // Whether the samples' own source is live, such as Data.topic(name).live.
    property bool sourceLive: true

    readonly property bool live: Data.connected && !Data.stale && sourceLive
    readonly property real gapSize: Theme.d(gap)
    readonly property real cellHeight: Theme.d(cellH)
    readonly property real columnWidth: Theme.d(cellW)
    // Whole numbers, but real: ShaderEffect copies an int property's bits into the shader's float as they are, which
    // reads as almost zero and draws nothing (measured on BlockGraph).
    readonly property real columnCount: Segments.cellCount(width, columnWidth, gapSize)
    readonly property real fittedRows: Packing.fittedHalfRows(height, cellHeight, gapSize)
    // The rows drawn on each side: the ones nearest the axis of a graph taller than the packing holds, with the values
    // scaled to them.
    readonly property real halfRowCount: Packing.drawnHalfRows(fittedRows)
    readonly property real axisY: Math.floor(height / 2)
    // Leftover width goes on the left, so the newest sample always sits on the right edge.
    readonly property real firstColumnX: width - columnCount * (columnWidth + gapSize) + gapSize
    readonly property real skippedColumns: Math.max(0, columnCount - Packing.MAX_COLUMNS)
    readonly property size itemSize: Qt.size(width, height)
    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1

    // A cell past the lit ones, so segments.mjs stays the one place that defines how unlit cells look.
    readonly property color unlitColor: Segments.mirrorCellColor(true, 0, 0, 1, Theme)
    // The ramps' alpha travels in the row words, so the shader restates no colour or alpha either.
    readonly property color upColor: Qt.rgba(Theme.hi.r, Theme.hi.g, Theme.hi.b, 1)
    readonly property color downColor: Qt.rgba(Theme.signal.r, Theme.signal.g, Theme.signal.b, 1)

    readonly property real scaleMax: Packing.mirrorMax(up, down, floor)
    readonly property var columnWords: Packing.mirrorColumnWords(up, down, columnCount, fittedRows, {
        max: scaleMax,
        live
    })
    readonly property var rowWords: Packing.mirrorRowWords(fittedRows)

    property bool warnedColumns: false
    property bool warnedRows: false

    function word(words, index) {
        const components = words[index];
        return Qt.vector4d(components[0], components[1], components[2], components[3]);
    }

    // Once each, since a resize can cross a limit many times.
    onColumnCountChanged: {
        if (columnCount > Packing.MAX_COLUMNS && !warnedColumns) {
            warnedColumns = true;
            console.warn("MirrorGraph draws at most " + Packing.MAX_COLUMNS + " columns, got " + columnCount + "; the oldest stay unlit");
        }
    }
    onFittedRowsChanged: {
        if (fittedRows > Packing.MAX_HALF_ROWS && !warnedRows) {
            warnedRows = true;
            console.warn("MirrorGraph draws only the " + Packing.MAX_HALF_ROWS + " rows nearest the axis of halves that fit " + fittedRows + "; the height beyond stays empty");
        }
    }

    // ShaderEffect maps properties to uniforms by name and ignores arrays, hence one property per word.
    readonly property vector4d columnWord0: word(columnWords, 0)
    readonly property vector4d columnWord1: word(columnWords, 1)
    readonly property vector4d columnWord2: word(columnWords, 2)
    readonly property vector4d columnWord3: word(columnWords, 3)
    readonly property vector4d columnWord4: word(columnWords, 4)
    readonly property vector4d columnWord5: word(columnWords, 5)
    readonly property vector4d columnWord6: word(columnWords, 6)
    readonly property vector4d columnWord7: word(columnWords, 7)
    readonly property vector4d columnWord8: word(columnWords, 8)
    readonly property vector4d columnWord9: word(columnWords, 9)
    readonly property vector4d columnWord10: word(columnWords, 10)
    readonly property vector4d columnWord11: word(columnWords, 11)
    readonly property vector4d columnWord12: word(columnWords, 12)
    readonly property vector4d columnWord13: word(columnWords, 13)
    readonly property vector4d columnWord14: word(columnWords, 14)
    readonly property vector4d columnWord15: word(columnWords, 15)
    readonly property vector4d columnWord16: word(columnWords, 16)
    readonly property vector4d columnWord17: word(columnWords, 17)
    readonly property vector4d columnWord18: word(columnWords, 18)
    readonly property vector4d columnWord19: word(columnWords, 19)
    readonly property vector4d columnWord20: word(columnWords, 20)
    readonly property vector4d columnWord21: word(columnWords, 21)
    readonly property vector4d columnWord22: word(columnWords, 22)
    readonly property vector4d columnWord23: word(columnWords, 23)
    readonly property vector4d columnWord24: word(columnWords, 24)
    readonly property vector4d columnWord25: word(columnWords, 25)
    readonly property vector4d columnWord26: word(columnWords, 26)
    readonly property vector4d columnWord27: word(columnWords, 27)
    readonly property vector4d columnWord28: word(columnWords, 28)
    readonly property vector4d columnWord29: word(columnWords, 29)
    readonly property vector4d columnWord30: word(columnWords, 30)
    readonly property vector4d columnWord31: word(columnWords, 31)
    readonly property vector4d columnWord32: word(columnWords, 32)
    readonly property vector4d columnWord33: word(columnWords, 33)
    readonly property vector4d columnWord34: word(columnWords, 34)
    readonly property vector4d columnWord35: word(columnWords, 35)
    readonly property vector4d columnWord36: word(columnWords, 36)
    readonly property vector4d columnWord37: word(columnWords, 37)
    readonly property vector4d columnWord38: word(columnWords, 38)
    readonly property vector4d columnWord39: word(columnWords, 39)
    readonly property vector4d columnWord40: word(columnWords, 40)
    readonly property vector4d columnWord41: word(columnWords, 41)
    readonly property vector4d columnWord42: word(columnWords, 42)
    readonly property vector4d columnWord43: word(columnWords, 43)
    readonly property vector4d columnWord44: word(columnWords, 44)
    readonly property vector4d columnWord45: word(columnWords, 45)
    readonly property vector4d columnWord46: word(columnWords, 46)
    readonly property vector4d columnWord47: word(columnWords, 47)
    readonly property vector4d rowWord0: word(rowWords, 0)
    readonly property vector4d rowWord1: word(rowWords, 1)
    readonly property vector4d rowWord2: word(rowWords, 2)
    readonly property vector4d rowWord3: word(rowWords, 3)
    readonly property vector4d rowWord4: word(rowWords, 4)
    readonly property vector4d rowWord5: word(rowWords, 5)
    readonly property vector4d rowWord6: word(rowWords, 6)
    readonly property vector4d rowWord7: word(rowWords, 7)
    readonly property vector4d rowWord8: word(rowWords, 8)
    readonly property vector4d rowWord9: word(rowWords, 9)
    readonly property vector4d rowWord10: word(rowWords, 10)

    fragmentShader: Qt.resolvedUrl("shaders/mirrorgraph.frag.qsb")
}
