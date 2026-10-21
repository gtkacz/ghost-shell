import QtQuick
import "lib/segments.mjs" as Segments

// A history graph drawn as an LED matrix: one column of cells per sample, the newest on the right.
Item {
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
    readonly property int columnCount: cols > 0 ? cols : Segments.cellCount(width, Theme.d(cellW), gapSize)
    readonly property real columnWidth: cols > 0 ? (width - (cols - 1) * gapSize) / cols : Theme.d(cellW)
    readonly property int rowCount: Segments.cellCount(height, cellHeight, gapSize)
    // Leftover width goes on the left, so the newest sample always sits on the right edge.
    readonly property real firstColumnX: width - columnCount * (columnWidth + gapSize) + gapSize
    // A cell above the lit ones, so segments.mjs stays the one place that defines how unlit cells look.
    readonly property color unlitColor: Segments.cellColor("ok", 0, 0, 1, Theme)

    // A series shorter than the graph fills it from the right; the columns before it read as missing.
    function sampleAt(column) {
        const series = values || [];
        const at = series.length - columnCount + column;
        return at >= 0 ? series[at] : undefined;
    }

    Repeater {
        model: root.columnCount

        Item {
            id: column

            required property int index
            readonly property var sample: root.sampleAt(index)
            // A missing sample lights nothing, even below minLit, so it reads as unavailable rather than idle.
            readonly property int lit: root.live && Number.isFinite(sample) ? Segments.litCells(sample, root.lo, root.hi, root.rowCount, root.minLit) : 0
            readonly property string columnLevel: Segments.level((sample - root.lo) / (root.hi - root.lo), root.warnAt, root.critAt)

            x: root.firstColumnX + index * (root.columnWidth + root.gapSize)
            width: root.columnWidth
            height: root.height

            Repeater {
                model: root.rowCount

                Rectangle {
                    required property int index

                    y: root.height - (index + 1) * (root.cellHeight + root.gapSize) + root.gapSize
                    width: column.width
                    height: root.cellHeight
                    // Unlit cells share one colour, so a full-width graph scrolling at 1 Hz recomputes the
                    // colour only of the cells that are lit.
                    color: index < column.lit ? Segments.cellColor(column.columnLevel, index, column.lit, root.rowCount, Theme) : root.unlitColor
                }
            }
        }
    }
}
