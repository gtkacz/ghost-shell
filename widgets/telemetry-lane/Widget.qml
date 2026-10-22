import QtQuick
import GhostKit
import "lanes.mjs" as Lanes

// One MONITOR telemetry lane, chosen by the metric setting: a label block with the metric's name, its current value
// and a sub line, beside a full-width history graph. CPU is a heat grid with a row per core, network and disk I/O
// are mirror graphs, GPU and memory are bar graphs, and thermals are three thin bar graphs.
Panel {
    id: root

    readonly property string cpuMetric: "cpu"
    readonly property string gpuMetric: "gpu"
    readonly property string memMetric: "mem"
    readonly property string netMetric: "net"
    readonly property string diskMetric: "disk"
    readonly property string thermMetric: "therm"

    // Design pixels, from the reference mockup's .lane, .lh and .lc.
    readonly property real labelWidthPx: 132
    readonly property real labelSidePx: 12
    readonly property real labelGapPx: 2
    readonly property real nameGapPx: 6
    readonly property real namePx: 9.5
    readonly property real valuePx: 21
    readonly property real subPx: 8.5
    readonly property real graphVerticalPx: 8
    readonly property real graphRightPx: 14
    readonly property real cellWPx: 4
    readonly property real barCellHPx: 3
    readonly property real thermCellHPx: 2
    readonly property real thermGapPx: 2
    // Line height of the value, in its font size.
    readonly property real valueLineHeight: 1.1
    // The mockup's chevrons are .8em of their line.
    readonly property real glyphEm: 0.8
    // Letter spacing in em.
    readonly property real nameTracking: 0.2
    readonly property real laneTagTracking: 0.04
    readonly property real subTracking: 0.06
    // The mockup's writeLabels(): each label scrambles for between 500 and 800 ms.
    readonly property real nameScrambleMinMs: 500
    readonly property real nameScrambleSpanMs: 300
    readonly property int thermGraphs: 3
    readonly property int kibPerGib: 1048576

    // The daemon's label for the CPU package temperature; sensors.labels maps another machine's chips onto it.
    readonly property string tctlLabel: "CPU TCTL"
    // History series, named as the daemon records them; the per-core ones end in the core's index in cpu.cores.
    readonly property string coreKeyPrefix: "core"
    readonly property string gpuLoadKey: "gpu_load"
    readonly property string memUsedKey: "mem_used"
    readonly property string netDownKey: "net_down"
    readonly property string netUpKey: "net_up"
    readonly property string diskReadKey: "disk_read"
    readonly property string diskWriteKey: "disk_write"
    readonly property string cpuTempKey: "temp_cpu"
    readonly property string gpuTempKey: "temp_gpu"
    readonly property string nvmeTempKey: "temp_nvme"

    property var settings

    readonly property string metric: settings.metric
    readonly property var lane: Lanes.LANES[metric]
    readonly property var lanes: Lanes.laneText(Fmt)
    readonly property bool isNet: metric === netMetric

    // Only the current metric's topics are read, so the others' messages leave this lane's text alone.
    readonly property var inputs: inputsFor(metric)
    readonly property bool valueLive: valueLiveFor(metric)
    readonly property var value: lanes.laneValue(metric, inputs)
    readonly property string valueText: isNet ? "" : value
    readonly property bool valueKnown: valueText !== Lanes.MISSING
    // A string, so a message that leaves the sub as it was does not rebuild its words.
    readonly property string subText: lanes.laneSub(metric, inputs, spanS)
    readonly property var subParts: Lanes.subParts(metric, subText)
    readonly property var spanS: metric === cpuMetric && graphLoader.item ? Lanes.spanSeconds(graphLoader.item.columnCount, Data.stepMs) : null
    // A line wider than the label block, such as a three-digit span or a network value in megabytes, shrinks to end
    // where the graph begins: it may use the block's right padding, as the mockup's unwrapped text does, but never
    // the graph.
    readonly property real fitWidth: Theme.d(labelWidthPx - labelSidePx)
    readonly property font valueFont: Qt.font({
        family: Theme.font,
        pixelSize: Theme.d(valuePx),
        weight: Font.Bold
    })
    readonly property font subFont: Qt.font({
        family: Theme.font,
        pixelSize: Theme.d(subPx),
        letterSpacing: Theme.d(subPx) * subTracking
    })

    function fitScale(lineWidth) {
        return lineWidth > fitWidth ? fitWidth / lineWidth : 1;
    }

    function last(series) {
        return series && series.length > 0 ? series[series.length - 1] : null;
    }

    function readingValue(label) {
        const found = Fmt.readingByLabel(Data.topic("sensors").value, label);
        return found ? found.value : null;
    }

    function gpuTemp() {
        const gpu = Data.topic("gpu").value;
        return gpu && gpu.present ? gpu.temp_c : null;
    }

    function inputsFor(name) {
        switch (name) {
        case cpuMetric:
            {
                const cpu = Data.topic("cpu").value;
                return {
                    cpu,
                    tctl: readingValue(tctlLabel),
                    cores: cpu ? cpu.cores.length : null
                };
            }
        case gpuMetric:
            return {
                gpu: Data.topic("gpu").value
            };
        case memMetric:
            return {
                mem: Data.topic("mem").value
            };
        case netMetric:
            return {
                net: Data.topic("net").value,
                links: Data.topic("net.links").value
            };
        case diskMetric:
            return {
                read: last(Data.history[diskReadKey]),
                write: last(Data.history[diskWriteKey])
            };
        case thermMetric:
            return {
                tctl: readingValue(tctlLabel),
                gpuT: gpuTemp()
            };
        default:
            return {};
        }
    }

    function valueLiveFor(name) {
        switch (name) {
        case cpuMetric:
            return Data.topic("cpu").live && Data.topic("sensors").live;
        case gpuMetric:
            return Data.topic("gpu").live;
        case memMetric:
            return Data.topic("mem").live;
        case netMetric:
            return Data.topic("net").live;
        case diskMetric:
            return Data.topic("disks").live;
        case thermMetric:
            return Data.topic("sensors").live && Data.topic("gpu").live;
        default:
            return false;
        }
    }

    anchors.fill: parent
    header: false
    padding: 0

    Column {
        id: labelBlock

        anchors {
            left: parent.left
            leftMargin: Theme.d(root.labelSidePx)
            verticalCenter: parent.verticalCenter
        }
        width: Theme.d(root.labelWidthPx - 2 * root.labelSidePx)
        spacing: Theme.d(root.labelGapPx)

        Row {
            transformOrigin: Item.Left
            scale: root.fitScale(implicitWidth)
            spacing: Theme.d(root.nameGapPx)

            ScrambleText {
                id: nameText

                anchors.verticalCenter: parent.verticalCenter
                text: root.lane ? root.lane.name : ""
                scrambleKey: "lane:" + text
                durationMs: root.nameScrambleMinMs + Math.random() * root.nameScrambleSpanMs
                color: Theme.hi
                font {
                    family: Theme.font
                    pixelSize: Theme.d(root.namePx)
                    weight: Font.DemiBold
                    letterSpacing: Theme.d(root.namePx) * root.nameTracking
                    capitalization: Font.AllUppercase
                }
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: text !== ""
                text: root.lane ? Theme.tag(root.lane.tag) : ""
                color: Theme.signal
                font {
                    family: Theme.fontCjk
                    pixelSize: Theme.d(root.namePx)
                    weight: Font.Medium
                    letterSpacing: Theme.d(root.namePx) * root.laneTagTracking
                }
            }
        }

        Item {
            width: parent.width
            height: Theme.d(root.valuePx * root.valueLineHeight)

            Digits {
                anchors.verticalCenter: parent.verticalCenter
                visible: !root.isNet
                transformOrigin: Item.Left
                scale: root.fitScale(implicitWidth)
                text: root.valueText
                pixelSize: Theme.d(root.valuePx)
                sourceLive: root.valueLive
                color: live && root.valueKnown ? Theme.hi : Theme.dim
            }

            InlineRow {
                anchors.verticalCenter: parent.verticalCenter
                visible: root.isNet
                transformOrigin: Item.Left
                scale: root.fitScale(implicitWidth)

                Glyph {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "chevron-down"
                    size: Theme.d(root.valuePx * root.glyphEm)
                    color: downDigits.color
                }

                Digits {
                    id: downDigits

                    text: root.isNet ? root.value.down : ""
                    pixelSize: Theme.d(root.valuePx)
                    sourceLive: root.valueLive
                }

                Text {
                    text: " "
                    font: root.valueFont
                }

                Glyph {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "chevron-up"
                    size: Theme.d(root.valuePx * root.glyphEm)
                    color: upDigits.color
                }

                Digits {
                    id: upDigits

                    text: root.isNet ? root.value.up : ""
                    pixelSize: Theme.d(root.valuePx)
                    sourceLive: root.valueLive
                }
            }
        }

        InlineRow {
            transformOrigin: Item.Left
            scale: root.fitScale(implicitWidth)

            Repeater {
                model: root.subParts

                InlineRow {
                    id: part

                    required property var modelData
                    required property int index

                    Text {
                        visible: part.index > 0
                        text: " · "
                        textFormat: Text.PlainText
                        color: Theme.dim
                        font: root.subFont
                    }

                    Glyph {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: part.modelData.glyph !== ""
                        // Glyph warns about a name it lacks even while hidden.
                        name: part.modelData.glyph !== "" ? part.modelData.glyph : "chevron-down"
                        size: Theme.d(root.subPx * root.glyphEm)
                        color: Theme.dim
                    }

                    Text {
                        text: (part.modelData.glyph !== "" ? " " : "") + part.modelData.text
                        textFormat: Text.PlainText
                        color: Theme.dim
                        font: root.subFont
                    }
                }
            }
        }
    }

    Loader {
        id: graphLoader

        anchors {
            fill: parent
            leftMargin: Theme.d(root.labelWidthPx)
            rightMargin: Theme.d(root.graphRightPx)
            topMargin: Theme.d(root.graphVerticalPx)
            bottomMargin: Theme.d(root.graphVerticalPx)
        }
        sourceComponent: {
            switch (root.metric) {
            case root.cpuMetric:
                return heatLane;
            case root.gpuMetric:
                return gpuLane;
            case root.memMetric:
                return memLane;
            case root.netMetric:
                return netLane;
            case root.diskMetric:
                return diskLane;
            case root.thermMetric:
                return thermLane;
            default:
                return null;
            }
        }
    }

    Component {
        id: heatLane

        HeatGraph {
            readonly property var cpu: Data.topic("cpu").value
            readonly property int coreCount: cpu ? cpu.cores.length : 0

            rows: Array.from({
                length: coreCount
            }, (_, core) => Data.history[root.coreKeyPrefix + core] || [])
            sourceLive: Data.topic("cpu").live
        }
    }

    Component {
        id: gpuLane

        BlockGraph {
            values: Data.history[root.gpuLoadKey] || []
            cellW: root.cellWPx
            cellH: root.barCellHPx
            minLit: 1
            sourceLive: Data.topic("gpu").live
        }
    }

    Component {
        id: memLane

        BlockGraph {
            readonly property var mem: Data.topic("mem").value

            // Until the total arrives there is no scale to draw the samples on.
            values: mem ? Data.history[root.memUsedKey] || [] : []
            hi: mem ? mem.total_kib / root.kibPerGib : 1
            cellW: root.cellWPx
            cellH: root.barCellHPx
            minLit: 0
            sourceLive: Data.topic("mem").live
        }
    }

    Component {
        id: netLane

        MirrorGraph {
            up: Data.history[root.netUpKey] || []
            down: Data.history[root.netDownKey] || []
            floor: Lanes.NET_FLOOR_BPS
            sourceLive: Data.topic("net").live
        }
    }

    Component {
        id: diskLane

        MirrorGraph {
            up: Data.history[root.diskWriteKey] || []
            down: Data.history[root.diskReadKey] || []
            floor: Lanes.DISK_FLOOR_BPS
            sourceLive: Data.topic("disks").live
        }
    }

    Component {
        id: thermLane

        Column {
            id: therm

            readonly property real graphHeight: (height - (root.thermGraphs - 1) * spacing) / root.thermGraphs
            readonly property real tempSpan: Lanes.THERM_HI_C - Lanes.THERM_LO_C

            spacing: Theme.d(root.thermGapPx)

            Repeater {
                model: [
                    {
                        key: root.cpuTempKey,
                        topic: "sensors"
                    },
                    {
                        key: root.gpuTempKey,
                        topic: "gpu"
                    },
                    {
                        key: root.nvmeTempKey,
                        topic: "disks"
                    }
                ]

                BlockGraph {
                    required property var modelData

                    width: therm.width
                    height: therm.graphHeight
                    values: Data.history[modelData.key] || []
                    lo: Lanes.THERM_LO_C
                    hi: Lanes.THERM_HI_C
                    warnAt: (Theme.tempWarn - Lanes.THERM_LO_C) / therm.tempSpan
                    critAt: (Theme.tempCrit - Lanes.THERM_LO_C) / therm.tempSpan
                    cellW: root.cellWPx
                    cellH: root.thermCellHPx
                    minLit: 1
                    sourceLive: Data.topic(modelData.topic).live
                }
            }
        }
    }
}
