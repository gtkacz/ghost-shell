import QtQuick
import GhostKit

// The media widget in the bar: the art and the name, and a click on them opens the widget's stack in a popup.
Item {
    id: root

    // Design pixels, from the bar mockup's .np and its tall popup.
    readonly property real artPx: 22
    readonly property real sidePx: 8
    readonly property real gapPx: 8
    readonly property real namePx: 190
    readonly property real nameGapPx: 6
    readonly property real titlePx: 10
    readonly property real artistPx: 10
    readonly property real edgePx: 1
    readonly property real popupWidthPx: 248
    readonly property real popupGapPx: 1
    // The popup floats over windows, so it takes the tips' near-opaque ground rather than the panel's translucent fill.
    readonly property real popupFillAlpha: 0.96
    readonly property real openAlpha: 0.08
    // Letter spacing in em, as the panel's title.
    readonly property real titleTracking: 0.02
    readonly property real titleScrambleMs: 600
    readonly property real artistScrambleMs: 700

    required property string name
    required property string artist
    required property string artUrl
    required property string nameKey
    required property string artistKey
    required property bool playing
    required property bool mono
    // What the popup shows.
    required property Component popupBody

    property bool open: false

    readonly property bool atBottom: Data.config?.bar?.position === "bottom"

    // Whatever the length of the name, so the bar does not shift from song to song.
    implicitWidth: Theme.d(2 * sidePx + artPx + gapPx + namePx)

    Rectangle {
        anchors.fill: parent
        visible: root.open
        color: Qt.alpha(Theme.signal, root.openAlpha)
    }

    // On the edge facing the popup.
    Rectangle {
        anchors {
            left: parent.left
            right: parent.right
            top: root.atBottom ? parent.top : undefined
            bottom: root.atBottom ? undefined : parent.bottom
        }
        visible: root.open
        height: Theme.d(root.edgePx)
        color: Theme.signal
    }

    Art {
        id: art

        x: Theme.d(root.sidePx)
        anchors.verticalCenter: parent.verticalCenter
        sidePx: root.artPx
        source: root.artUrl
        mono: root.mono
        dimmed: !root.playing
    }

    ScrambleText {
        id: title

        anchors {
            left: art.right
            leftMargin: Theme.d(root.gapPx)
            verticalCenter: parent.verticalCenter
        }
        width: Math.min(implicitWidth, Theme.d(root.namePx))
        text: root.name
        scrambleKey: root.nameKey
        durationMs: root.titleScrambleMs
        color: root.playing ? Theme.hi : Theme.dim
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: Theme.d(root.titlePx)
            weight: Font.Bold
            letterSpacing: Theme.d(root.titlePx) * root.titleTracking
        }
    }

    ScrambleText {
        anchors {
            left: title.right
            leftMargin: Theme.d(root.nameGapPx)
            right: parent.right
            rightMargin: Theme.d(root.sidePx)
            verticalCenter: parent.verticalCenter
        }
        text: root.artist
        scrambleKey: root.artistKey
        durationMs: root.artistScrambleMs
        color: Theme.dim
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: Theme.d(root.artistPx)
            weight: Font.Medium
        }
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: root.open = !root.open
    }

    BarPopup {
        anchorItem: root
        gap: Theme.d(root.popupGapPx)
        open: root.open
        catchOutside: true
        onDismissed: root.open = false
        content: Component {
            Panel {
                header: false
                fill: Qt.alpha(Theme.ground, root.popupFillAlpha)
                implicitWidth: Theme.d(root.popupWidthPx)
                implicitHeight: body.implicitHeight + 2 * padding

                Loader {
                    id: body

                    anchors.fill: parent
                    sourceComponent: root.popupBody
                }
            }
        }
    }
}
