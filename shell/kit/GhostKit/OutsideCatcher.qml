import QtQuick
import Quickshell
import Quickshell.Wayland

// A transparent surface over a whole screen that turns the next click into clicked(), so a popup can close when the
// pointer goes elsewhere. The click is consumed: it neither reaches the window under it nor the bar.
Scope {
    id: root

    property bool active: false
    property ShellScreen screen

    signal clicked

    LazyLoader {
        active: root.active && root.screen !== null

        // One layer below the popups it serves, which sit on Overlay. The namespace is the transient one, which no
        // KWin effect fades (spike 2026-10-23-bar-bridge-spike).
        PanelWindow {
            screen: root.screen
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }
            exclusionMode: ExclusionMode.Ignore
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Top
            WlrLayershell.namespace: "utility"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.AllButtons
                onPressed: root.clicked()
            }
        }
    }
}
