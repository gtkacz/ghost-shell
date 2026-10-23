import Quickshell
import "host"

ShellRoot {
    // Read once, so a bar that is off stays off for the whole run.
    readonly property bool barEnabled: Quickshell.env("GHOST_BAR") !== "off"

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
}
