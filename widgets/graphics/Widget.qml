import QtQuick
import GhostKit

// GPU load, memory, temperature, power and fan, with the load history. Without a GPU every reading is
// unavailable rather than zero.
Panel {
    id: root

    // Design pixels, from the reference mockup's Graphics panel.
    readonly property real stackGapPx: 8
    readonly property real rowGapPx: 5
    readonly property real bigPx: 30
    readonly property real captionPx: 9
    readonly property real captionGapPx: 8
    readonly property real valuePx: 10
    // Letter spacing in em.
    readonly property real captionTracking: 0.12
    // The temperature meter spans 0 to 100 °C, as the mockup's does.
    readonly property real tempScaleC: 100
    readonly property int kibPerMib: 1024

    property var settings

    readonly property var gpu: Data.topics.gpu || null
    readonly property bool present: gpu !== null && gpu.present
    readonly property bool live: Data.connected && !Data.stale

    readonly property var load: present ? gpu.load : null
    readonly property var vramUsedKib: present ? gpu.vram_used_mib * kibPerMib : null
    readonly property var vramTotalKib: present ? gpu.vram_total_mib * kibPerMib : null
    readonly property var temp: present ? gpu.temp_c : null
    readonly property var power: present ? gpu.power_w : null
    readonly property var powerLimit: present ? gpu.power_limit_w : null
    readonly property var fan: present ? gpu.fan_pct : null

    // The widest text each row can reach, so the meters keep one length while the values change.
    readonly property string widestValue: [vramText(vramTotalKib), Fmt.tempC(tempScaleC), Fmt.watts(powerLimit), Fmt.pct(Theme.percent)].reduce((widest, text) => text.length > widest.length ? text : widest)

    function vramText(usedKib) {
        if (!Number.isFinite(usedKib) || !Number.isFinite(vramTotalKib)) {
            return Fmt.gib(null);
        }
        return Fmt.gib(usedKib) + " / " + Fmt.gib(vramTotalKib, 0) + "G";
    }

    function share(part, whole) {
        return Number.isFinite(part) && whole > 0 ? part / whole : null;
    }

    // Missing and stale readings are dim, so "--" never reads as a value.
    function tone(value) {
        return live && Number.isFinite(value) ? Theme.hi : Theme.dim;
    }

    component MeterRow: Item {
        id: row

        // Design pixels, from the mockup's .row.
        readonly property real labelWidthPx: 38
        readonly property real gapPx: 8
        readonly property real textPx: 10

        property string label
        property var fraction
        property real warnAt: Theme.loadWarn
        property real critAt: Theme.loadCrit
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
            warnAt: row.warnAt
            critAt: row.critAt
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
    label: "Graphics"
    tagKey: "gpu"
    meta: present ? Fmt.gpuNameShort(gpu.name) : "--"

    TextMetrics {
        id: valueSpan

        font {
            family: Theme.font
            pixelSize: Theme.d(root.valuePx)
            weight: Font.Medium
        }
        text: root.widestValue
    }

    Digits {
        id: big

        text: Fmt.pct(root.load)
        pixelSize: Theme.d(root.bigPx)
        color: root.tone(root.load)
    }

    Text {
        anchors {
            left: big.right
            leftMargin: Theme.d(root.captionGapPx)
            baseline: big.baseline
        }
        text: "LOAD"
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
            label: "VRAM"
            fraction: root.share(root.vramUsedKib, root.vramTotalKib)
            value: root.vramText(root.vramUsedKib)
            valueColor: root.tone(root.vramUsedKib)
            valueWidth: valueSpan.advanceWidth
        }

        MeterRow {
            width: rows.width
            label: "TEMP"
            fraction: root.share(root.temp, root.tempScaleC)
            warnAt: Theme.tempWarn / root.tempScaleC
            critAt: Theme.tempCrit / root.tempScaleC
            value: Fmt.tempC(root.temp)
            valueColor: root.tone(root.temp)
            valueWidth: valueSpan.advanceWidth
        }

        MeterRow {
            width: rows.width
            label: "POWER"
            fraction: root.share(root.power, root.powerLimit)
            value: Fmt.watts(root.power)
            valueColor: root.tone(root.power)
            valueWidth: valueSpan.advanceWidth
        }

        MeterRow {
            width: rows.width
            label: "FAN"
            fraction: root.share(root.fan, Theme.percent)
            value: Fmt.pct(root.fan)
            valueColor: root.tone(root.fan)
            valueWidth: valueSpan.advanceWidth
        }
    }

    BlockGraph {
        anchors {
            top: rows.bottom
            topMargin: Theme.d(root.stackGapPx)
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        // Without a GPU there is no history, which is unavailable, not idle.
        values: root.present ? Data.history.gpu_load || [] : []
    }
}
