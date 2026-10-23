import QtQuick
import "lib/gate.mjs" as GateLib

// One screen of the gate: ground, dots and scanlines everywhere, and the style named by the snapshot on the primary one.
Item {
    id: root

    property var adapter
    property bool primary: false

    // Emitted once the screen has nothing left to show: after the scan pass at unlock, at once at login.
    signal done

    Component.onCompleted: Theme.fit(width)
    onWidthChanged: Theme.fit(width)

    Rectangle {
        anchors.fill: parent
        color: Theme.ground
    }

    Dots {
        anchors.fill: parent
    }

    // Clicking anywhere gives the password field the keyboard back; the chips and actions sit above and take their own.
    MouseArea {
        anchors.fill: parent
        onClicked: {
            if (style.item) {
                style.item.refocus();
            }
        }
    }

    // The pass erases from the top by clipping the style away above its line, which leaves the Dots drawn here.
    Item {
        y: scan.lineY
        width: parent.width
        height: parent.height - y
        clip: true

        Item {
            y: -parent.y
            width: root.width
            height: root.height

            Loader {
                id: style

                anchors.fill: parent
                active: root.primary
                sourceComponent: Theme.style === "prompt" ? promptStyle : null
                onLoaded: console.info("gate: " + Theme.style + " " + root.adapter.mode)
            }
        }
    }

    ScanPass {
        id: scan

        anchors.fill: parent
        onFinished: root.done()
    }

    Scanlines {
        anchors.fill: parent
    }

    Component {
        id: promptStyle

        Prompt {
            adapter: root.adapter
        }
    }

    Connections {
        target: root.adapter

        function onSucceeded() {
            if (root.primary) {
                console.info("gate: succeeded");
            }
            if (root.primary && root.adapter.mode === "lock") {
                scan.start();
            } else {
                root.done();
            }
        }

        function onFailed() {
            if (root.primary) {
                console.info("gate: failed " + root.adapter.attempts);
            }
        }
    }
}
