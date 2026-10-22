import QtQuick
import "lib/heatgraph.mjs" as Packing
import "lib/segments.mjs" as Segments

// Several histories as rows of cells whose brightness shows the value, such as one row per CPU core, the top row
// first and the newest sample on the right. Each band of HEAT_BAND_ROWS rows is one shader whose cells sit in a ring,
// so a new sample re-sends one word per band rather than every word: re-sending them all cost about 0.2 % of a core
// per 16-row lane, and a single shader for all the rows cost NVIDIA's driver about 20 MiB more than the bands' one.
Item {
    id: root

    // One series per row.
    property var rows: []
    property real lo: 0
    property real hi: 100
    // Fractions of lo..hi.
    property real warnAt: Theme.loadWarn
    property real critAt: Theme.loadCrit
    // Design pixels.
    property real cellW: 4
    property real gap: 1
    // Whether the samples' own source is live, such as Data.topic(name).live.
    property bool sourceLive: true

    readonly property bool live: Data.connected && !Data.stale && sourceLive
    readonly property real gapSize: Theme.d(gap)
    readonly property real columnWidth: Theme.d(cellW)
    readonly property real columnCount: Segments.cellCount(width, columnWidth, gapSize)
    readonly property real rowHeight: Packing.heatRowHeight(height, rows.length, gapSize)
    // Leftover width goes on the left, so the newest sample always sits on the right edge.
    readonly property real firstColumnX: width - columnCount * (columnWidth + gapSize) + gapSize
    readonly property var graph: ({
            lo,
            hi,
            warnAt,
            critAt,
            live
        })

    property bool warnedColumns: false

    // Once, since a resize can cross the limit many times.
    onColumnCountChanged: {
        if (columnCount > Packing.HEAT_MAX_COLUMNS && !warnedColumns) {
            warnedColumns = true;
            console.warn("HeatGraph draws at most " + Packing.HEAT_MAX_COLUMNS + " columns, got " + columnCount + "; the oldest stay unlit");
        }
    }

    Repeater {
        model: Math.ceil(root.rows.length / Packing.HEAT_BAND_ROWS)

        HeatBand {
            required property int index
            readonly property int firstRow: index * Packing.HEAT_BAND_ROWS

            y: firstRow * (root.rowHeight + root.gapSize)
            width: root.width
            height: rowCount * (root.rowHeight + root.gapSize) - root.gapSize
            series: root.rows.slice(firstRow, firstRow + Packing.HEAT_BAND_ROWS)
            graph: root.graph
            columnCount: root.columnCount
            columnWidth: root.columnWidth
            gapSize: root.gapSize
            rowHeight: root.rowHeight
            firstColumnX: root.firstColumnX
        }
    }
}
