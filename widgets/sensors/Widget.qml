import QtQuick
import GhostKit

// Every temperature, coloured past its limits, then every fan, then every device battery. Two readings share a line,
// so a whole machine fits the panel's usual height; lines that still don't fit are dropped whole, from the last group
// up, while the meta still counts every probe.
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

    readonly property var sensors: Data.topic("sensors").value
    readonly property bool live: Data.connected && !Data.stale
    readonly property var temps: sensors ? sensors.temps : []
    readonly property var fans: sensors ? sensors.fans : []
    readonly property var batteries: Data.topic("batteries").value
    readonly property var batteryDevices: batteries ? batteries.devices : []

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

    // A battery alarms as it runs low, and a missing charge stays dim like any missing reading.
    function batteryColor(pct) {
        const level = Fmt.levelLow(pct, Theme.batteryWarn, Theme.batteryCrit);
        return tone(pct, level === "crit" ? Theme.crit : level === "warn" ? Theme.warn : Theme.hi);
    }

    function batteryText(battery) {
        const charging = battery.charging && Number.isFinite(battery.pct);
        return Fmt.pct(battery.pct) + (charging ? "+" : "");
    }

    function valueText(entry) {
        if (entry.kind === "battery") {
            return batteryText(entry.reading);
        }
        return entry.kind === "fan" ? Fmt.rpm(entry.reading.value) : Fmt.tempC(entry.reading.value);
    }

    function valueColor(entry) {
        if (entry.kind === "battery") {
            return batteryColor(entry.reading.pct);
        }
        return entry.kind === "fan" ? tone(entry.reading.value, Theme.hi) : tempColor(entry.reading);
    }

    anchors.fill: parent
    label: "Sensors"
    tagKey: "sens"
    meta: sensors ? (temps.length + fans.length + batteryDevices.length) + " PROBES" : "--"

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
        // The readings that fit, each with its column and y: the temperatures, the fans, then the batteries, a
        // separator between each two groups that have readings.
        readonly property var cells: {
            const placed = [];
            let y = 0;
            const place = (readings, kind) => {
                for (let start = 0; start < readings.length; start += root.columns) {
                    if (y + lineHeight > height) {
                        return false;
                    }
                    for (let column = 0; column < root.columns && start + column < readings.length; column += 1) {
                        placed.push({
                            reading: readings[start + column],
                            kind: kind,
                            column: column,
                            y: y
                        });
                    }
                    y += rowStep;
                }
                return true;
            };
            const groups = [[root.temps, "temp"], [root.fans, "fan"], [root.batteryDevices, "battery"]];
            for (const [readings, kind] of groups) {
                if (readings.length === 0) {
                    continue;
                }
                if (placed.length > 0) {
                    y += Theme.d(root.separatorPx + root.rowGapPx);
                }
                if (!place(readings, kind)) {
                    break;
                }
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
                    // Labels can come from the user's config, a chip's own *_label file or a device's own model or name,
                    // which AutoText would render as markup.
                    textFormat: Text.PlainText
                    color: Theme.dim
                    elide: Text.ElideRight
                    font: line.font
                }

                Digits {
                    id: reading

                    anchors.right: parent.right
                    text: root.valueText(cell.entry)
                    pixelSize: Theme.d(root.textPx)
                    weight: Font.Medium
                    color: root.valueColor(cell.entry)
                }
            }
        }
    }
}
