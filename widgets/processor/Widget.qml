import QtQuick
import GhostKit

// Per-CCD thread loads with their temperatures, the load history, and the headline readings.
Panel {
    id: root

    // Design pixels, from the reference mockup's Processor panel.
    readonly property real stackGapPx: 8
    readonly property real groupGapPx: 12
    readonly property real groupTitlePx: 8.5
    readonly property real groupTitleGapPx: 3
    readonly property real threadCellPx: 3
    readonly property real threadGapPx: 2
    readonly property real historyPx: 40
    readonly property real kvGapPx: 10
    // Letter spacing in em.
    readonly property real groupTitleTracking: 0.14
    readonly property int kvCount: 4

    readonly property string tctlId: "k10temp/Tctl"
    // k10temp numbers its CCD sensors from 1, so CCD n reads Tccd(n + 1).
    readonly property string ccdTempPrefix: "k10temp/Tccd"
    readonly property string cpuFanId: "nct6799/fan2"

    property var settings

    readonly property var cpu: Data.topics.cpu || null
    readonly property var sensors: Data.topics.sensors || null
    readonly property bool live: Data.connected && !Data.stale
    readonly property var ccds: cpu ? cpu.ccds : []

    function readingValue(id) {
        const found = Fmt.reading(sensors, id);
        return found ? found.value : null;
    }

    // Missing and stale readings are dim, so "--" never reads as a value.
    function tone(value, liveColor) {
        return live && Number.isFinite(value) ? liveColor : Theme.dim;
    }

    anchors.fill: parent
    label: "Processor"
    tagKey: "cpu"
    meta: cpu ? Fmt.cpuModelShort(cpu.model) : "--"

    Item {
        id: groups

        readonly property real gap: Theme.d(root.groupGapPx)
        readonly property real groupWidth: (width - (root.ccds.length - 1) * gap) / Math.max(1, root.ccds.length)

        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: history.top
            bottomMargin: Theme.d(root.stackGapPx)
        }

        Repeater {
            model: root.ccds.length

            Item {
                id: group

                required property int index
                readonly property var ccd: root.ccds[index]
                readonly property var temp: root.readingValue(root.ccdTempPrefix + (ccd.id + 1))

                x: index * (groups.groupWidth + groups.gap)
                width: groups.groupWidth
                height: groups.height

                Text {
                    id: groupTitle

                    anchors {
                        left: parent.left
                        right: groupTemp.left
                    }
                    text: Fmt.ccdTitle(group.ccd)
                    color: Theme.dim
                    elide: Text.ElideRight
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.groupTitlePx)
                        weight: Font.Medium
                        letterSpacing: Theme.d(root.groupTitlePx) * root.groupTitleTracking
                    }
                }

                Digits {
                    id: groupTemp

                    anchors {
                        right: parent.right
                        verticalCenter: groupTitle.verticalCenter
                    }
                    text: Fmt.tempShort(group.temp)
                    pixelSize: Theme.d(root.groupTitlePx)
                    weight: Font.Medium
                    color: root.tone(group.temp, Theme.text)
                }

                BlockGraph {
                    anchors {
                        top: groupTitle.bottom
                        topMargin: Theme.d(root.groupTitleGapPx)
                        left: parent.left
                        right: parent.right
                        bottom: parent.bottom
                    }
                    values: group.ccd.cpus.map(thread => root.cpu.threads[thread])
                    cols: group.ccd.cpus.length
                    cellH: root.threadCellPx
                    gap: root.threadGapPx
                }
            }
        }
    }

    BlockGraph {
        id: history

        anchors {
            left: parent.left
            right: parent.right
            bottom: readings.top
            bottomMargin: Theme.d(root.stackGapPx)
        }
        height: Theme.d(root.historyPx)
        values: Data.history.load || []
    }

    Row {
        id: readings

        readonly property real cellWidth: (width - (root.kvCount - 1) * spacing) / root.kvCount
        readonly property var tctl: root.readingValue(root.tctlId)
        readonly property var fan: root.readingValue(root.cpuFanId)

        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        spacing: Theme.d(root.kvGapPx)

        Kv {
            width: readings.cellWidth
            key: "LOAD"
            value: Fmt.pct(root.cpu ? root.cpu.load : null)
            valueColor: root.tone(root.cpu ? root.cpu.load : null, Theme.hi)
        }

        Kv {
            width: readings.cellWidth
            key: "TCTL"
            value: Fmt.tempC(readings.tctl)
            valueColor: root.tone(readings.tctl, Theme.hi)
        }

        Kv {
            width: readings.cellWidth
            key: "FREQ"
            value: Fmt.ghz(root.cpu ? root.cpu.freq_mhz : null)
            valueColor: root.tone(root.cpu ? root.cpu.freq_mhz : null, Theme.hi)
        }

        Kv {
            width: readings.cellWidth
            key: "CPU FAN"
            value: Fmt.rpm(readings.fan)
            valueColor: root.tone(readings.fan, Theme.hi)
        }
    }
}
