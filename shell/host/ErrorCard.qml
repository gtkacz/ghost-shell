import QtQuick
import GhostKit

// Stands in for a widget that can't be shown: the panel names the widget and a crit alert gives the
// reason, so a broken slot never takes the rest of the view down with it.
Panel {
    id: root

    // Design pixels, from the reference mockup's panel body.
    readonly property real stackGapPx: 8
    readonly property real detailPx: 9.5

    required property string widgetId
    required property string message

    label: widgetId

    AlertBox {
        id: alert

        width: parent.width
        level: "crit"
        chipKey: "crit"
        text: root.message
    }

    // The alert is a single line, so a reason too long for it is repeated here in full.
    Text {
        anchors {
            top: alert.bottom
            topMargin: Theme.d(root.stackGapPx)
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        visible: alert.truncated
        text: root.message
        color: Data.connected && !Data.stale ? Theme.text : Theme.dim
        wrapMode: Text.Wrap
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: Theme.d(root.detailPx)
        }
    }
}
