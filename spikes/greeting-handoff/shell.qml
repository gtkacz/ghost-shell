import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

// The greeting's stand-in: a full-screen Overlay layer in the ground colour, mapped once the file named by
// SPIKE_ARM_FILE exists and given the keyboard exclusively once SPIKE_FOCUS_FILE exists. Each swapped
// frame bumps the counter, which asks for the next frame, so the log shows exactly when Qt renders.
// KWin types a layer surface by its namespace (SPIKE_NAMESPACE), and the type decides which open effect
// animates it.
ShellRoot {
    id: root

    readonly property string armFile: Quickshell.env("SPIKE_ARM_FILE") ?? ""
    readonly property string focusFile: Quickshell.env("SPIKE_FOCUS_FILE") ?? ""
    readonly property string surfaceNamespace: Quickshell.env("SPIKE_NAMESPACE") ?? ""
    readonly property color ground: "#0a0f11"
    readonly property color counterColour: "#3d5552"
    readonly property int pollMs: 50
    readonly property int counterPx: 24
    readonly property int marginPx: 24

    property bool armed: false
    property bool focused: false
    property int frames: 0

    Component.onCompleted: {
        if (armFile === "" || focusFile === "" || surfaceNamespace === "") {
            console.error("SPIKE_ARM_FILE, SPIKE_FOCUS_FILE and SPIKE_NAMESPACE must all be set");
            Qt.quit();
            return;
        }
        console.info("spike overlay ready, namespace " + surfaceNamespace + ", at " + Date.now());
    }

    onArmedChanged: console.info("spike overlay armed at " + Date.now())
    onFocusedChanged: console.info("spike overlay exclusive keyboard requested at " + Date.now())

    FileView {
        id: armView

        path: root.armFile
        printErrors: false
        onLoaded: root.armed = true
    }

    FileView {
        id: focusView

        path: root.focusFile
        printErrors: false
        onLoaded: root.focused = true
    }

    // FileView does not report a file that appears after it first looked, so both are asked again.
    Timer {
        interval: root.pollMs
        repeat: true
        running: !root.armed || !root.focused
        onTriggered: {
            if (!root.armed) {
                armView.reload();
            }
            if (!root.focused) {
                focusView.reload();
            }
        }
    }

    PanelWindow {
        id: overlay

        visible: root.armed
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        exclusionMode: ExclusionMode.Ignore
        color: root.ground
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: root.surfaceNamespace
        WlrLayershell.keyboardFocus: root.focused ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

        Text {
            id: counter

            x: root.marginPx
            y: root.marginPx
            text: root.frames
            color: root.counterColour
            font.pixelSize: root.counterPx
            focus: true
            Keys.onPressed: event => console.info("spike overlay key " + event.key + " at " + Date.now())
        }

        Connections {
            target: counter.Window.window

            function onFrameSwapped() {
                root.frames += 1;
                console.info("spike overlay frame " + root.frames + " at " + Date.now());
            }

            function onActiveChanged() {
                console.info("spike overlay active " + counter.Window.window.active + " at " + Date.now());
            }
        }
    }
}
