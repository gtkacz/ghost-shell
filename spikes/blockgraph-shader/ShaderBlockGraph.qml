import QtQuick
import GhostKit
import "kit/GhostKit/lib/segments.mjs" as Segments

// BlockGraph's interface and look, drawn by one fragment shader instead of one Rectangle per cell. The
// per-column work (which sample, how many cells lit, which level) stays in JavaScript and segments.mjs;
// it reaches the shader as a two-row image.
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
    readonly property string imageHeader: "P3\n"
    readonly property int imageRows: 2
    readonly property int byteMax: 255
    readonly property string imageMaxValue: "\n" + byteMax + "\n"
    // Percent-encoded: Qt.btoa(string) is deprecated and logs a warning on every call.
    readonly property string dataUrlPrefix: "data:image/x-portable-pixmap,"

    readonly property real imageWidth: Math.max(columnCount, rowCount)

    // Shader uniforms. The colours come from segments.mjs rather than being restated in the shader; the
    // ramp's per-row alpha travels in the image's second row.
    readonly property size itemSize: Qt.size(width, height)
    readonly property color unlitColor: Segments.cellColor("ok", 0, 0, 1, Theme)
    readonly property color rampFloor: Segments.cellColor("ok", 0, 2, rowCount, Theme)
    readonly property color rampColor: Qt.rgba(rampFloor.r, rampFloor.g, rampFloor.b, 1)
    readonly property color hiColor: Theme.hi
    readonly property color warnColor: Theme.warn
    readonly property color critColor: Theme.crit
    property var columns: columnImage

    // Row 0, one texel per column: the lit cell count and the level, by BlockGraph.sampleAt's rule (a series
    // shorter than the graph fills it from the right). Row 1, one texel per graph row: the ramp's alpha
    // byte there, as withAlpha() rounds it.
    function encodeImage() {
        const series = values || [];
        const texels = [];
        for (let column = 0; column < imageWidth; column += 1) {
            const at = series.length - columnCount + column;
            const sample = column < columnCount && at >= 0 ? series[at] : undefined;
            const lit = live && Number.isFinite(sample) ? Segments.litCells(sample, lo, hi, rowCount, minLit) : 0;
            const level = levelCodes[Segments.level((sample - lo) / (hi - lo), warnAt, critAt)];
            texels.push(lit + " " + level + " 0");
        }
        for (let row = 0; row < imageWidth; row += 1) {
            const ramp = Qt.color(Segments.cellColor("ok", row, row + 2, rowCount, Theme));
            texels.push(Math.round(ramp.a * byteMax) + " 0 0");
        }
        return dataUrlPrefix + encodeURIComponent(imageHeader + imageWidth + " " + imageRows + imageMaxValue + texels.join(" ") + "\n");
    }

    fragmentShader: Qt.resolvedUrl("blockgraph.frag.qsb")

    Image {
        id: columnImage

        // A texture provider only: the shader reads it, it is never drawn itself.
        visible: false
        cache: false
        smooth: false
        // A data URL still decodes on a later event-loop turn, so a new source renders a frame before the
        // decoded one. Keeping the old image while loading left that frame out on about 40 % of updates at
        // 251 columns, and on none at 314.
        retainWhileLoading: true
        source: root.columnCount > 0 && root.rowCount > 0 ? root.encodeImage() : ""
    }
}
