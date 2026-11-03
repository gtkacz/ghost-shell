import QtQuick
import Quickshell
import Quickshell.Wayland

// A solid strip on the layer the real bar uses, so a capture can tell whether the backdrop covers it.
ShellRoot {
    PanelWindow {
        anchors {
            top: true
            left: true
            right: true
        }
        implicitHeight: 56
        exclusiveZone: 56
        color: "#00ff00"
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "dock"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    }

    Component.onCompleted: console.info("spike bar ready")
}
