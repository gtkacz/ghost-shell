import QtQuick
import Quickshell
import Quickshell.Wayland
import GhostKit
import "kit/GhostKit/lib/scale.mjs" as Scale

// Six full-width history lanes, drawn by the current BlockGraph (SPIKE_MODE=rect) or by ShaderBlockGraph
// (shader), fed by Data's 1 Hz history; `none` subscribes the same way and draws no lane, as a baseline;
// `compare` draws both kinds from one fixed series and from the live history, and saves each to SPIKE_GRAB_DIR.
ShellRoot {
    id: root

    readonly property var modes: ["none", "rect", "shader", "compare"]
    readonly property string mode: Quickshell.env("SPIKE_MODE") ?? "none"
    readonly property string grabDir: Quickshell.env("SPIKE_GRAB_DIR") ?? ""
    // Design pixels, as "<cell width>x<cell height>"; the default is the reference mockup's lane cell.
    readonly property var cell: (Quickshell.env("SPIKE_CELL") ?? "4x3").split("x").map(Number)

    readonly property int laneCount: 6
    // MONITOR's lane area: 1256 wide, and six lanes with 6 px gaps under the 104 px band, less the lane's
    // own 8 px padding above and below its graph.
    readonly property real laneWidthPx: 1256
    readonly property real laneHeightPx: 70
    readonly property real laneGapPx: 6
    readonly property real marginPx: 12
    readonly property real cellGapPx: 1
    // The daemon's history keys; six lanes reuse them, so every lane scrolls on every tick.
    readonly property var laneKeys: ["load", "ccd0", "ccd1", "gpu_load", "load", "gpu_load"]
    readonly property int fullColumns: Math.floor((laneWidthPx + cellGapPx) / (cell[0] + cellGapPx))

    // compare mode: a fixed series that covers missing samples, every level, an empty and a full column.
    readonly property int compareLength: 300
    readonly property int compareMissingEvery: 37
    readonly property int grabDelayMs: 2500
    readonly property int liveGrabRounds: 2
    property int grabRound: 0
    readonly property var compareSeries: {
        const series = [];
        for (let i = 0; i < compareLength; i += 1) {
            series.push(i % compareMissingEvery === 0 ? null : (i * 7919) % 101);
        }
        return series;
    }

    readonly property int secondsPerMinute: 60
    // At 1 Hz. A lane wider than the history shows its oldest columns as missing, as MONITOR would.
    readonly property int historySamples: Data.config ? Data.config.dashboard.history_minutes * secondsPerMinute : 0
    readonly property bool historyReady: historySamples > 0 && (Data.history.load || []).length >= Math.min(fullColumns, historySamples)
    // Theme.u is 1 until the Binding below gives it the screen's unit, which is larger on every screen wider than 1280.
    readonly property bool geometryReady: Theme.u > 1
    property bool created: false

    Component.onCompleted: {
        if (!modes.includes(mode)) {
            console.error("SPIKE_MODE must be one of " + modes.join(", ") + ", got '" + mode + "'");
            Qt.quit();
            return;
        }
        console.info("spike mode=" + mode + " cell=" + cell.join("x") + " columns=" + fullColumns);
        Data.want("cpu");
        Data.want("gpu");
    }

    // Lanes are created once, at their final size and with a full history, so the creation time covers
    // the work of a real MONITOR view rather than a rebuild at provisional units.
    onHistoryReadyChanged: Qt.callLater(createLanes)
    onGeometryReadyChanged: Qt.callLater(createLanes)

    function createLanes() {
        if (created || !historyReady || !geometryReady) {
            return;
        }
        created = true;
        const started = Date.now();
        lanesLoader.active = true;
        console.info("spike lanes created in " + (Date.now() - started) + " ms");
        if (mode === "compare") {
            grabTimer.start();
        }
    }

    function grab(item, name) {
        const path = grabDir + "/" + name + ".png";
        // At the screen's own scale in both directions. The default size cut a 140.0-pixel height to 139,
        // which scaled the grab unevenly and put cell edges on pixel centres that the screen never has.
        const ratio = window.devicePixelRatio;
        const size = Qt.size(Math.round(item.width * ratio) / ratio, Math.round(item.height * ratio) / ratio);
        item.grabToImage(result => console.info("spike grab " + (result.saveToFile(path) ? "saved " : "FAILED ") + path), size);
    }

    PanelWindow {
        id: window

        screen: Quickshell.screens.find(s => s.model.includes("M27Q")) ?? Quickshell.screens[0]
        anchors {
            bottom: true
            left: true
            right: true
        }
        implicitHeight: Theme.d(root.laneCount * root.laneHeightPx + (root.laneCount - 1) * root.laneGapPx + 2 * root.marginPx)
        exclusionMode: ExclusionMode.Ignore
        color: Theme.ground
        WlrLayershell.layer: WlrLayer.Bottom
        WlrLayershell.namespace: "ghost-spike"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        // An empty mask lets every click through, so the spike takes no input from the live desktop.
        mask: Region {}

        Binding {
            target: Theme.hostControl
            property: "u"
            // From the screen, as Desktop.qml does: the window's own width passes through provisional values.
            value: Scale.designUnit(window.screen.width)
        }

        Loader {
            id: lanesLoader

            x: Theme.d(root.marginPx)
            y: Theme.d(root.marginPx)
            active: false
            sourceComponent: root.mode === "compare" ? compareComponent : lanesComponent
        }
    }

    Component {
        id: lanesComponent

        Column {
            spacing: Theme.d(root.laneGapPx)

            Repeater {
                model: root.mode === "none" ? 0 : root.laneCount

                Loader {
                    required property int index

                    width: Theme.d(root.laneWidthPx)
                    height: Theme.d(root.laneHeightPx)
                    sourceComponent: root.mode === "shader" ? shaderLane : rectLane

                    Component {
                        id: rectLane

                        BlockGraph {
                            values: Data.history[root.laneKeys[index]] || []
                            cellW: root.cell[0]
                            cellH: root.cell[1]
                            gap: root.cellGapPx
                        }
                    }

                    Component {
                        id: shaderLane

                        ShaderBlockGraph {
                            values: Data.history[root.laneKeys[index]] || []
                            cellW: root.cell[0]
                            cellH: root.cell[1]
                            gap: root.cellGapPx
                        }
                    }
                }
            }
        }
    }

    Component {
        id: compareComponent

        Column {
            spacing: Theme.d(root.laneGapPx)

            BlockGraph {
                id: rectFixed

                width: Theme.d(root.laneWidthPx)
                height: Theme.d(root.laneHeightPx)
                values: root.compareSeries
                cellW: root.cell[0]
                cellH: root.cell[1]
                gap: root.cellGapPx
            }

            ShaderBlockGraph {
                id: shaderFixed

                width: Theme.d(root.laneWidthPx)
                height: Theme.d(root.laneHeightPx)
                values: root.compareSeries
                cellW: root.cell[0]
                cellH: root.cell[1]
                gap: root.cellGapPx
            }

            BlockGraph {
                id: rectLive

                width: Theme.d(root.laneWidthPx)
                height: Theme.d(root.laneHeightPx)
                values: Data.history.load || []
                cellW: root.cell[0]
                cellH: root.cell[1]
                gap: root.cellGapPx
            }

            ShaderBlockGraph {
                id: shaderLive

                width: Theme.d(root.laneWidthPx)
                height: Theme.d(root.laneHeightPx)
                values: Data.history.load || []
                cellW: root.cell[0]
                cellH: root.cell[1]
                gap: root.cellGapPx
            }

            // Two rounds of the live pair, far enough apart for a daemon tick between them, show that the
            // shader's image follows new data and still matches the Rectangles after it.
            Connections {
                target: grabTimer

                function onTriggered() {
                    root.grabRound += 1;
                    if (root.grabRound === 1) {
                        root.grab(rectFixed, "rect");
                        root.grab(shaderFixed, "shader");
                    }
                    root.grab(rectLive, "rect-live" + root.grabRound);
                    root.grab(shaderLive, "shader-live" + root.grabRound);
                    if (root.grabRound === root.liveGrabRounds) {
                        grabTimer.stop();
                    }
                }
            }
        }
    }

    Timer {
        id: grabTimer

        interval: root.grabDelayMs
        repeat: true
    }
}
