import QtQuick
import GhostKit

// One icon of the tray: a drawn glyph for a built-in, or whatever a status item puts inside it. It reports clicks and
// wheel notches and shows its tip after a pause, so a pointer passing over the bar opens nothing.
Item {
    id: root

    // Design pixels, from the reference mockup's .tray and .tip.
    readonly property real iconPx: 13
    readonly property real hitPadPx: 5
    readonly property real tipTextPx: 9.5
    readonly property real tipPadXPx: 8
    readonly property real tipPadYPx: 2
    readonly property real tipGapPx: 1
    readonly property real tipFillAlpha: 0.96
    // Letter spacing in em.
    readonly property real tipTracking: 0.04
    readonly property int tipOpenMs: 300
    // The wheel's delta for one notch, in eighths of a degree.
    readonly property real notchDelta: 120

    property string glyph
    property bool dim: false
    property string tip
    // Hides the tip while something else, such as this item's menu, is open beside it.
    property bool tipSuppressed: false
    default property alias content: holder.data

    readonly property bool tipShown: hoverDelay.shown && tip !== "" && !tipSuppressed
    property real wheelRemainder: 0

    signal clicked(int button)
    signal scrolled(int notches)

    implicitWidth: Theme.d(iconPx)
    implicitHeight: Theme.d(iconPx)

    // A status item has no glyph of its own, and Glyph warns about an empty name.
    Loader {
        anchors.fill: parent
        active: root.glyph !== ""

        sourceComponent: Glyph {
            name: root.glyph
            size: Theme.d(root.iconPx)
            color: root.dim ? Theme.dim : Theme.signal
        }
    }

    Item {
        id: holder

        anchors.fill: parent
    }

    Timer {
        id: hoverDelay

        property bool shown: false

        interval: root.tipOpenMs
        onTriggered: shown = true
    }

    MouseArea {
        id: area

        anchors {
            fill: parent
            margins: -Theme.d(root.hitPadPx)
        }
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: mouse => root.clicked(mouse.button)
        onContainsMouseChanged: {
            if (containsMouse) {
                hoverDelay.restart();
            } else {
                hoverDelay.stop();
                hoverDelay.shown = false;
            }
        }
        onWheel: wheel => {
            root.wheelRemainder += wheel.angleDelta.y;
            const notches = Math.trunc(root.wheelRemainder / root.notchDelta);
            root.wheelRemainder -= notches * root.notchDelta;
            if (notches !== 0) {
                root.scrolled(notches);
            }
        }
    }

    BarPopup {
        anchorItem: root
        gap: Theme.d(root.hitPadPx + root.tipGapPx)
        open: root.tipShown
        content: Component {
            Rectangle {
                implicitWidth: label.implicitWidth + Theme.d(2 * root.tipPadXPx)
                implicitHeight: label.implicitHeight + Theme.d(2 * root.tipPadYPx)
                color: Qt.alpha(Theme.ground, root.tipFillAlpha)
                border {
                    width: Theme.d(1)
                    color: Theme.hair
                }

                Text {
                    id: label

                    anchors.centerIn: parent
                    text: root.tip.toUpperCase()
                    textFormat: Text.PlainText
                    color: Theme.text
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.tipTextPx)
                        weight: Font.DemiBold
                        letterSpacing: Theme.d(root.tipTextPx) * root.tipTracking
                    }
                }
            }
        }
    }
}
