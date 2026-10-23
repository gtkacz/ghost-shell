import QtQuick
import QtQuick.Shapes
import GhostKit

// The brand block at the bar's left edge, which opens the launcher.
Item {
    id: root

    anchors.fill: parent

    // Design pixels, from the reference mockup's .brand.
    readonly property real chamferPx: 14
    readonly property real padLeftPx: 12
    readonly property real padRightPx: 26
    readonly property real gapPx: 8
    readonly property real textPx: 15
    readonly property real subtitlePx: 10
    // Letter spacing in em.
    readonly property real textTracking: 0.04
    readonly property real separatorOpacity: 0.5
    readonly property real subtitleOpacity: 0.75
    readonly property string separatorText: "//"

    property var settings

    readonly property var brand: Data.config ? Data.config.bar.brand : null
    readonly property bool shown: brand !== null && brand.enabled
    readonly property bool atBottom: Data.config !== null && Data.config.bar.position === "bottom"
    readonly property var pieces: shown ? brand.text.split(separatorText) : [""]
    readonly property bool hasSeparator: pieces.length > 1
    readonly property bool hasSubtitle: shown && brand.subtitle !== "" && Theme.japanese !== "off"
    readonly property real chamfer: Theme.d(chamferPx)
    readonly property font textFont: Qt.font({
        family: Theme.font,
        pixelSize: Theme.d(textPx),
        weight: Font.Bold,
        letterSpacing: Theme.d(textPx) * textTracking
    })

    visible: shown
    implicitWidth: shown ? Theme.d(padLeftPx + padRightPx) + head.implicitWidth + separator.implicitWidth + tail.implicitWidth + (hasSubtitle ? Theme.d(gapPx) + subtitle.implicitWidth : 0) : 0

    Shape {
        anchors.fill: parent
        // The default renderer leaves the cut without antialiasing.
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: Theme.signal
            strokeColor: "transparent"
            startX: 0
            startY: 0

            PathLine {
                x: root.atBottom ? root.width - root.chamfer : root.width
                y: 0
            }
            PathLine {
                x: root.atBottom ? root.width : root.width - root.chamfer
                y: root.height
            }
            PathLine {
                x: 0
                y: root.height
            }
            PathLine {
                x: 0
                y: 0
            }
        }
    }

    Text {
        id: head

        x: Theme.d(root.padLeftPx)
        anchors.verticalCenter: parent.verticalCenter
        text: root.pieces[0]
        color: Theme.ground
        font: root.textFont
    }

    Text {
        id: separator

        anchors {
            left: head.right
            baseline: head.baseline
        }
        text: root.hasSeparator ? root.separatorText : ""
        color: Theme.ground
        opacity: root.separatorOpacity
        font: root.textFont
    }

    Text {
        id: tail

        anchors {
            left: separator.right
            baseline: head.baseline
        }
        text: root.hasSeparator ? root.pieces.slice(1).join(root.separatorText) : ""
        color: Theme.ground
        font: root.textFont
    }

    Text {
        id: subtitle

        anchors {
            left: tail.right
            leftMargin: root.hasSubtitle ? Theme.d(root.gapPx) : 0
            baseline: head.baseline
        }
        text: root.hasSubtitle ? root.brand.subtitle : ""
        color: Theme.ground
        opacity: root.subtitleOpacity
        font {
            family: Theme.fontCjk
            pixelSize: Theme.d(root.subtitlePx)
            weight: Font.DemiBold
        }
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: Launcher.toggle()
    }
}
