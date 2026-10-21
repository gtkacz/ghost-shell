import QtQuick
import GhostKit

// Every temperature, coloured past its limits, then every fan. Two readings share a line, so a whole
// machine fits the panel's usual height; lines that still don't fit are dropped whole.
Panel {
    id: root

    // Design pixels, from the reference mockup's Sensors panel.
    readonly property real textPx: 9.5
    readonly property real rowGapPx: 2
    readonly property real separatorPx: 4
    readonly property real valueGapPx: 6
    // As between the mockup's CCD groups.
    readonly property real columnGapPx: 12
    readonly property int columns: 2

    property var settings

    readonly property var sensors: Data.topics.sensors || null
    readonly property bool live: Data.connected && !Data.stale
    readonly property var temps: sensors ? sensors.temps : []
    readonly property var fans: sensors ? sensors.fans : []

    // Missing and stale readings are dim, so "--" never reads as a value.
    function tone(value, liveColor) {
        return live && Number.isFinite(value) ? liveColor : Theme.dim;
    }

    // A sensor with its own limits in the config uses them instead of the temperature thresholds.
    function tempColor(reading) {
        const limits = (Theme.thresholds.sensors || {})[reading.id] || {
            warn: Theme.tempWarn,
            crit: Theme.tempCrit
        };
        if (reading.value >= limits.crit) {
            return tone(reading.value, Theme.crit);
        }
        if (reading.value >= limits.warn) {
            return tone(reading.value, Theme.warn);
        }
        return tone(reading.value, Theme.hi);
    }

    anchors.fill: parent
    label: "Sensors"
    tagKey: "sens"
    meta: sensors ? (temps.length + fans.length) + " PROBES" : "--"

    FontMetrics {
        id: line

        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
            weight: Font.Medium
        }
    }

    Item {
        id: grid

        readonly property real gap: Theme.d(root.columnGapPx)
        readonly property real columnWidth: (width - (root.columns - 1) * gap) / root.columns
        readonly property real lineHeight: Math.ceil(line.height)
        readonly property real rowStep: lineHeight + Theme.d(root.rowGapPx)
        // The readings that fit, each with its column and y: the temperatures, then the fans.
        readonly property var cells: {
            const placed = [];
            let y = 0;
            const place = (readings, isFan) => {
                for (let start = 0; start < readings.length; start += root.columns) {
                    if (y + lineHeight > height) {
                        return false;
                    }
                    for (let column = 0; column < root.columns && start + column < readings.length; column += 1) {
                        placed.push({
                            reading: readings[start + column],
                            isFan: isFan,
                            column: column,
                            y: y
                        });
                    }
                    y += rowStep;
                }
                return true;
            };
            if (place(root.temps, false)) {
                if (root.temps.length > 0) {
                    y += Theme.d(root.separatorPx + root.rowGapPx);
                }
                place(root.fans, true);
            }
            return placed;
        }

        anchors.fill: parent

        Repeater {
            model: grid.cells.length

            Item {
                id: cell

                required property int index
                readonly property var entry: grid.cells[index]
                readonly property var value: entry.reading.value

                x: entry.column * (grid.columnWidth + grid.gap)
                y: entry.y
                width: grid.columnWidth
                height: grid.lineHeight

                Text {
                    anchors {
                        left: parent.left
                        right: reading.left
                        rightMargin: Theme.d(root.valueGapPx)
                    }
                    text: cell.entry.reading.label
                    color: Theme.dim
                    elide: Text.ElideRight
                    font: line.font
                }

                Digits {
                    id: reading

                    anchors.right: parent.right
                    text: cell.entry.isFan ? Fmt.rpm(cell.value) : Fmt.tempC(cell.value)
                    pixelSize: Theme.d(root.textPx)
                    weight: Font.Medium
                    color: cell.entry.isFan ? root.tone(cell.value, Theme.hi) : root.tempColor(cell.entry.reading)
                }
            }
        }
    }
}
