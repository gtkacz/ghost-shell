import QtQuick
import GhostKit

// A StatusNotifierItem of another app. Left-click activates it, middle-click activates it secondarily and right-click
// shows its menu, as does a left-click on an item that is only a menu.
TrayItem {
    id: root

    required property var item

    property bool menuOpen: false

    // Plasma lists a passive item only in its expanded tray, which the chevron opens.
    visible: TrayRules.showsInline(item.status)

    tip: item.tooltipTitle || item.title || item.id
    tipSuppressed: menuOpen

    onClicked: button => {
        if (button === Qt.MiddleButton) {
            item.secondaryActivate();
        } else if (button === Qt.RightButton || item.onlyMenu) {
            menuOpen = item.hasMenu;
        } else {
            item.activate();
        }
    }

    AppIcon {
        anchors.fill: parent
        source: root.item.icon
        size: Theme.d(root.iconPx)
    }

    BarPopup {
        anchorItem: root
        gap: Theme.d(root.hitPadPx + root.tipGapPx)
        open: root.menuOpen
        catchOutside: true
        onDismissed: root.menuOpen = false
        content: Component {
            TrayMenu {
                menu: root.item.menu
                onTriggered: root.menuOpen = false
            }
        }
    }
}
