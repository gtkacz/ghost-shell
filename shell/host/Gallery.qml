import QtQuick
import Quickshell
import GhostKit
import "../kit/GhostKit/lib/theme.mjs" as ThemeLib

// The GhostKit building blocks on fixed values, so they can be checked by eye and by the smoke test
// without depending on any daemon topic.
Item {
    id: root

    // Design pixels.
    readonly property real marginPx: 12
    readonly property real gutterPx: 12
    readonly property real barBandPx: 30
    readonly property real panelHeightPx: 200
    readonly property real bandHeightPx: 130
    // The reference mockup's gap between the blocks of a panel body.
    readonly property real stackGapPx: 8
    readonly property real digitsPx: 30
    readonly property real glyphPx: 20
    readonly property real scrambleTextPx: 14
    readonly property real toggleHeightPx: 24
    readonly property real toggleTextPx: 10
    readonly property real toggleSidePx: 10
    readonly property real toggleBorderPx: 1

    readonly property int panelCount: 3
    readonly property int rampSamples: 32
    readonly property real rampTop: 100
    readonly property real batteryPercent: 15
    readonly property real tracking: 0.1

    readonly property string scrambleDemoKey: "gallery:first-write"
    readonly property var chipKeys: Object.keys(ThemeLib.CHIPS)
    readonly property var glyphNames: ["chevron-up", "chevron-down", "chevron-left", "chevron-right", "swap", "prev", "next", "play", "pause", "search", "scroll"]

    readonly property real panelWidth: (width - 2 * Theme.d(marginPx) - (panelCount - 1) * Theme.d(gutterPx)) / panelCount
    readonly property real columnStep: panelWidth + Theme.d(gutterPx)
    readonly property real secondRowY: Theme.d(marginPx + barBandPx + panelHeightPx + gutterPx)
    readonly property real thirdRowY: secondRowY + Theme.d(panelHeightPx + gutterPx)

    // Held through the toggle below; GHOST_GALLERY_FAST=1 starts it held, so a run can measure fast mode
    // without a click.
    property bool fastHeld: Quickshell.env("GHOST_GALLERY_FAST") === "1"
    property bool holdsFast: false

    // The window passes every click through except these, the way the layout view does for its tab strips.
    readonly property var inputRects: [
        {
            x: Math.floor(fastToggle.x),
            y: Math.floor(fastToggle.y),
            width: Math.ceil(fastToggle.x + fastToggle.width) - Math.floor(fastToggle.x),
            height: Math.ceil(fastToggle.y + fastToggle.height) - Math.floor(fastToggle.y)
        }
    ]

    function ramp(count, top) {
        return Array.from({
            length: count
        }, (_, i) => top * i / (count - 1));
    }

    function holdFast(wanted) {
        if (wanted === holdsFast) {
            return;
        }
        if (wanted) {
            FrameClock.acquireFast();
        } else {
            FrameClock.releaseFast();
        }
        holdsFast = wanted;
    }

    onFastHeldChanged: holdFast(fastHeld)
    Component.onCompleted: {
        holdFast(fastHeld);
        FrameClock.acquireSeconds();
    }
    Component.onDestruction: {
        holdFast(false);
        FrameClock.releaseSeconds();
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

    Row {
        x: Theme.d(root.marginPx)
        y: root.secondRowY
        spacing: Theme.d(root.gutterPx)

        Panel {
            width: root.panelWidth
            height: Theme.d(root.panelHeightPx)
            label: "Chips"
            tagKey: "notif"
            meta: "EVERY KEY"

            Flow {
                width: parent.width
                spacing: Theme.d(root.stackGapPx)

                Repeater {
                    model: root.chipKeys

                    Chip {
                        required property string modelData

                        key: modelData
                    }
                }
            }
        }

        Panel {
            width: root.panelWidth
            height: Theme.d(root.panelHeightPx)
            label: "Glyphs"
            tagKey: "net"
            meta: "12-UNIT BOX"

            Flow {
                width: parent.width
                spacing: Theme.d(root.gutterPx)

                Repeater {
                    model: root.glyphNames

                    Glyph {
                        required property string modelData

                        name: modelData
                        size: Theme.d(root.glyphPx)
                        color: Theme.hi
                    }
                }
            }
        }

        Panel {
            width: root.panelWidth
            height: Theme.d(root.panelHeightPx)
            label: "Clock modes"
            tagKey: "clock"
            meta: "SCRAMBLE · EPOCH"

            Column {
                width: parent.width
                spacing: Theme.d(root.stackGapPx)

                ScrambleText {
                    width: parent.width
                    text: "FIRST WRITE SCRAMBLES"
                    scrambleKey: root.scrambleDemoKey
                    color: Theme.hi
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.scrambleTextPx)
                        weight: Font.Bold
                    }
                }

                // The same key, so the first one has already taken the write.
                ScrambleText {
                    width: parent.width
                    text: "SAME KEY, SHOWN DIRECTLY"
                    scrambleKey: root.scrambleDemoKey
                    color: Theme.text
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.scrambleTextPx)
                    }
                }

                Digits {
                    text: "EPOCH " + FrameClock.epochSecond
                    pixelSize: Theme.d(root.scrambleTextPx)
                    weight: Font.Medium
                }
            }
        }
    }

    Row {
        x: Theme.d(root.marginPx)
        y: root.thirdRowY
        spacing: Theme.d(root.gutterPx)

        Panel {
            width: root.panelWidth
            height: Theme.d(root.bandHeightPx)
            header: false
            padding: Theme.d(root.gutterPx)

            Digits {
                anchors.verticalCenter: parent.verticalCenter
                text: "HEADERLESS"
                pixelSize: Theme.d(root.digitsPx)
            }
        }

        Panel {
            width: root.panelWidth
            height: Theme.d(root.bandHeightPx)
            header: false
            framed: false

            Digits {
                anchors.centerIn: parent
                text: "FRAMELESS"
                pixelSize: Theme.d(root.digitsPx)
            }
        }
    }

    Rectangle {
        id: fastToggle

        x: Theme.d(root.marginPx) + 2 * root.columnStep
        y: root.thirdRowY
        width: toggleLabel.implicitWidth + Theme.d(2 * (root.toggleSidePx + root.toggleBorderPx))
        height: Theme.d(root.toggleHeightPx)
        color: "transparent"
        border {
            width: Theme.d(root.toggleBorderPx)
            color: root.fastHeld ? Theme.signal : Theme.hair
        }

        Text {
            id: toggleLabel

            anchors.centerIn: parent
            text: root.fastHeld ? "FAST CLOCK · HELD (33 MS)" : "FAST CLOCK · OFF (67 MS)"
            color: root.fastHeld ? Theme.hi : Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(root.toggleTextPx)
                weight: Font.Medium
                letterSpacing: Theme.d(root.toggleTextPx) * root.tracking
            }
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.fastHeld = !root.fastHeld
        }
    }
}
