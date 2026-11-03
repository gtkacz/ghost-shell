import QtQuick
import org.kde.kirigami as Kirigami

// A window's icon, which KWin hands over as a QIcon rather than a name, so GhostKit's AppIcon can't draw it. The mono icon
// style tints it, and the colour style draws it as it is.
Item {
    id: root

    property var source
    property bool selected: false

    readonly property bool mono: Theme.icons !== "colour"

    Kirigami.Icon {
        id: picture

        anchors.fill: parent
        source: root.source
        // A fade between icons would run on every Tab while the caption's icon follows the selection.
        animated: false
        visible: !root.mono
        // A hidden item gives Duotone a texture only through its layer.
        layer.enabled: root.mono
    }

    // The shadow is transparent so a dark part of the icon shows the panel through it instead of a box.
    Duotone {
        anchors.fill: parent
        visible: root.mono
        source: picture
        shadowColor: "transparent"
        lightColor: root.selected ? Theme.hi : Theme.dim
    }
}
