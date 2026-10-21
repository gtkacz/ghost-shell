import QtQuick
import "lib/segments.mjs" as Segments

Item {
    id: root

    // Design pixels.
    readonly property real cellPx: 4
    readonly property real gapPx: 2
    readonly property real heightPx: 7

    // A fraction, 0..1. Anything else (null, undefined, NaN) is an unavailable reading.
    property var value
    // When true, low values are the alarm, as for batteries.
    property bool low: false
    property real warnAt: low ? Theme.batteryWarn / Theme.percent : Theme.loadWarn
    property real critAt: low ? Theme.batteryCrit / Theme.percent : Theme.loadCrit
    // Whether the reading's own source is live, such as Data.topic(name).live.
    property bool sourceLive: true

    readonly property bool live: Data.connected && !Data.stale && sourceLive
    readonly property bool available: live && Number.isFinite(value)
    readonly property real cellStep: Theme.d(cellPx + gapPx)
    readonly property int cells: Segments.cellCount(width, Theme.d(cellPx), Theme.d(gapPx))
    readonly property int lit: available ? Segments.litCells(value, 0, 1, cells) : 0
    readonly property string meterLevel: low ? Segments.levelLow(value, warnAt, critAt) : Segments.level(value, warnAt, critAt)
    readonly property color litColor: meterLevel === "crit" ? Theme.crit : meterLevel === "warn" ? Theme.warn : Theme.signal
    // A cell above the lit ones, so segments.mjs stays the one place that defines how unlit cells look.
    readonly property color unlitColor: Segments.cellColor("ok", 0, 0, 1, Theme)

    implicitHeight: Theme.d(heightPx)

    Repeater {
        model: root.cells

        Rectangle {
            required property int index

            x: index * root.cellStep
            width: Theme.d(root.cellPx)
            height: root.height
            color: index < root.lit ? root.litColor : root.unlitColor
        }
    }
}
