import Quickshell
import "host"

ShellRoot {
    // First, so its overlay is created before the dashboard.
    Greeting {
        id: greeting
    }

    Desktop {
        greetingHides: greeting.hidesDashboard
    }
}
