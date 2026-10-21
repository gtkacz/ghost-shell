import QtQuick
import Quickshell
import "lib/blockgraph.mjs" as Packing
import "lib/segments.mjs" as Segments

// A history graph drawn as an LED matrix: one column of cells per sample, the newest on the right. One shader draws
// every cell: with a Rectangle per cell, six full-width graphs cost 4 % of a core and 190 MiB and took 185 ms to
// create, against 0.3 %, 6 MiB and 5 ms (docs/superpowers/notes/2026-10-21-blockgraph-shader-spike.md). The
// per-column work stays in JavaScript and reaches the shader packed into uniforms, so an update is one uniform change
// and renders one frame.
ShaderEffect {
    id: root

    property var values: []
    property real lo: 0
    property real hi: 100
    // 0 fits as many cellW-wide columns as the width allows; any other count stretches the columns to fill it.
    property int cols: 0
    // Design pixels.
    property real cellW: 3
    property real cellH: 2
    property real gap: 1
    // Fractions of lo..hi.
    property real warnAt: Theme.loadWarn
    property real critAt: Theme.loadCrit
    property int minLit: 1
    // Whether the samples' own source is live, such as Data.topic(name).live.
    property bool sourceLive: true

    readonly property bool live: Data.connected && !Data.stale && sourceLive
    readonly property real gapSize: Theme.d(gap)
    readonly property real cellHeight: Theme.d(cellH)
    // Whole numbers, but real: ShaderEffect copies an int property's bits into the shader's float as they are, which
    // reads as almost zero and draws nothing (measured).
    readonly property real columnCount: cols > 0 ? cols : Segments.cellCount(width, Theme.d(cellW), gapSize)
    readonly property real columnWidth: cols > 0 ? (width - (cols - 1) * gapSize) / cols : Theme.d(cellW)
    readonly property real rowCount: Segments.cellCount(height, cellHeight, gapSize)
    // Leftover width goes on the left, so the newest sample always sits on the right edge.
    readonly property real firstColumnX: width - columnCount * (columnWidth + gapSize) + gapSize
    readonly property real skippedColumns: Packing.skippedColumns(columnCount)
    readonly property size itemSize: Qt.size(width, height)
    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1

    // A cell above the lit ones, so segments.mjs stays the one place that defines how unlit cells look.
    readonly property color unlitColor: Segments.cellColor("ok", 0, 0, 1, Theme)
    // The ramp's alpha travels in the row words, so the shader restates no colour or alpha either.
    readonly property color rampColor: Qt.rgba(Theme.signal.r, Theme.signal.g, Theme.signal.b, 1)
    readonly property color hiColor: Theme.hi
    readonly property color warnColor: Theme.warn
    readonly property color critColor: Theme.crit

    readonly property var columnWords: Packing.columnWords(values, columnCount, rowCount, {
        lo,
        hi,
        warnAt,
        critAt,
        minLit,
        live
    })
    readonly property var rowWords: Packing.rowWords(rowCount)

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
            console.warn("BlockGraph draws at most " + Packing.MAX_COLUMNS + " columns, got " + columnCount + "; the oldest stay unlit");
        }
    }
    onRowCountChanged: {
        if (rowCount > Packing.MAX_ROWS && !warnedRows) {
            warnedRows = true;
            console.warn("BlockGraph draws at most " + Packing.MAX_ROWS + " rows, got " + rowCount + "; the top ones stay unlit");
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
    readonly property vector4d rowWord11: word(rowWords, 11)
    readonly property vector4d rowWord12: word(rowWords, 12)
    readonly property vector4d rowWord13: word(rowWords, 13)
    readonly property vector4d rowWord14: word(rowWords, 14)
    readonly property vector4d rowWord15: word(rowWords, 15)
    readonly property vector4d rowWord16: word(rowWords, 16)
    readonly property vector4d rowWord17: word(rowWords, 17)
    readonly property vector4d rowWord18: word(rowWords, 18)
    readonly property vector4d rowWord19: word(rowWords, 19)
    readonly property vector4d rowWord20: word(rowWords, 20)
    readonly property vector4d rowWord21: word(rowWords, 21)

    fragmentShader: Qt.resolvedUrl("shaders/blockgraph.frag.qsb")
}
