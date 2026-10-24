import QtQuick
import "lib/gate.mjs" as GateLib

// One screen of the gate: ground, dots and scanlines everywhere, and the style named by the snapshot on the primary one.
Item {
    id: root

    property var adapter
    property bool primary: false

    // Emitted once the screen has nothing left to show: after the scan pass at unlock, at once at login.
    signal done

    // Types a key that reached another screen's window into this screen's prompt.
    function type(event) {
        if (style.item) {
            style.item.type(event);
        }
    }

    // The keyboard can land on any screen's window, and only the primary one has a field. KWin moves the keyboard on
    // requestActivate() only after real input, so the first keys are passed on by hand until it has.
    focus: true
    Keys.onPressed: (event) => {
        const target = PrimaryScreen.screen;
        if (primary || !target) {
            return;
        }
        target.type(event);
        target.Window.window.requestActivate();
        event.accepted = true;
    }

    Component.onCompleted: {
        Theme.fit(width);
        if (primary) {
            PrimaryScreen.screen = root;
        }
    }
    onWidthChanged: Theme.fit(width)
    onPrimaryChanged: {
        if (primary) {
            PrimaryScreen.screen = root;
        } else if (PrimaryScreen.screen === root) {
            PrimaryScreen.screen = null;
        }
    }
    Component.onDestruction: {
        if (PrimaryScreen.screen === root) {
            PrimaryScreen.screen = null;
        }
    }

    // Another screen that was primary for a moment gives the slot up empty, so the real one takes it back.
    Connections {
        target: PrimaryScreen

        function onScreenChanged() {
            if (root.primary && PrimaryScreen.screen === null) {
                PrimaryScreen.screen = root;
            }
        }
    }

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
            } else if (PrimaryScreen.screen) {
                PrimaryScreen.screen.Window.window.requestActivate();
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

        // Only the primary screen reports done, so a host that exits on the first one cannot cut the pass short.
        function onSucceeded() {
            if (!root.primary) {
                return;
            }
            console.info("gate: succeeded");
            if (root.adapter.mode === "lock") {
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
