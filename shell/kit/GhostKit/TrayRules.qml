pragma Singleton

import Quickshell
import "lib/tray.mjs" as TrayLib

// Re-exports the tray rules so the tray widget reaches them through the GhostKit import instead of by file path.
Singleton {
    readonly property var applets: TrayLib.APPLETS

    function network(links) {
        return TrayLib.networkState(links);
    }

    function bluetooth(state) {
        return TrayLib.bluetoothState(state);
    }

    function volume(state) {
        return TrayLib.volumeState(state);
    }

    function kdeConnect(batteries) {
        return TrayLib.kdeConnectState(batteries);
    }

    function stepVolume(volume, notches) {
        return TrayLib.stepVolume(volume, notches);
    }
}
