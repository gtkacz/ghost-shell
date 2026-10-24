import QtQuick
import QtCore
import "GhostGate"
import "GhostGate/lib/gate.mjs" as GateLib
import "GhostGate/lib/scale.mjs" as ScaleLib

// sddm-greeter-qt6 loads this once per screen, sized to it. GhostGate beside it is the copy install.sh assembles.
GateScreen {
    id: root

    // Set once the snapshot is in the Theme, so the primary screen is chosen by the configured display model only.
    property bool themed: false

    adapter: LoginAdapter {
        driving: root.primary
    }
    // Until the root is in its window, Screen reads the primary output, which would load the prompt on every screen.
    primary: themed && Window.window !== null && Screen.name === (ScaleLib.pickScreen(Qt.application.screens, Theme.displayModel) || {}).name

    // Absolute, because the greeter's own home has no state directory; ghostd writes here (see install.sh --login).
    Settings {
        id: snapshot

        location: "file:///var/lib/ghost-shell/gate.ini"
    }

    Component.onCompleted: {
        Theme.load(GateLib.readSnapshot((key) => snapshot.value(key)));
        themed = true;
    }
}
