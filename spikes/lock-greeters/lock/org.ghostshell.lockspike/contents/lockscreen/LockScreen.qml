import QtQuick
import QtCore
import org.kde.plasma.workspace.keyboardlayout as KeyboardLayouts
import org.kde.plasma.private.keyboardindicator as KeyboardIndicator
import org.kde.plasma.private.sessions
import "GhostGate"

// Prints what the lock greeter hands its root; never starts authenticating and never responds.
Rectangle {
    id: root
    property bool viewVisible: false
    property bool locked: false
    color: "#102030"

    function log(key, value) {
        console.log("SPIKE " + key + " = " + JSON.stringify(value));
    }
    function membersOf(object) {
        const names = [];
        for (const name in object) {
            names.push(name);
        }
        return names.sort();
    }

    KeyboardLayouts.KeyboardLayout { id: layouts }
    KeyboardIndicator.KeyState {
        id: capsLock
        key: Qt.Key_CapsLock
        onLockedChanged: root.log("capsLock.locked changed", locked)
    }
    SessionManagement { id: sessions }
    Probe { id: probe }
    Settings {
        id: gate
        location: StandardPaths.writableLocation(StandardPaths.GenericStateLocation) + "/ghost-shell/gate.ini"
    }
    Settings {
        id: missingGate
        location: StandardPaths.writableLocation(StandardPaths.GenericStateLocation) + "/ghost-shell/absent.ini"
    }

    focus: true
    Keys.onPressed: event => log("key pressed", event.key)
    onViewVisibleChanged: log("viewVisible", viewVisible)
    onLockedChanged: log("locked", locked)

    Timer {
        interval: 1500
        running: true
        onTriggered: {
            root.log("package", "org.ghostshell.lockspike");
            root.log("interfaceVersion", org_kde_plasma_screenlocker_greeter_interfaceVersion);
            root.log("view", String(org_kde_plasma_screenlocker_greeter_view));
            root.log("userName", kscreenlocker_userName);
            root.log("userImage", kscreenlocker_userImage);
            root.log("authenticator.members", root.membersOf(authenticator));
            root.log("authenticator.state", authenticator.state);
            root.log("authenticator.authenticatorTypes", authenticator.authenticatorTypes);
            root.log("authenticator.graceLocked", authenticator.graceLocked);
            root.log("config", String(config));
            root.log("Screen.name", Screen.name);
            root.log("Window.window", String(root.Window.window));
            root.log("size", [root.width, root.height]);
            root.log("capsLock.locked", capsLock.locked);
            root.log("sessions.canSuspend", sessions.canSuspend);
            root.log("sessions.canSwitchUser", sessions.canSwitchUser);
            root.log("sessions.canHibernate", sessions.canHibernate);
            root.log("layouts.layoutsList", layouts.layoutsList);
            root.log("layouts.layout", layouts.layout);
            root.log("gate.location", String(gate.location));
            root.log("gate.style", gate.value("gate/style", "MISSING"));
            root.log("missingGate.style", missingGate.value("gate/style", "DEFAULT"));
            root.log("GhostGate", probe.hello("the lock greeter"));
            layouts.switchToNextLayout();
            switchCheck.start();
        }
    }
    Timer {
        id: switchCheck
        interval: 1000
        onTriggered: root.log("layouts.layout after switchToNextLayout", layouts.layout)
    }
}
