import QtQuick
import Quickshell

// Red, so any red in a capture after the unlock is a window shown before the greeting.
ShellRoot {
    id: root

    readonly property color red: "#ff0000"

    FloatingWindow {
        id: stand

        implicitWidth: Quickshell.screens[0].width
        implicitHeight: Quickshell.screens[0].height
        color: root.red
        title: "greeting-handoff red"

        Item {
            id: sink

            anchors.fill: parent
            focus: true
            Keys.onPressed: event => console.info("spike red key " + event.key + " at " + Date.now())
        }

        Connections {
            target: sink.Window.window

            function onActiveChanged() {
                console.info("spike red active " + sink.Window.window.active + " at " + Date.now());
            }
        }

        Component.onCompleted: console.info("spike red window ready at " + Date.now())
    }
}
