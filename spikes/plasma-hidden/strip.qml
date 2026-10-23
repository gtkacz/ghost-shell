import QtQuick
import Quickshell
import Quickshell.Wayland

// A red stand-in for the bar's strip, to see which of it and Plasma's popups KWin stacks on top.
// SPIKE_EXCLUSIVE=1 reserves the strip's height as an exclusive zone; otherwise it reserves nothing.
ShellRoot {
    id: root

    readonly property int stripHeight: 52
    readonly property bool exclusive: Quickshell.env("SPIKE_EXCLUSIVE") === "1"

    PanelWindow {
        anchors {
            top: true
            left: true
            right: true
        }
        implicitHeight: root.stripHeight
        exclusionMode: root.exclusive ? ExclusionMode.Auto : ExclusionMode.Ignore
        color: "#ff0000"
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "dock"
    }
}
