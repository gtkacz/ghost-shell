import QtQuick
import Quickshell

// A stand-in window for smoke-switcher.sh: one flat colour, and a log line for each change of focus.
ShellRoot {
    id: root

    readonly property string name: Quickshell.env("SMOKE_WINDOW")

    FloatingWindow {
        implicitWidth: 1800
        implicitHeight: 1100
        color: Quickshell.env("SMOKE_COLOUR")
        title: "smoke " + root.name

        // Only an item has the attached Window, which reports the focus.
        Item {
            id: sink

            anchors.fill: parent
        }

        Connections {
            target: sink.Window.window

            function onActiveChanged() {
                console.info("smoke-window " + root.name + " active " + sink.Window.window.active);
            }
        }
    }
}
