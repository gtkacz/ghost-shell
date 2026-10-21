import QtQuick
import Quickshell
import Quickshell.Wayland

ShellRoot {
    id: root

    readonly property var modes: ["static", "data", "anim"]
    readonly property string mode: Quickshell.env("SPIKE_MODE") ?? "static"
    readonly property bool showData: mode === "data" || mode === "anim"
    readonly property bool showMotion: mode === "anim"

    readonly property int rectColumns: 30
    readonly property int rectRows: 20
    readonly property int rectPitch: 12
    readonly property int dataIntervalMs: 1000
    readonly property int motionIntervalMs: 66
    readonly property int motionTravelPx: 200

    property int tick: 0

    Component.onCompleted: {
        if (!modes.includes(mode)) {
            console.error("SPIKE_MODE must be one of " + modes.join(", ") + ", got '" + mode + "'");
            Qt.quit();
        }
    }

    PanelWindow {
        id: window

        screen: Quickshell.screens.find(s => s.model.includes("M27Q")) ?? Quickshell.screens[0]
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Bottom
        // gridlock needs the "desktop" namespace for KWin to keep its surface visible under Show Desktop; reusing it here
        // keeps the layer as the only variable between the two surfaces.
        WlrLayershell.namespace: "desktop"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        // An empty mask lets every click through, so the spike does not steal input from the live desktop.
        mask: Region {}

        readonly property real gridLeft: width - (root.rectColumns + 2) * root.rectPitch
        readonly property real gridTop: height - (root.rectRows + 2) * root.rectPitch

        Component.onCompleted: console.info("spike mode=" + root.mode + " screen=" + screen.name + " model=" + screen.model)

        ShaderEffect {
            anchors.fill: parent
            property real spacing: 24 * window.devicePixelRatio
            property size resolution: Qt.size(width * window.devicePixelRatio, height * window.devicePixelRatio)
            fragmentShader: Qt.resolvedUrl("dots.frag.qsb")
        }

        Repeater {
            model: root.showData ? root.rectColumns * root.rectRows : 0

            Rectangle {
                required property int index

                x: window.gridLeft + (index % root.rectColumns) * root.rectPitch
                y: window.gridTop + Math.floor(index / root.rectColumns) * root.rectPitch
                width: root.rectPitch - 2
                height: root.rectPitch - 2
                color: Qt.hsla(((index * 7 + root.tick * 37) % 360) / 360, 0.6, 0.5, 0.25)
            }
        }

        Rectangle {
            id: mover

            property int step: 0

            visible: root.showMotion
            x: window.gridLeft + step
            y: window.gridTop - 2 * root.rectPitch
            width: 8
            height: 8
            color: Qt.rgba(0.72, 0.82, 0.80, 0.4)
        }
    }

    Timer {
        interval: root.dataIntervalMs
        repeat: true
        running: root.showData
        onTriggered: root.tick++
    }

    Timer {
        interval: root.motionIntervalMs
        repeat: true
        running: root.showMotion
        onTriggered: mover.step = (mover.step + 1) % root.motionTravelPx
    }
}
