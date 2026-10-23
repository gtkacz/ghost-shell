import Quickshell
import "host"

ShellRoot {
    Greeting {
        id: greeting
    }

    Desktop {
        greetingHides: greeting.hidesDashboard
    }
}
