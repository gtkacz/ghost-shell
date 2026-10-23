import QtQuick
import GhostKit

// Shows whether the dashboard is on a main view or on MONITOR; the chip toggles MONITOR and the chevrons swap the main view.
Item {
    id: root

    anchors.fill: parent

    // Design pixels, from the reference mockup's .modechip and .swapchip.
    readonly property real textPx: 10
    readonly property real borderPx: 1
    readonly property real padXPx: 8
    readonly property real padYPx: 2
    readonly property real glyphPx: 12
    readonly property real gapPx: 4
    // Letter spacing in em.
    readonly property real tracking: 0.14

    property var settings

    implicitWidth: chip.width + Theme.d(gapPx) + swap.width

    Rectangle {
        id: chip

        anchors.verticalCenter: parent.verticalCenter
        width: label.implicitWidth + Theme.d(2 * (root.padXPx + root.borderPx))
        height: label.implicitHeight + Theme.d(2 * (root.padYPx + root.borderPx))
        color: "transparent"
        border {
            width: Theme.d(root.borderPx)
            color: chipArea.containsMouse ? Theme.signal : Theme.hair
        }

        Text {
            id: label

            anchors.centerIn: parent
            text: Views.isMonitor ? "MONITOR" : "MAIN"
            color: Theme.hi
            font {
                family: Theme.font
                pixelSize: Theme.d(root.textPx)
                weight: Font.DemiBold
                letterSpacing: Theme.d(root.textPx) * root.tracking
            }
        }

        MouseArea {
            id: chipArea

            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: Views.toggleMonitor()
        }
    }

    Item {
        id: swap

        anchors {
            left: chip.right
            leftMargin: Theme.d(root.gapPx)
            verticalCenter: parent.verticalCenter
        }
        width: glyph.width
        height: parent.height

        Glyph {
            id: glyph

            anchors.verticalCenter: parent.verticalCenter
            name: "swap"
            size: Theme.d(root.glyphPx)
            color: Theme.signal
        }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: Views.swap()
        }
    }
}
