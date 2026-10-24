import QtQuick
import QtCore
import "GhostGate"
import "GhostGate/lib/gate.mjs" as GateLib
import "GhostGate/lib/scale.mjs" as ScaleLib

// kscreenlocker's greeter loads this once per screen, sized to it. GhostGate beside it is the copy install.sh assembles.
GateScreen {
    id: root

    // Written by the greeter.
    property bool locked: false
    property bool viewVisible: false

    // Set once the snapshot is in the Theme, so the primary screen is chosen by the configured display model only.
    property bool themed: false

    adapter: LockAdapter {
        driving: root.primary
    }
    // Until the root is in its window, Screen reads the primary output, which would load the prompt on every screen.
    primary: themed && Window.window !== null && Screen.name === (ScaleLib.pickScreen(Qt.application.screens, Theme.displayModel) || {}).name

    onDone: adapter.finish()

    // Declares no properties: a declared one would be written back to ghostd's file whenever it changed.
    Settings {
        id: snapshot

        location: StandardPaths.writableLocation(StandardPaths.GenericStateLocation) + "/ghost-shell/gate.ini"
    }

    Component.onCompleted: {
        Theme.load(GateLib.readSnapshot((key) => snapshot.value(key)));
        themed = true;
    }
}
