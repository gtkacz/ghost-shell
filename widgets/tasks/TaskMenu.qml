import QtQuick
import GhostKit

// The right-click menu of an app: the entry's own actions, then PIN or UNPIN, then CLOSE. Each item is
// {label, kind, rule, action}; a rule draws a hairline above it.
Rectangle {
    id: root

    // Design pixels, from the reference mockup's .tip.
    readonly property real textPx: 9.5
    readonly property real padXPx: 8
    readonly property real padYPx: 2
    readonly property real rowHeightPx: 18
    readonly property real fillAlpha: 0.96
    readonly property real hoverFillAlpha: 0.08
    // Letter spacing in em.
    readonly property real tracking: 0.04

    property var items: []

    signal picked(var item)

    readonly property font menuFont: Qt.font({
        family: Theme.font,
        pixelSize: Theme.d(textPx),
        weight: Font.DemiBold,
        letterSpacing: Theme.d(textPx) * tracking
    })

    implicitWidth: column.implicitWidth + Theme.d(2 * padXPx)
    implicitHeight: column.implicitHeight + Theme.d(2 * padYPx)
    color: Qt.alpha(Theme.ground, fillAlpha)
    border {
        width: Theme.d(1)
        color: Theme.hair
    }

    Column {
        id: column

        x: Theme.d(root.padXPx)
        y: Theme.d(root.padYPx)

        Repeater {
            model: root.items

            Item {
                id: row

                required property var modelData

                width: label.implicitWidth
                height: Theme.d(root.rowHeightPx)

                Rectangle {
                    visible: row.modelData.rule
                    x: -Theme.d(root.padXPx)
                    width: root.width
                    height: Theme.d(1)
                    color: Theme.hair2
                }

                Rectangle {
                    x: -Theme.d(root.padXPx)
                    width: root.width
                    height: parent.height
                    color: area.containsMouse ? Qt.alpha(Theme.signal, root.hoverFillAlpha) : "transparent"
                }

                Text {
                    id: label

                    anchors.verticalCenter: parent.verticalCenter
                    text: row.modelData.label.toUpperCase()
                    textFormat: Text.PlainText
                    color: area.containsMouse ? Theme.hi : Theme.text
                    font: root.menuFont
                }

                MouseArea {
                    id: area

                    x: -Theme.d(root.padXPx)
                    width: root.width
                    height: parent.height
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.picked(row.modelData)
                }
            }
        }
    }
}
