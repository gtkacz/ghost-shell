import QtQuick
import GhostKit

// What hovering or clicking an app shows: its name in capitals, then one row per window, or NOT RUNNING.
Rectangle {
    id: root

    // Design pixels, from the reference mockup's .tip.
    readonly property real textPx: 9.5
    readonly property real padXPx: 8
    readonly property real padYPx: 2
    readonly property real rowHeightPx: 16
    readonly property real maxTitlePx: 280
    readonly property real fillAlpha: 0.96
    readonly property real hoverFillAlpha: 0.08
    // Letter spacing in em.
    readonly property real tracking: 0.04
    readonly property string notRunningText: "NOT RUNNING"

    property string appName
    property var windows: []
    readonly property bool hovered: hover.hovered

    signal picked(string id)

    readonly property font tipFont: Qt.font({
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

    HoverHandler {
        id: hover
    }

    Column {
        id: column

        x: Theme.d(root.padXPx)
        y: Theme.d(root.padYPx)

        Item {
            width: header.implicitWidth
            height: Theme.d(root.rowHeightPx)

            Text {
                id: header

                anchors.verticalCenter: parent.verticalCenter
                text: root.appName.toUpperCase() + (root.windows.length === 0 ? "  " + root.notRunningText : "")
                textFormat: Text.PlainText
                color: root.windows.length === 0 ? Theme.dim : Theme.hi
                font: root.tipFont
            }
        }

        Repeater {
            model: root.windows

            Item {
                id: row

                required property var modelData

                width: title.width
                height: Theme.d(root.rowHeightPx)

                Rectangle {
                    x: -Theme.d(root.padXPx)
                    width: root.width
                    height: parent.height
                    color: area.containsMouse ? Qt.alpha(Theme.signal, root.hoverFillAlpha) : "transparent"
                }

                Text {
                    id: title

                    width: Math.min(implicitWidth, Theme.d(root.maxTitlePx))
                    anchors.verticalCenter: parent.verticalCenter
                    text: row.modelData.caption
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: row.modelData.active ? Theme.hi : Theme.text
                    font: root.tipFont
                }

                MouseArea {
                    id: area

                    x: -Theme.d(root.padXPx)
                    width: root.width
                    height: parent.height
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.picked(row.modelData.id)
                }
            }
        }
    }
}
