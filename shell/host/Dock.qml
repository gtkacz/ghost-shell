import QtQuick
import Quickshell
import Quickshell.Wayland
import GhostKit
import "../kit/GhostKit/lib/theme.mjs" as ThemeLib

// Layout B's floating dock, centred on the edge opposite the bar.
Scope {
    id: root

    // Design pixels, from the reference mockup's .dock.
    readonly property real edgeGapPx: 10
    readonly property real cellHeightPx: 38
    readonly property real padPx: 6
    readonly property real padEndPx: 12
    readonly property real cellGapPx: 2
    readonly property real fillPct: 90

    required property var screen
    property bool atBottom: true
    // The resolved slots of the dock, as Data.layout.bar.dock carries them.
    property var slots: []

    PanelWindow {
        id: window

        screen: root.screen
        // Centred by margin rather than left to the compositor, so a popup beside a dock item knows where the dock is.
        anchors {
            left: true
            top: !root.atBottom
            bottom: root.atBottom
        }
        margins {
            left: Math.max(0, (root.screen.width - width) / 2)
            top: Theme.d(root.edgeGapPx)
            bottom: Theme.d(root.edgeGapPx)
        }
        implicitWidth: row.implicitWidth + Theme.d(root.padPx + root.padEndPx)
        implicitHeight: Theme.d(root.cellHeightPx + 2 * root.padPx)
        exclusionMode: ExclusionMode.Ignore
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "dock"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

        Panel {
            anchors.fill: parent
            header: false
            padding: 0
            fill: ThemeLib.withAlpha(Theme.ground, root.fillPct)

            BarRow {
                id: row

                anchors {
                    fill: parent
                    topMargin: Theme.d(root.padPx)
                    bottomMargin: Theme.d(root.padPx)
                    leftMargin: Theme.d(root.padPx)
                    rightMargin: Theme.d(root.padEndPx)
                }
                area: "dock"
                slots: root.slots
                spacing: Theme.d(root.cellGapPx)
            }
        }
    }
}
