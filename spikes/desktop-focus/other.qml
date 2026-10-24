import QtQuick
import Quickshell

// The window that holds the keyboard before the desktop surface asks for it.
ShellRoot {
    FloatingWindow {
        id: stand

        implicitWidth: 1200
        implicitHeight: 700
        color: "#802020"
        title: "desktop-focus other"

        Item {
            anchors.fill: parent
            focus: true
            Keys.onPressed: event => console.info("spike other key " + event.key + " at " + Date.now())

            MouseArea {
                anchors.fill: parent
                onPressed: mouse => console.info("spike other press " + mouse.x + "," + mouse.y + " at " + Date.now())
            }
        }

        Connections {
            target: stand.contentItem.Window.window

            function onActiveChanged() {
                console.info("spike other active " + stand.contentItem.Window.window.active + " at " + Date.now());
            }
        }

        Component.onCompleted: console.info("spike other ready at " + Date.now())
    }
}
