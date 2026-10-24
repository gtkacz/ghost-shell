import QtQuick
import QtQuick.Shapes
import GhostKit

// The dock's launcher tile, which opens the launcher.
Item {
    id: root

    anchors.fill: parent

    // Design pixels, from the reference mockup's .dock .launch.
    readonly property real tileWidthPx: 40
    readonly property real marginRightPx: 8
    readonly property real chamferPx: 9
    readonly property real textPx: 12

    property var settings

    readonly property real chamfer: Theme.d(chamferPx)
    readonly property real tileWidth: Theme.d(tileWidthPx)

    implicitWidth: tileWidth + Theme.d(marginRightPx)

    Shape {
        width: root.tileWidth
        height: parent.height
        // The default renderer leaves the cut without antialiasing.
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: Theme.signal
            strokeColor: "transparent"
            startX: 0
            startY: 0

            PathLine {
                x: root.tileWidth
                y: 0
            }
            PathLine {
                x: root.tileWidth
                y: root.height
            }
            PathLine {
                x: root.chamfer
                y: root.height
            }
            PathLine {
                x: 0
                y: root.height - root.chamfer
            }
            PathLine {
                x: 0
                y: 0
            }
        }
    }

    Text {
        x: (root.tileWidth - implicitWidth) / 2
        anchors.verticalCenter: parent.verticalCenter
        text: "GS"
        color: Theme.ground
        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
            weight: Font.Bold
        }
    }

    MouseArea {
        width: root.tileWidth
        height: parent.height
        cursorShape: Qt.PointingHandCursor
        onClicked: Launcher.toggle()
    }
}
