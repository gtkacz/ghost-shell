import QtQuick
import Quickshell
import GhostKit

// An application's icon by theme name. The mono icon style tints it in the signal colour, or in hi for the focused
// app; the colour style draws it as it is.
Item {
    id: root

    property string name
    property bool focused: false
    property real size: Theme.d(17)

    readonly property bool mono: (Data.config ? Data.config.bar.icons : "mono") === "mono"
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1

    implicitWidth: size
    implicitHeight: size

    // Shown as it is in the colour style; the Duotone below draws it when it is tinted.
    Image {
        id: picture

        anchors.fill: parent
        source: root.name !== "" ? "image://icon/" + root.name : ""
        sourceSize: Qt.size(root.size * root.pixelRatio, root.size * root.pixelRatio)
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        visible: !root.mono
    }

    // The shadow is transparent so a dark part of the icon shows the bar through it instead of a box.
    Duotone {
        anchors.fill: parent
        visible: root.mono
        source: picture
        shadowColor: "transparent"
        lightColor: root.focused ? Theme.hi : Theme.signal
    }
}
