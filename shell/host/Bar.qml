import QtQuick
import Quickshell
import Quickshell.Wayland
import GhostKit
import "../kit/GhostKit/lib/layout.mjs" as Layout
import "../kit/GhostKit/lib/theme.mjs" as ThemeLib

// The bar on the main screen: the left group at the screen's edge, the right group at the other, and the centre group
// between them. In layout B it also carries the dock.
Scope {
    id: root

    // Design pixels, from the reference mockup's .bar.
    readonly property real itemGapPx: 14
    readonly property real rightPadPx: 12
    readonly property real edgePx: 1
    readonly property real fillPct: 92

    // The main screen, which the shell picks once for the dashboard and the bar alike.
    required property var screen

    readonly property bool ready: Data.config !== null && Data.layout !== null
    readonly property string position: ready ? Data.config.bar.position : "top"
    readonly property string layoutName: ready ? Data.config.bar.layout : "a"
    readonly property bool atBottom: position === "bottom"
    readonly property var slots: ready && Data.layout.bar ? Data.layout.bar : null

    readonly property string summary: "bar on " + (screen ? screen.name : "") + " " + position + " layout " + layoutName
    // One line per arrangement, though the config and layout that make it ready arrive separately.
    property string announced: ""

    function announce() {
        if (ready && summary !== announced) {
            announced = summary;
            console.info(summary);
        }
    }

    // Deferred, since the handler of ready runs before the position and layout bindings have caught up with it.
    onSummaryChanged: Qt.callLater(announce)
    onReadyChanged: Qt.callLater(announce)

    PanelWindow {
        id: window

        // Whole device pixels, so the hairline and the fill meet the screen's edge exactly.
        function snap(logical) {
            return Math.round(logical * devicePixelRatio) / devicePixelRatio;
        }

        screen: root.screen
        visible: root.ready && root.slots !== null
        anchors {
            left: true
            right: true
            top: !root.atBottom
            bottom: root.atBottom
        }
        implicitHeight: snap(Theme.d(Layout.BAR_BAND_PX))
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "dock"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

        Rectangle {
            anchors.fill: parent
            color: ThemeLib.withAlpha(Theme.ground, root.fillPct)
        }

        // On the edge facing the middle of the screen.
        Rectangle {
            anchors {
                left: parent.left
                right: parent.right
                bottom: root.atBottom ? undefined : parent.bottom
                top: root.atBottom ? parent.top : undefined
            }
            height: window.snap(Theme.d(root.edgePx))
            color: Theme.hair
        }

        BarRow {
            id: left

            anchors {
                left: parent.left
                top: parent.top
                bottom: parent.bottom
            }
            area: "left"
            slots: root.slots ? root.slots.left : []
            spacing: Theme.d(root.itemGapPx)
        }

        BarRow {
            id: right

            anchors {
                right: parent.right
                rightMargin: Theme.d(root.rightPadPx)
                top: parent.top
                bottom: parent.bottom
            }
            area: "right"
            slots: root.slots ? root.slots.right : []
            spacing: Theme.d(root.itemGapPx)
        }

        BarRow {
            anchors {
                left: left.right
                leftMargin: Theme.d(root.itemGapPx)
                right: right.left
                rightMargin: Theme.d(root.itemGapPx)
                top: parent.top
                bottom: parent.bottom
            }
            area: "centre"
            slots: root.slots ? root.slots.centre : []
            spacing: Theme.d(root.itemGapPx)
            fill: true
        }
    }

    LazyLoader {
        active: root.ready && root.slots !== null && root.layoutName === "b"

        Dock {
            screen: root.screen
            atBottom: !root.atBottom
            slots: root.slots ? root.slots.dock : []
        }
    }
}
