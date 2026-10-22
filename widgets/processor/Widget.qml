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
    readonly property int baseKvCount: 4

    // The daemon's labels for these readings; sensors.labels maps another machine's chips onto them.
    readonly property string tctlLabel: "CPU TCTL"
    readonly property string ccdTempLabelPrefix: "CCD"
    readonly property string cpuFanLabel: "CPU FAN"

    property var settings

    readonly property var cpu: Data.topic("cpu").value
    readonly property var sensors: Data.topic("sensors").value
    readonly property bool live: Data.connected && !Data.stale
    readonly property var ccds: cpu ? cpu.ccds : []
    // The daemon reports 0 MHz without cpufreq and an empty thread list when it has no CPU data;
    // neither is a reading, so both stay missing instead of showing as zero.
    readonly property var loadPct: cpu && cpu.threads.length > 0 ? cpu.load : null
    readonly property var freqMhz: cpu && cpu.freq_mhz > 0 ? cpu.freq_mhz : null
    // Null while the RAPL counter is unreadable, in which case the PKG cell is left out entirely.
    readonly property var packageW: cpu && Number.isFinite(cpu.package_w) ? cpu.package_w : null
    readonly property int kvCount: baseKvCount + (packageW === null ? 0 : 1)

    function readingValue(label) {
        const found = Fmt.readingByLabel(sensors, label);
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
                readonly property var temp: root.readingValue(root.ccdTempLabelPrefix + ccd.id)

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
        readonly property var tctl: root.readingValue(root.tctlLabel)
        readonly property var fan: root.readingValue(root.cpuFanLabel)

        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        spacing: Theme.d(root.kvGapPx)

        Kv {
            width: readings.cellWidth
            key: "LOAD"
            value: Fmt.pct(root.loadPct)
            valueColor: root.tone(root.loadPct, Theme.hi)
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
            value: Fmt.ghz(root.freqMhz)
            valueColor: root.tone(root.freqMhz, Theme.hi)
        }

        Kv {
            visible: root.packageW !== null
            width: readings.cellWidth
            key: "PKG"
            value: Fmt.watts(root.packageW)
            valueColor: root.tone(root.packageW, Theme.hi)
        }

        Kv {
            width: readings.cellWidth
            key: "CPU FAN"
            value: Fmt.rpm(readings.fan)
            valueColor: root.tone(readings.fan, Theme.hi)
        }
    }
}
