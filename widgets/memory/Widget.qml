import QtQuick
import GhostKit

// Used memory, the RAM and swap meters, and the busiest processes, as many whole rows as fit.
Panel {
    id: root

    // Design pixels, from the reference mockup's Memory panel.
    readonly property real stackGapPx: 8
    readonly property real rowGapPx: 5
    readonly property real bigPx: 30
    readonly property real captionPx: 9
    readonly property real captionGapPx: 8
    readonly property real valuePx: 10
    readonly property real processHeaderPx: 8.5
    readonly property real processPx: 9.5
    readonly property real processGapPx: 2
    readonly property real processColumnPx: 44
    // Letter spacing in em.
    readonly property real captionTracking: 0.12
    readonly property real processHeaderTracking: 0.14
    readonly property int processDecimals: 1

    property var settings

    readonly property var mem: Data.topic("mem").value
    readonly property var procs: Data.topic("procs").value
    readonly property bool live: Data.connected && !Data.stale

    readonly property var usedKib: mem ? mem.used_kib : null
    readonly property var totalKib: mem ? mem.total_kib : null
    readonly property var swapUsedKib: mem ? mem.swap_used_kib : null
    readonly property var swapTotalKib: mem ? mem.swap_total_kib : null
    readonly property var ramShare: share(usedKib, totalKib)
    readonly property var processes: procs ? procs.top : []

    // The widest text each row can reach, so the meters keep one length while the values change.
    readonly property string widestValue: [Fmt.pct(Theme.percent), swapText(swapTotalKib)].reduce((widest, text) => text.length > widest.length ? text : widest)

    function share(part, whole) {
        return Number.isFinite(part) && whole > 0 ? part / whole : null;
    }

    // A machine without swap has no swap reading rather than an empty one.
    function swapText(usedKib) {
        if (!Number.isFinite(usedKib) || !(swapTotalKib > 0)) {
            return Fmt.gib(null);
        }
        return Fmt.gib(usedKib) + " / " + Fmt.gib(swapTotalKib, 0) + "G";
    }

    // Missing and stale readings are dim, so "--" never reads as a value.
    function tone(value, liveColor) {
        return live && Number.isFinite(value) ? liveColor : Theme.dim;
    }

    component MeterRow: Item {
        id: row

        // Design pixels, from the mockup's .row.
        readonly property real labelWidthPx: 38
        readonly property real gapPx: 8
        readonly property real textPx: 10

        property string label
        property var fraction
        property string value
        property color valueColor
        property real valueWidth

        implicitHeight: labelText.implicitHeight

        Text {
            id: labelText

            width: Theme.d(row.labelWidthPx)
            text: row.label
            color: Theme.dim
            elide: Text.ElideRight
            font {
                family: Theme.font
                pixelSize: Theme.d(row.textPx)
                weight: Font.Medium
            }
        }

        SegMeter {
            anchors {
                left: labelText.right
                leftMargin: Theme.d(row.gapPx)
                right: parent.right
                rightMargin: row.valueWidth + Theme.d(row.gapPx)
                verticalCenter: parent.verticalCenter
            }
            value: row.fraction
        }

        Digits {
            anchors {
                right: parent.right
                verticalCenter: parent.verticalCenter
            }
            text: row.value
            pixelSize: Theme.d(row.textPx)
            weight: Font.Medium
            color: row.valueColor
        }
    }

    anchors.fill: parent
    label: "Memory"
    tagKey: "mem"
    meta: Number.isFinite(totalKib) ? Fmt.gib(totalKib) + " GIB" : "--"

    TextMetrics {
        id: valueSpan

        font {
            family: Theme.font
            pixelSize: Theme.d(root.valuePx)
            weight: Font.Medium
        }
        text: root.widestValue
    }

    FontMetrics {
        id: processLine

        font {
            family: Theme.font
            pixelSize: Theme.d(root.processPx)
            weight: Font.Medium
        }
    }

    Digits {
        id: big

        text: Fmt.gib(root.usedKib)
        pixelSize: Theme.d(root.bigPx)
        color: root.tone(root.usedKib, Theme.hi)
    }

    Text {
        anchors {
            left: big.right
            leftMargin: Theme.d(root.captionGapPx)
            baseline: big.baseline
        }
        text: "/ " + Fmt.gib(root.totalKib) + " GiB"
        color: Theme.dim
        font {
            family: Theme.font
            pixelSize: Theme.d(root.captionPx)
            weight: Font.Medium
            letterSpacing: Theme.d(root.captionPx) * root.captionTracking
        }
    }

    Column {
        id: rows

        anchors {
            top: big.bottom
            topMargin: Theme.d(root.stackGapPx)
            left: parent.left
            right: parent.right
        }
        spacing: Theme.d(root.rowGapPx)

        MeterRow {
            width: rows.width
            label: "RAM"
            fraction: root.ramShare
            value: Fmt.pct(Number.isFinite(root.ramShare) ? root.ramShare * Theme.percent : null)
            valueColor: root.tone(root.ramShare, Theme.hi)
            valueWidth: valueSpan.advanceWidth
        }

        MeterRow {
            width: rows.width
            label: "SWAP"
            fraction: root.share(root.swapUsedKib, root.swapTotalKib)
            value: root.swapText(root.swapUsedKib)
            valueColor: root.tone(root.share(root.swapUsedKib, root.swapTotalKib), Theme.hi)
            valueWidth: valueSpan.advanceWidth
        }
    }

    Item {
        id: processList

        readonly property real columnWidth: Theme.d(root.processColumnPx)
        readonly property real rowHeight: Math.ceil(processLine.height)
        readonly property real rowStep: rowHeight + Theme.d(root.processGapPx)
        readonly property int fitting: Math.max(0, Math.floor((height - header.height) / rowStep))
        readonly property int shown: Math.min(root.settings.top, root.processes.length, fitting)
        readonly property color rowColor: root.live ? Theme.text : Theme.dim

        anchors {
            top: rows.bottom
            topMargin: Theme.d(root.stackGapPx)
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }

        Item {
            id: header

            width: parent.width
            height: processHeader.implicitHeight

            Text {
                id: processHeader

                anchors {
                    left: parent.left
                    right: cpuHeader.left
                }
                text: "PROCESS"
                color: Theme.dim
                elide: Text.ElideRight
                font {
                    family: Theme.font
                    pixelSize: Theme.d(root.processHeaderPx)
                    weight: Font.Medium
                    letterSpacing: Theme.d(root.processHeaderPx) * root.processHeaderTracking
                }
            }

            Text {
                id: cpuHeader

                anchors.right: memHeader.left
                width: processList.columnWidth
                horizontalAlignment: Text.AlignRight
                text: "CPU"
                color: Theme.dim
                font: processHeader.font
            }

            Text {
                id: memHeader

                anchors.right: parent.right
                width: processList.columnWidth
                horizontalAlignment: Text.AlignRight
                text: "MEM"
                color: Theme.dim
                font: processHeader.font
            }
        }

        Repeater {
            model: processList.shown

            Item {
                id: process

                required property int index
                readonly property var entry: root.processes[index]

                y: header.height + Theme.d(root.processGapPx) + index * processList.rowStep
                width: processList.width
                height: processList.rowHeight

                Text {
                    anchors {
                        left: parent.left
                        right: parent.right
                        rightMargin: 2 * processList.columnWidth
                    }
                    text: process.entry.name
                    // Any local process picks its own name, which AutoText would render as markup.
                    textFormat: Text.PlainText
                    color: processList.rowColor
                    elide: Text.ElideRight
                    font: processLine.font
                }

                Digits {
                    anchors {
                        right: parent.right
                        rightMargin: processList.columnWidth
                    }
                    text: Fmt.pct(process.entry.cpu, root.processDecimals)
                    pixelSize: Theme.d(root.processPx)
                    weight: Font.Medium
                    color: processList.rowColor
                }

                Digits {
                    anchors.right: parent.right
                    text: Fmt.pct(process.entry.mem, root.processDecimals)
                    pixelSize: Theme.d(root.processPx)
                    weight: Font.Medium
                    color: processList.rowColor
                }
            }
        }
    }
}
