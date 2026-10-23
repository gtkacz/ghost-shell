import QtQuick
import QtCore
import "GhostGate"

// Prints what sddm-greeter-qt6 hands a theme; test mode has no daemon, so nothing can log in.
Rectangle {
    id: root
    color: "#203010"

    function log(key, value) {
        console.log("SPIKE " + key + " = " + JSON.stringify(value));
    }

    Probe { id: probe }
    Settings { id: systemGate; location: "file:///var/lib/ghost-shell/gate.ini" }
    Settings { id: standInGate; location: "file://" + config.standInGate }
    Loader { id: kwinLayouts; source: "KwinLayouts.qml" }
    Loader { id: keyState; source: "KeyStateProbe.qml" }
    Connections {
        target: keyboard
        function onCapsLockStateChanged() { root.log("keyboard.capsLock changed", keyboard.capsLock); }
    }
    focus: true
    Keys.onPressed: event => log("key pressed", event.key)

    Timer {
        interval: 1500
        running: true
        onTriggered: {
            root.log("config.probe", config.probe);
            root.log("config.fromUserFile", config.fromUserFile);
            root.log("config.standInGate", config.standInGate);
            root.log("sddm.hostName", sddm.hostName);
            root.log("sddm.can", [sddm.canPowerOff, sddm.canReboot, sddm.canSuspend, sddm.canHibernate, sddm.canHybridSleep]);
            root.log("keyboard.enabled", keyboard.enabled);
            root.log("keyboard.layouts", keyboard.layouts.map(l => l.shortName + "/" + l.longName));
            root.log("keyboard.currentLayout", keyboard.currentLayout);
            root.log("keyboard.capsLock", keyboard.capsLock);
            keyboard.currentLayout = 1;
            root.log("keyboard.currentLayout after write 1", keyboard.currentLayout);
            root.log("userModel", [userModel.count, userModel.lastUser, userModel.lastIndex]);
            root.log("sessionModel", [sessionModel.count, sessionModel.lastIndex]);
            root.log("primaryScreen", primaryScreen);
            root.log("Screen.name", Screen.name);
            root.log("screenModel.count", screenModel.rowCount());
            root.log("systemGate.style", systemGate.value("gate/style", "DEFAULT"));
            root.log("standInGate.style", standInGate.value("gate/style", "MISSING"));
            root.log("GhostGate", probe.hello("sddm-greeter-qt6"));
            root.log("kwinLayouts.status", kwinLayouts.status);
            root.log("keyState.status", keyState.status);
            if (kwinLayouts.item) {
                root.log("kwinLayouts.layoutsList", kwinLayouts.item.layoutsList);
                kwinLayouts.item.switchToNextLayout();
                switchCheck.start();
            }
        }
    }
    Timer {
        id: switchCheck
        interval: 1000
        onTriggered: root.log("kwinLayouts.layout after switchToNextLayout", kwinLayouts.item.layout)
    }
}
