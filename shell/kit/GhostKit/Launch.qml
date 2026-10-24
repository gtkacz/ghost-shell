pragma Singleton

import Quickshell
import "lib/launch.mjs" as LaunchLib

// The one way the shell starts an app, so every launch gets KDE's scope, startup notification and Exec parsing.
Singleton {
    id: root

    readonly property string home: Quickshell.env("HOME")
    readonly property var dirs: LaunchLib.searchDirs(Quickshell.env("XDG_DATA_HOME") || home + "/.local/share", Quickshell.env("XDG_DATA_DIRS"))

    function app(desktop) {
        if (!desktop) {
            return;
        }
        Quickshell.execDetached(LaunchLib.appCommand(desktop.id, dirs));
        console.info("launch app " + desktop.id);
    }

    function action(desktopAction) {
        Quickshell.execDetached(LaunchLib.actionCommand(desktopAction, home));
        console.info("launch action " + desktopAction.id);
    }
}
