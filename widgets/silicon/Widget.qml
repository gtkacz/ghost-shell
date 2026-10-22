import QtQuick
import QtQuick.Shapes
import GhostKit
import "silicon.mjs" as Sil

// A schematic of the CPU package after the reference mockup's drawDie: its CCDs with a tile per core and the shared
// L3, the I/O die's blocks, and a packet on each Infinity Fabric link whose speed follows its CCD's load. Only the
// packets move between samples; the chamfered outlines and the links are one static Shape.
Panel {
    id: root

    // Opacities, from the mockup's rgba('sig', …).
    readonly property real packageOpacity: 0.25
    readonly property real outlineOpacity: 0.45
    readonly property real l3FillOpacity: 0.06
    readonly property real hatchOpacity: 0.35
    readonly property real tileOpacity: 0.3
    readonly property real linkOpacity: 0.35
    readonly property real blockOpacity: 0.25
    // Design pixels, from the mockup's drawDie.
    readonly property real strokePx: 1
    readonly property real packetPx: 3
    // The mockup's vbar() cells.
    readonly property real threadCellPx: 2
    readonly property real threadGapPx: 1
    readonly property var linkDash: [2, 3]
    readonly property real titleGapPx: 8
    // The mockup's fonts: fs + 1 for the CCD and I/O-die heads, fs - 1 for the small labels.
    readonly property real headStepPx: 1

    // The daemon's labels for these readings; sensors.labels maps another machine's chips onto them.
    readonly property string tctlLabel: "CPU TCTL"
    readonly property string ccdTempLabelPrefix: "CCD"

    property var settings

    readonly property var cpu: Data.topic("cpu").value
    readonly property var sensors: Data.topic("sensors").value
    readonly property var mem: Data.topic("mem").value
    readonly property var gpu: Data.topic("gpu").value
    readonly property var disks: Data.topic("disks").value
    readonly property bool live: Data.connected && !Data.stale

    readonly property var tiles: Sil.ccdTiles(cpu, Theme.loadWarn, Theme.loadCrit)
    readonly property var blocks: Sil.iodBlocks(mem, gpu, disks, cpu, Fmt)
    // A string, so that a new sample with the same topology changes nothing and the layout is not redone each tick.
    readonly property string coreCounts: tiles.map(ccd => ccd.length).join(",")
    readonly property var freqMhz: cpu && cpu.freq_mhz > 0 ? cpu.freq_mhz : null
    readonly property var tctl: readingValue(tctlLabel)

    readonly property bool wantsClock: visible && !Data.covered
    property bool holdsClock: false
    property var die: null

    function readingValue(label) {
        const found = Fmt.readingByLabel(sensors, label);
        return found ? found.value : null;
    }

    function signal(opacity) {
        return Qt.rgba(Theme.signal.r, Theme.signal.g, Theme.signal.b, opacity);
    }

    // Colour only for attention, and none for a reading that may be stale.
    function tileColor(level) {
        if (!live || level === "ok") {
            return signal(tileOpacity);
        }
        return level === "crit" ? Theme.crit : Theme.warn;
    }

    function tempColor(value) {
        if (!live || !Number.isFinite(value)) {
            return Theme.dim;
        }
        return value >= Theme.tempCrit ? Theme.crit : value >= Theme.tempWarn ? Theme.warn : Theme.signal;
    }

    function relayout() {
        const w = area.width / Theme.u;
        const h = area.height / Theme.u;
        if (coreCounts === "" || !(w > 0) || !(h > 0)) {
            die = null;
            return;
        }
        die = Sil.dieLayout(w, h, coreCounts.split(",").map(Number));
    }

    // Held only while the schematic is on screen and uncovered, so the clock stops when no packet can be seen.
    function holdClock() {
        if (wantsClock === holdsClock) {
            return;
        }
        if (wantsClock) {
            FrameClock.acquire();
        } else {
            FrameClock.release();
        }
        holdsClock = wantsClock;
    }

    anchors.fill: parent
    label: "Silicon"
    tagKey: "die"
    meta: Sil.headerMeta(cpu, gpu)

    onCoreCountsChanged: Qt.callLater(relayout)
    // A hold changes a turn later, never inside the update of the binding that asked for it.
    onWantsClockChanged: Qt.callLater(holdClock)
    Component.onCompleted: holdClock()
    // Directly: a call left for later is dropped once the widget is gone, and its hold would stay.
    Component.onDestruction: {
        if (holdsClock) {
            FrameClock.release();
            holdsClock = false;
        }
    }

    component DieText: Text {
        property real px
        property real baselineAt
        property bool bold: false

        y: Theme.d(baselineAt) - baselineOffset
        textFormat: Text.PlainText
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: Theme.d(px)
            weight: bold ? Font.Bold : Font.DemiBold
        }
    }

    Item {
        id: area

        readonly property real stroke: Theme.d(root.strokePx)

        anchors.fill: parent

        onWidthChanged: Qt.callLater(root.relayout)
        onHeightChanged: Qt.callLater(root.relayout)

        Shape {
            anchors.fill: parent
            visible: root.die !== null
            // The default renderer leaves the 45° cuts without antialiasing.
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: "transparent"
                strokeColor: root.signal(root.packageOpacity)
                strokeWidth: area.stroke

                PathSvg {
                    path: root.die ? Sil.chamferPath(root.die.package, Sil.PACKAGE_CHAMFER_PX, Theme.u) : ""
                }
            }

            ShapePath {
                fillColor: "transparent"
                strokeColor: root.signal(root.outlineOpacity)
                strokeWidth: area.stroke

                PathSvg {
                    path: root.die ? root.die.ccds.concat([root.die.iod]).map(rect => Sil.chamferPath(rect, Sil.BLOCK_CHAMFER_PX, Theme.u)).join(" ") : ""
                }
            }

            ShapePath {
                fillColor: "transparent"
                strokeColor: root.signal(root.linkOpacity)
                strokeWidth: area.stroke
                strokeStyle: ShapePath.DashLine
                dashPattern: root.linkDash
                capStyle: ShapePath.FlatCap

                PathSvg {
                    path: root.die ? root.die.ccds.map(ccd => "M " + Theme.d(ccd.link.x) + " " + Theme.d(ccd.link.y0) + " L " + Theme.d(ccd.link.x) + " " + Theme.d(ccd.link.y1)).join(" ") : ""
                }
            }
        }

        Item {
            anchors.fill: parent
            visible: root.die !== null

            DieText {
                id: titleText

                x: Theme.d(root.die ? root.die.titleX : 0)
                width: Math.max(0, Theme.d(root.die ? root.die.titleRight : 0) - x - clockText.implicitWidth - Theme.d(root.titleGapPx))
                px: root.die ? root.die.fs : 0
                baselineAt: root.die ? root.die.titleBaseline : 0
                text: Sil.titleLine(root.cpu, root.die ? root.die.small : false)
                color: Theme.dim
            }

            DieText {
                id: clockText

                x: Theme.d(root.die ? root.die.titleRight : 0) - implicitWidth
                px: root.die ? root.die.fs : 0
                baselineAt: root.die ? root.die.titleBaseline : 0
                text: "TCTL " + Fmt.tempC(root.tctl) + " · " + Fmt.ghz(root.freqMhz)
                color: root.live ? Theme.signal : Theme.dim
            }

            Repeater {
                model: root.die ? root.die.ccds.length : 0

                Item {
                    id: ccdItem

                    required property int index
                    readonly property var geometry: root.die.ccds[index]
                    readonly property var ccd: root.cpu && index < root.cpu.ccds.length ? root.cpu.ccds[index] : null
                    readonly property var temp: ccd ? root.readingValue(root.ccdTempLabelPrefix + ccd.id) : null

                    anchors.fill: parent

                    Rectangle {
                        id: l3Strip

                        x: Theme.d(ccdItem.geometry.l3.x)
                        y: Theme.d(ccdItem.geometry.l3.y)
                        width: Theme.d(ccdItem.geometry.l3.w)
                        height: Theme.d(ccdItem.geometry.l3.h)
                        color: root.signal(root.l3FillOpacity)
                        clip: true

                        Shape {
                            anchors.fill: parent
                            visible: ccdItem.ccd !== null && ccdItem.ccd.vcache
                            preferredRendererType: Shape.CurveRenderer

                            ShapePath {
                                fillColor: "transparent"
                                strokeColor: root.signal(root.hatchOpacity)
                                strokeWidth: area.stroke

                                PathSvg {
                                    path: Sil.hatchPath(ccdItem.geometry.l3.w, ccdItem.geometry.l3.h, Theme.u)
                                }
                            }
                        }
                    }

                    DieText {
                        x: Theme.d(ccdItem.geometry.headX)
                        width: Math.max(0, Theme.d(ccdItem.geometry.headRight) - x - tempText.implicitWidth)
                        px: root.die.fs + root.headStepPx
                        bold: true
                        baselineAt: ccdItem.geometry.headBaseline
                        text: ccdItem.ccd ? Fmt.ccdTitle(ccdItem.ccd) : ""
                        color: Theme.hi
                    }

                    DieText {
                        id: tempText

                        x: Theme.d(ccdItem.geometry.headRight) - implicitWidth
                        px: root.die.fs + root.headStepPx
                        bold: true
                        baselineAt: ccdItem.geometry.headBaseline
                        text: Fmt.tempC(ccdItem.temp)
                        color: root.tempColor(ccdItem.temp)
                    }

                    DieText {
                        x: l3Strip.x + Math.max(0, (l3Strip.width - implicitWidth) / 2)
                        width: Math.min(implicitWidth, l3Strip.width)
                        px: root.die.fs - root.headStepPx
                        baselineAt: ccdItem.geometry.l3Baseline
                        text: ccdItem.ccd && root.cpu ? Sil.l3Text(root.cpu.ccds, ccdItem.ccd, root.die.small) : ""
                        color: Theme.hi
                    }

                    Repeater {
                        model: ccdItem.geometry.tiles.length

                        Item {
                            id: tileItem

                            required property int index
                            readonly property var geometry: ccdItem.geometry.tiles[index]
                            readonly property var tile: ccdItem.index < root.tiles.length && index < root.tiles[ccdItem.index].length ? root.tiles[ccdItem.index][index] : null

                            x: Theme.d(geometry.x)
                            y: Theme.d(geometry.y)
                            width: Theme.d(geometry.w)
                            height: Theme.d(geometry.h)

                            Rectangle {
                                anchors.fill: parent
                                color: "transparent"
                                border {
                                    width: area.stroke
                                    color: root.tileColor(tileItem.tile ? tileItem.tile.level : "ok")
                                }
                            }

                            DieText {
                                x: Theme.d(tileItem.geometry.labelX - tileItem.geometry.x)
                                px: root.die.fs - root.headStepPx
                                baselineAt: tileItem.geometry.labelBaseline - tileItem.geometry.y
                                text: tileItem.tile ? tileItem.tile.name : ""
                                color: Theme.dim
                            }

                            BlockGraph {
                                x: Theme.d(tileItem.geometry.graph.x - tileItem.geometry.x)
                                y: Theme.d(tileItem.geometry.graph.y - tileItem.geometry.y)
                                width: Theme.d(tileItem.geometry.graph.w)
                                height: Theme.d(tileItem.geometry.graph.h)
                                values: tileItem.tile ? tileItem.tile.loads : []
                                cols: tileItem.tile ? tileItem.tile.loads.length : 0
                                cellH: root.threadCellPx
                                gap: root.threadGapPx
                            }
                        }
                    }

                    DieText {
                        visible: !root.die.small
                        x: Theme.d(ccdItem.geometry.link.x + root.die.linkLabelOffset)
                        px: root.die.fs - root.headStepPx
                        baselineAt: (ccdItem.geometry.link.y0 + ccdItem.geometry.link.y1) / 2 + root.die.linkLabelDrop
                        text: "IF"
                        color: Theme.dim
                    }

                    Rectangle {
                        readonly property real phase: Sil.packetPhase(FrameClock.elapsedMs, ccdItem.ccd ? ccdItem.ccd.load : 0, ccdItem.index)
                        readonly property var link: ccdItem.geometry.link

                        x: Theme.d(link.x - root.packetPx / 2)
                        y: Theme.d(link.y1 - (link.y1 - link.y0) * phase - root.packetPx / 2)
                        width: Theme.d(root.packetPx)
                        height: width
                        color: Theme.hi
                    }
                }
            }

            DieText {
                x: Theme.d(root.die ? root.die.iodHeadX : 0)
                px: root.die ? root.die.fs + root.headStepPx : 0
                bold: true
                baselineAt: root.die ? root.die.iodBaseline : 0
                text: "IOD"
                color: Theme.hi
            }

            Repeater {
                model: root.die ? root.die.blocks.length : 0

                Item {
                    id: blockItem

                    required property int index
                    readonly property var geometry: root.die.blocks[index]
                    readonly property var block: root.blocks[index]

                    x: Theme.d(geometry.x)
                    y: Theme.d(geometry.y)
                    width: Theme.d(geometry.w)
                    height: Theme.d(geometry.h)

                    Rectangle {
                        anchors.fill: parent
                        color: "transparent"
                        border {
                            width: area.stroke
                            color: root.signal(root.blockOpacity)
                        }
                    }

                    DieText {
                        x: Theme.d(blockItem.geometry.textX - blockItem.geometry.x)
                        width: Math.min(implicitWidth, parent.width - 2 * x)
                        px: root.die.fs - root.headStepPx
                        baselineAt: blockItem.geometry.nameBaseline - blockItem.geometry.y
                        text: blockItem.block.name
                        color: Theme.signal
                    }

                    DieText {
                        visible: blockItem.geometry.showsValue
                        x: Theme.d(blockItem.geometry.textX - blockItem.geometry.x)
                        width: Math.min(implicitWidth, parent.width - 2 * x)
                        px: root.die.fs - root.headStepPx
                        baselineAt: blockItem.geometry.valueBaseline - blockItem.geometry.y
                        text: blockItem.block.value
                        color: root.live ? Theme.hi : Theme.dim
                    }

                    SegMeter {
                        visible: blockItem.geometry.showsMeter
                        x: Theme.d(blockItem.geometry.meter.x - blockItem.geometry.x)
                        y: Theme.d(blockItem.geometry.meter.y - blockItem.geometry.y)
                        width: Theme.d(blockItem.geometry.meter.w)
                        height: Theme.d(blockItem.geometry.meter.h)
                        value: blockItem.block.fraction
                        warnAt: blockItem.block.alarms ? Theme.loadWarn : Infinity
                        critAt: blockItem.block.alarms ? Theme.loadCrit : Infinity
                    }
                }
            }
        }
    }
}
