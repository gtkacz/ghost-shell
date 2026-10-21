import QtQuick
import GhostKit
import "kit/GhostKit/lib/segments.mjs" as Segments

// BlockGraph's interface and look, drawn by one fragment shader instead of one Rectangle per cell. The
// per-column work (which sample, how many cells lit, which level) stays in JavaScript and segments.mjs;
// it reaches the shader packed into uniforms, so an update is one uniform change and renders one frame.
ShaderEffect {
    id: root

    property var values: []
    property real lo: 0
    property real hi: 100
    property int cols: 0
    property real cellW: 3
    property real cellH: 2
    property real gap: 1
    property real warnAt: Theme.loadWarn
    property real critAt: Theme.loadCrit
    property int minLit: 1
    property bool sourceLive: true

    readonly property bool live: Data.connected && !Data.stale && sourceLive
    readonly property real gapSize: Theme.d(gap)
    readonly property real cellHeight: Theme.d(cellH)
    // real rather than BlockGraph's int: ShaderEffect handed an int property to its float uniform as 0.
    readonly property real columnCount: cols > 0 ? cols : Segments.cellCount(width, Theme.d(cellW), gapSize)
    readonly property real columnWidth: cols > 0 ? (width - (cols - 1) * gapSize) / cols : Theme.d(cellW)
    readonly property real rowCount: Segments.cellCount(height, cellHeight, gapSize)
    readonly property real firstColumnX: width - columnCount * (columnWidth + gapSize) + gapSize

    readonly property var levelCodes: ({
            ok: 0,
            warn: 1,
            crit: 2
        })
    // As blockgraph.frag unpacks them: a column's byte is lit * levels + level, three bytes per component,
    // the first in the lowest bits, so every component is an integer a float holds exactly.
    readonly property int levels: 4
    readonly property int byteRange: 256
    readonly property int byteMax: 255
    readonly property int bytesPerComponent: 3
    readonly property int componentsPerWord: 4
    readonly property int bytesPerWord: bytesPerComponent * componentsPerWord
    readonly property int columnWords: 32
    readonly property int rowWords: 6
    readonly property int maxColumns: columnWords * bytesPerWord
    // The lit count must leave room for the level in one byte.
    readonly property int maxRows: Math.floor(byteRange / levels) - 1

    onColumnCountChanged: {
        if (columnCount > maxColumns) {
            console.warn("ShaderBlockGraph: " + columnCount + " columns; only the newest " + maxColumns + " can light");
        }
    }
    onRowCountChanged: {
        if (rowCount > maxRows) {
            console.warn("ShaderBlockGraph: " + rowCount + " rows; only the lowest " + maxRows + " can light");
        }
    }

    // Shader uniforms. The colours come from segments.mjs rather than being restated in the shader; the
    // ramp's per-row alpha travels in the row words.
    readonly property size itemSize: Qt.size(width, height)
    readonly property color unlitColor: Segments.cellColor("ok", 0, 0, 1, Theme)
    readonly property color rampFloor: Segments.cellColor("ok", 0, 2, rowCount, Theme)
    readonly property color rampColor: Qt.rgba(rampFloor.r, rampFloor.g, rampFloor.b, 1)
    readonly property color hiColor: Theme.hi
    readonly property color warnColor: Theme.warn
    readonly property color critColor: Theme.crit

    // The words hold the newest maxColumns columns; any older ones draw unlit.
    readonly property real skippedColumns: Math.max(0, columnCount - maxColumns)
    readonly property var columnBytes: {
        const series = values || [];
        const bytes = [];
        for (let column = skippedColumns; column < columnCount; column += 1) {
            // BlockGraph.sampleAt's rule: a series shorter than the graph fills it from the right.
            const at = series.length - columnCount + column;
            const sample = at >= 0 ? series[at] : undefined;
            const lit = live && Number.isFinite(sample) ? Segments.litCells(sample, lo, hi, rowCount, minLit) : 0;
            const level = levelCodes[Segments.level((sample - lo) / (hi - lo), warnAt, critAt)];
            bytes.push(Math.min(lit, maxRows) * levels + level);
        }
        return pack(bytes, columnWords);
    }
    readonly property var rowBytes: {
        const bytes = [];
        for (let row = 0; row < Math.min(rowCount, rowWords * bytesPerWord); row += 1) {
            bytes.push(Math.round(Qt.color(Segments.cellColor("ok", row, row + 2, rowCount, Theme)).a * byteMax));
        }
        return pack(bytes, rowWords);
    }

    function pack(bytes, wordCount) {
        const words = [];
        for (let word = 0; word < wordCount; word += 1) {
            const components = [];
            for (let slot = 0; slot < componentsPerWord; slot += 1) {
                let value = 0;
                for (let shift = bytesPerComponent - 1; shift >= 0; shift -= 1) {
                    value = value * byteRange + (bytes[(word * componentsPerWord + slot) * bytesPerComponent + shift] || 0);
                }
                components.push(value);
            }
            words.push(Qt.vector4d(components[0], components[1], components[2], components[3]));
        }
        return words;
    }

    // ShaderEffect maps properties to uniforms by name and ignores arrays, hence one property per word.
    readonly property vector4d c0: columnBytes[0]
    readonly property vector4d c1: columnBytes[1]
    readonly property vector4d c2: columnBytes[2]
    readonly property vector4d c3: columnBytes[3]
    readonly property vector4d c4: columnBytes[4]
    readonly property vector4d c5: columnBytes[5]
    readonly property vector4d c6: columnBytes[6]
    readonly property vector4d c7: columnBytes[7]
    readonly property vector4d c8: columnBytes[8]
    readonly property vector4d c9: columnBytes[9]
    readonly property vector4d c10: columnBytes[10]
    readonly property vector4d c11: columnBytes[11]
    readonly property vector4d c12: columnBytes[12]
    readonly property vector4d c13: columnBytes[13]
    readonly property vector4d c14: columnBytes[14]
    readonly property vector4d c15: columnBytes[15]
    readonly property vector4d c16: columnBytes[16]
    readonly property vector4d c17: columnBytes[17]
    readonly property vector4d c18: columnBytes[18]
    readonly property vector4d c19: columnBytes[19]
    readonly property vector4d c20: columnBytes[20]
    readonly property vector4d c21: columnBytes[21]
    readonly property vector4d c22: columnBytes[22]
    readonly property vector4d c23: columnBytes[23]
    readonly property vector4d c24: columnBytes[24]
    readonly property vector4d c25: columnBytes[25]
    readonly property vector4d c26: columnBytes[26]
    readonly property vector4d c27: columnBytes[27]
    readonly property vector4d c28: columnBytes[28]
    readonly property vector4d c29: columnBytes[29]
    readonly property vector4d c30: columnBytes[30]
    readonly property vector4d c31: columnBytes[31]
    readonly property vector4d r0: rowBytes[0]
    readonly property vector4d r1: rowBytes[1]
    readonly property vector4d r2: rowBytes[2]
    readonly property vector4d r3: rowBytes[3]
    readonly property vector4d r4: rowBytes[4]
    readonly property vector4d r5: rowBytes[5]

    fragmentShader: Qt.resolvedUrl("blockgraph.frag.qsb")
}
