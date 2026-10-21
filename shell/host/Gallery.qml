import QtQuick
import GhostKit

// The GhostKit building blocks on fixed values, so they can be checked by eye and by the smoke test
// without depending on any daemon topic.
Item {
    id: root

    // Design pixels.
    readonly property real marginPx: 12
    readonly property real gutterPx: 12
    readonly property real barBandPx: 30
    readonly property real panelHeightPx: 200
    // The reference mockup's gap between the blocks of a panel body.
    readonly property real stackGapPx: 8
    readonly property real digitsPx: 30

    readonly property int panelCount: 3
    readonly property int rampSamples: 32
    readonly property real rampTop: 100
    readonly property real batteryPercent: 15

    readonly property real panelWidth: (width - 2 * Theme.d(marginPx) - (panelCount - 1) * Theme.d(gutterPx)) / panelCount

    function ramp(count, top) {
        return Array.from({
            length: count
        }, (_, i) => top * i / (count - 1));
    }

    Row {
        x: Theme.d(root.marginPx)
        y: Theme.d(root.marginPx + root.barBandPx)
        spacing: Theme.d(root.gutterPx)

        Panel {
            width: root.panelWidth
            height: Theme.d(root.panelHeightPx)
            label: "Meters"
            tagKey: "cpu"
            meta: "0.20 · 0.80 · 0.95 · BAT 15"

            Column {
                width: parent.width
                spacing: Theme.d(root.stackGapPx)

                SegMeter {
                    width: parent.width
                    value: 0.2
                }

                SegMeter {
                    width: parent.width
                    value: 0.8
                }

                SegMeter {
                    width: parent.width
                    value: 0.95
                }

                SegMeter {
                    width: parent.width
                    value: root.batteryPercent / Theme.percent
                    low: true
                }
            }
        }

        Panel {
            width: root.panelWidth
            height: Theme.d(root.panelHeightPx)
            label: "Graph"
            tagKey: "gpu"
            meta: "RAMP 0-100"

            Digits {
                id: clock

                text: "20:07"
                pixelSize: Theme.d(root.digitsPx)
            }

            BlockGraph {
                anchors {
                    top: clock.bottom
                    topMargin: Theme.d(root.stackGapPx)
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                }
                values: root.ramp(root.rampSamples, root.rampTop)
                cols: root.rampSamples
            }
        }

        Panel {
            width: root.panelWidth
            height: Theme.d(root.panelHeightPx)
            label: "Alerts"
            tagKey: "disk"
            meta: "WARN · CRIT"

            Column {
                width: parent.width
                spacing: Theme.d(root.stackGapPx)

                AlertBox {
                    width: parent.width
                    level: "warn"
                    text: "INMET · STORM WARNING (ORANGE)"
                    trailing: "01:24:09"
                }

                AlertBox {
                    width: parent.width
                    level: "crit"
                    chipKey: "crit"
                    text: "WD GREEN 960G · NVME1"
                    trailing: "DEAD"
                }
            }
        }
    }
}
