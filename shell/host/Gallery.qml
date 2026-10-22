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

    // GHOST_GALLERY_LANES swaps this page for MONITOR-sized graph lanes: `mixed` and `heat` move once a second, to
    // be measured; `fixed` stands still and writes each graph's grab to GHOST_GALLERY_GRAB_DIR, for pixel checks.
    readonly property var lanePages: ({
            mixed: ["block", "block", "mirror", "mirror", "heat", "heat"],
            heat: ["heat", "heat", "heat", "heat", "heat", "heat"],
            fixed: ["block", "mirror", "heat"]
        })
    readonly property string lanesPage: Quickshell.env("GHOST_GALLERY_LANES") || ""
    readonly property bool showsLanes: Object.prototype.hasOwnProperty.call(lanePages, lanesPage)
    readonly property bool movingLanes: showsLanes && lanesPage !== "fixed"
    readonly property string grabDir: Quickshell.env("GHOST_GALLERY_GRAB_DIR") || ""
    // Design pixels: the reference mockup's MONITOR lanes, less each lane's 8 px graph padding above and below,
    // and its lane cell.
    readonly property real laneWidthPx: 1256
    readonly property real laneHeightPx: 70
    readonly property real laneGapPx: 6
    readonly property real laneCellWPx: 4
    readonly property real laneCellHPx: 3
    // The mockup's CPU lane has a row per core of a 16-core CPU, and its network lane a floor of 50.
    readonly property int heatRows: 16
    readonly property real mirrorFloor: 50
    // Five minutes at 1 Hz, as Data.history keeps.
    readonly property int laneSamples: 300
    readonly property int laneTickMs: 1000
    // BlockGraph's spike series: a gap every 37 samples, and a prime step that visits every value of 0..100 within
    // 101 samples, so each lane shows every level, gaps, and empty and full columns.
    readonly property int laneMissingEvery: 37
    readonly property int laneValueStep: 7919
    readonly property int laneValueRange: 101
    // The series repeats after the least common multiple of the two, so one period and a lane's length cover every
    // window of it.
    readonly property int lanePeriod: laneMissingEvery * laneValueRange
    readonly property var laneTable: showsLanes ? Array.from({
            length: lanePeriod + laneSamples
        }, (_, at) => at % laneMissingEvery === 0 ? null : (at * laneValueStep) % laneValueRange) : []
    // Samples between the series of neighbouring lanes, of a mirror's two halves and of a heat graph's rows.
    readonly property int laneSeriesStride: 61
    readonly property int mirrorDownOffset: 29
    readonly property int heatRowStride: 19
    // Long enough for the first frame with live data to have rendered.
    readonly property int grabDelayMs: 1500
    readonly property bool lanesReady: showsLanes && Data.connected && !Data.stale
    property int laneTick: 0

    // GHOST_GALLERY_GLOBE=1 swaps this page for the uplink globe at the body sizes of the V2 centre and the V1 scope
    // (the globe spike's estimate), holding the frame clock while it is shown. With GHOST_GALLERY_GRAB_DIR set, each
    // globe is grabbed twice, 2 s apart.
    readonly property bool showsGlobe: Quickshell.env("GHOST_GALLERY_GLOBE") === "1"
    readonly property bool showsKit: !showsLanes && !showsGlobe
    readonly property real globeV2WidthPx: 608
    readonly property real globeV2HeightPx: 330
    readonly property real globeV1WidthPx: 276
    readonly property real globeV1HeightPx: 146
    // The fake daemon's uplink: the reference mockup's six PEERS, and São Paulo home.
    readonly property var globeEndpoints: [
        {
            lat: 39.04,
            lon: -77.49
        },
        {
            lat: 59.33,
            lon: 18.07
        },
        {
            lat: 47.61,
            lon: -122.33
        },
        {
            lat: 50.11,
            lon: 8.68
        },
        {
            lat: 37.77,
            lon: -122.42
        },
        {
            lat: -22.9,
            lon: -43.2
        }
    ]
    readonly property real homeLat: -23.55
    readonly property real homeLon: -46.63
    readonly property int globeGrabGapMs: 2000
    readonly property int globeGrabRounds: 2

    // The window passes every click through except these, the way the layout view does for its tab strips.
    readonly property var inputRects: showsKit ? [
        {
            x: Math.floor(fastToggle.x),
            y: Math.floor(fastToggle.y),
            width: Math.ceil(fastToggle.x + fastToggle.width) - Math.floor(fastToggle.x),
            height: Math.ceil(fastToggle.y + fastToggle.height) - Math.floor(fastToggle.y)
        }
    ] : []

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

    // The series `offset` samples into BlockGraph's spike series, moved on by one sample per tick. A slice of one
    // period of it, so that feeding the lanes costs next to nothing beside drawing them.
    function laneSeries(offset) {
        const start = (offset + laneTick) % lanePeriod;
        return laneTable.slice(start, start + laneSamples);
    }

    function heatSeries(offset) {
        return Array.from({
            length: heatRows
        }, (_, row) => laneSeries(offset + row * heatRowStride));
    }

    // Once the data is live, so the creation time includes packing every lit cell, as a real view's would.
    function createLanes() {
        if (lanesLoader.active || !lanesReady) {
            return;
        }
        const started = Date.now();
        lanesLoader.active = true;
        const ratio = QsWindow.window ? QsWindow.window.devicePixelRatio : 1;
        console.info("gallery lanes page " + lanesPage + " created in " + (Date.now() - started) + " ms at u " + Theme.u + ", pixel ratio " + ratio);
    }

    onFastHeldChanged: holdFast(fastHeld)
    onLanesReadyChanged: Qt.callLater(createLanes)
    // The lane pages draw only on their own tick, so a measurement counts the lanes' frames alone.
    Component.onCompleted: {
        holdFast(fastHeld);
        if (showsKit) {
            FrameClock.acquireSeconds();
        }
        if (lanesPage !== "" && !showsLanes) {
            console.warn("GHOST_GALLERY_LANES must be one of " + Object.keys(lanePages).join(", ") + ", got '" + lanesPage + "'; showing the kit");
        }
        Qt.callLater(createLanes);
    }
    Component.onDestruction: {
        holdFast(false);
        if (showsKit) {
            FrameClock.releaseSeconds();
        }
    }

    Row {
        visible: root.showsKit
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
        visible: root.showsKit
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
        visible: root.showsKit
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

        visible: root.showsKit
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

    // Inactive until createLanes(), which times its creation.
    Loader {
        id: lanesLoader

        x: Theme.d(root.marginPx)
        y: Theme.d(root.marginPx + root.barBandPx)
        active: false
        sourceComponent: Column {
            id: lanes

            property bool grabbed: false

            // At the screen's own scale in both directions: a grab without a size can cut a row of pixels and scale
            // the rest unevenly (blockgraph shader spike).
            function grabLanes() {
                grabbed = true;
                if (root.grabDir === "") {
                    console.warn("GHOST_GALLERY_GRAB_DIR is not set; the fixed lanes are not grabbed");
                    return;
                }
                const ratio = QsWindow.window ? QsWindow.window.devicePixelRatio : 1;
                for (let i = 0; i < laneRepeater.count; i += 1) {
                    const graph = laneRepeater.itemAt(i).item;
                    const path = root.grabDir + "/" + i + "-" + root.lanePages[root.lanesPage][i] + ".png";
                    const size = Qt.size(Math.round(graph.width * ratio) / ratio, Math.round(graph.height * ratio) / ratio);
                    graph.grabToImage(result => {
                        if (result.saveToFile(path)) {
                            console.info("gallery grab saved " + path);
                        } else {
                            console.warn("gallery grab not written to " + path);
                        }
                    }, size);
                }
            }

            spacing: Theme.d(root.laneGapPx)

            Repeater {
                id: laneRepeater

                model: root.lanePages[root.lanesPage]

                Loader {
                    id: lane

                    required property string modelData
                    required property int index
                    readonly property int offset: index * root.laneSeriesStride

                    width: Theme.d(root.laneWidthPx)
                    height: Theme.d(root.laneHeightPx)
                    sourceComponent: modelData === "block" ? blockLane : modelData === "mirror" ? mirrorLane : heatLane

                    Component {
                        id: blockLane

                        BlockGraph {
                            values: root.laneSeries(lane.offset)
                            cellW: root.laneCellWPx
                            cellH: root.laneCellHPx
                        }
                    }

                    Component {
                        id: mirrorLane

                        MirrorGraph {
                            up: root.laneSeries(lane.offset)
                            down: root.laneSeries(lane.offset + root.mirrorDownOffset)
                            floor: root.mirrorFloor
                        }
                    }

                    Component {
                        id: heatLane

                        HeatGraph {
                            rows: root.heatSeries(lane.offset)
                        }
                    }
                }
            }

            Timer {
                interval: root.laneTickMs
                repeat: true
                running: root.movingLanes && lanes.visible
                onTriggered: root.laneTick += 1
            }

            Timer {
                interval: root.grabDelayMs
                running: !root.movingLanes && root.lanesReady && !lanes.grabbed
                onTriggered: lanes.grabLanes()
            }
        }
    }

    Loader {
        x: Theme.d(root.marginPx)
        y: Theme.d(root.marginPx + root.barBandPx)
        active: root.showsGlobe
        sourceComponent: Row {
            id: globes

            readonly property bool wantsClock: visible
            property bool holdsClock: false
            property int grabs: 0

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

            // At the screen's own scale in both directions, as the lanes are grabbed.
            function grabGlobes() {
                const ratio = QsWindow.window ? QsWindow.window.devicePixelRatio : 1;
                const round = grabs;
                grabs += 1;
                for (const [globe, name] of [[v2Globe, "v2"], [v1Globe, "v1"]]) {
                    const path = root.grabDir + "/globe-" + name + "-" + round + ".png";
                    const size = Qt.size(Math.round(globe.width * ratio) / ratio, Math.round(globe.height * ratio) / ratio);
                    const elapsedMs = globe.elapsedMs;
                    globe.grabToImage(result => {
                        if (result.saveToFile(path)) {
                            console.info("gallery grab saved " + path + " at " + elapsedMs + " ms");
                        } else {
                            console.warn("gallery grab not written to " + path);
                        }
                    }, size);
                }
            }

            spacing: Theme.d(root.gutterPx)

            // A hold changes a turn later, never inside the update of the binding that asked for it.
            onWantsClockChanged: Qt.callLater(holdClock)
            Component.onCompleted: holdClock()
            // Directly: a call left for later is dropped once the page is gone, and its hold would stay.
            Component.onDestruction: {
                if (holdsClock) {
                    FrameClock.release();
                    holdsClock = false;
                }
            }

            Globe {
                id: v2Globe

                width: Theme.d(root.globeV2WidthPx)
                height: Theme.d(root.globeV2HeightPx)
                elapsedMs: FrameClock.elapsedMs
                hasHome: true
                homeLat: root.homeLat
                homeLon: root.homeLon
                endpoints: root.globeEndpoints
            }

            Globe {
                id: v1Globe

                width: Theme.d(root.globeV1WidthPx)
                height: Theme.d(root.globeV1HeightPx)
                elapsedMs: FrameClock.elapsedMs
                hasHome: true
                homeLat: root.homeLat
                homeLon: root.homeLon
                endpoints: root.globeEndpoints
            }

            Timer {
                interval: root.globeGrabGapMs
                repeat: true
                running: root.grabDir !== "" && globes.grabs < root.globeGrabRounds
                onTriggered: globes.grabGlobes()
            }
        }
    }
}
