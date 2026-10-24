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

    LazyLoader {
        active: barEnabled && desktop.mainScreen !== null

        Bar {
            screen: desktop.mainScreen
        }
    }

    LazyLoader {
        active: barEnabled && desktop.mainScreen !== null

        LauncherPanel {
            screen: desktop.mainScreen
        }
    }
}
