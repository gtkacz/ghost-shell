import QtQuick
import Quickshell
import GhostKit
import "host"

ShellRoot {
    // Read once, so a bar that is off stays off for the whole run.
    readonly property bool barEnabled: Quickshell.env("GHOST_BAR") !== "off"

    // A singleton is created on its first use, which would start the toasts' clock late: touching it here makes
    // everything that arrives once the shell is loaded count as new.
    Component.onCompleted: console.info("notifications ready (start " + Notifications.shellStartS + ")")

    Greeting {
        id: greeting
    }

    Desktop {
        id: desktop

        greetingHides: greeting.hidesDashboard
    }

    // Rebuilt for each main screen, as Desktop's windows are: KWin closes a layer surface when its output goes away, and
    // a window that was closed stays closed when it is only given a new screen.
    Variants {
        model: barEnabled && desktop.mainScreen !== null ? [desktop.mainScreen] : []

        Bar {
            required property var modelData

            screen: modelData
        }
    }

    LazyLoader {
        active: barEnabled && desktop.mainScreen !== null

        LauncherPanel {
            screen: desktop.mainScreen
        }
    }

    LazyLoader {
        active: barEnabled && desktop.mainScreen !== null

        Toasts {
            screen: desktop.mainScreen
        }
    }

    LazyLoader {
        active: barEnabled && desktop.mainScreen !== null

        NotificationCentre {
            screen: desktop.mainScreen
        }
    }
}
