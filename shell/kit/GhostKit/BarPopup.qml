import QtQuick
import Quickshell
import Quickshell.Wayland

// A transient surface beside an item of the bar or dock, opening on the anchor's far side from its screen edge: below a
// top bar, above a bottom bar and above the dock. Place it among the anchor's siblings, so that it finds the window
// the anchor is in.
Item {
    id: root

    property Item anchorItem
    property bool open: false
    property Component content
    // Closes on a click anywhere else, by emitting dismissed() for the owner to set open to false.
    property bool catchOutside: false
    property int keyboardFocus: WlrKeyboardFocus.None

    signal dismissed

    readonly property var barWindow: QsWindow.window
    // Where the anchor sat when the popup last opened, in the bar window's pixels; a moved anchor re-places it on the
    // next opening.
    property rect anchorRect: Qt.rect(0, 0, 0, 0)
    readonly property bool below: barWindow !== null && !barWindow.anchors.bottom

    onOpenChanged: {
        if (open && anchorItem) {
            anchorRect = anchorItem.mapToItem(null, 0, 0, anchorItem.width, anchorItem.height);
        }
    }

    OutsideCatcher {
        active: root.open && root.catchOutside
        screen: root.barWindow ? root.barWindow.screen : null
        onClicked: root.dismissed()
    }

    LazyLoader {
        active: root.open && root.barWindow !== null && root.content !== null

        PanelWindow {
            id: popup

            // Both bars span the screen from its edge, so the window's margin is where it starts; the dock sets its own.
            readonly property real leftEdge: root.barWindow.margins.left + root.anchorRect.x

            screen: root.barWindow.screen
            anchors {
                left: true
                top: root.below
                bottom: !root.below
            }
            margins {
                left: Math.max(0, Math.min(popup.leftEdge, root.barWindow.screen.width - width))
                top: root.below ? root.barWindow.margins.top + root.anchorRect.y + root.anchorRect.height : 0
                bottom: root.below ? 0 : Math.max(0, root.barWindow.margins.bottom + root.barWindow.height - root.anchorRect.y)
            }
            exclusionMode: ExclusionMode.Ignore
            color: "transparent"
            implicitWidth: body.implicitWidth
            implicitHeight: body.implicitHeight
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "utility"
            WlrLayershell.keyboardFocus: root.keyboardFocus

            Loader {
                id: body

                sourceComponent: root.content
            }
        }
    }
}
