import QtQuick
import GhostKit

// An empty panel that holds a slot's place until the widget meant for it exists.
Panel {
    property var settings

    anchors.fill: parent
    label: settings.label
}
