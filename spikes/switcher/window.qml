import QtQuick
import Quickshell

// A stand-in window whose colour changes every second, so a capture can tell a live thumbnail from a frozen one.
ShellRoot {
    id: root

    readonly property string name: Quickshell.env("SPIKE_NAME")
    readonly property var colours: Quickshell.env("SPIKE_COLOURS").split(",")
    readonly property bool cycling: Quickshell.env("SPIKE_STATIC") !== "1"
    property int shown: 0

    FloatingWindow {
        implicitWidth: 1800
        implicitHeight: 1100
        color: root.colours[root.shown]
        title: "spike " + root.name

        Item {
            id: sink

            anchors.fill: parent
            focus: true
        }

        Timer {
            interval: 1000
            running: root.cycling
            repeat: true
            onTriggered: root.shown = (root.shown + 1) % root.colours.length
        }

        Connections {
            target: sink.Window.window

            function onActiveChanged() {
                console.info("spike " + root.name + " active " + sink.Window.window.active + " at " + Date.now());
            }
        }
    }
}
