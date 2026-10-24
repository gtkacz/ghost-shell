import QtQuick
import QtQml
import org.kde.plasma.private.keyboardindicator as KeyboardIndicator
import org.kde.plasma.workspace.keyboardlayout as KeyboardLayouts
import "lib/gate.mjs" as GateLib

// The gate's adapter contract (see FakeAdapter) on sddm-greeter-qt6: its `sddm`, `userModel` and `sessionModel` context
// objects. Layouts and Caps Lock come from Plasma's modules, because SDDM's own `keyboard` object is inert on Wayland.
QtObject {
    id: root

    // Every screen has an adapter, but only the primary screen's has a prompt to submit from.
    property bool driving: false

    readonly property string mode: "login"
    property string userName: userModel.lastUser
    readonly property var users: names.count > 0 ? names.list() : [userName]
    property bool busy: false
    property int attempts: 0
    property string message: ""
    readonly property bool capsLock: capsState.locked
    readonly property var layouts: keyboard.layoutsList.map((entry) => entry.shortName)
    readonly property string layout: layouts.length > keyboard.layout ? layouts[keyboard.layout] : ""
    readonly property var sessions: sessionNames.count > 0 ? sessionNames.list() : []
    property int sessionIndex: sessionModel.lastIndex
    readonly property var allowed: ({
            switchUser: false,
            sleep: sddm.canSuspend,
            restart: sddm.canReboot,
            shutdown: sddm.canPowerOff
        })

    property var capsState: KeyboardIndicator.KeyState {
        key: Qt.Key_CapsLock
    }

    property var keyboard: KeyboardLayouts.KeyboardLayout {}

    // A model's roles are only reachable from a delegate, so one object per row carries its name out.
    property var names: Instantiator {
        model: userModel

        delegate: QtObject {
            required property string name
        }

        function list() {
            return Array.from({ length: count }, (_, row) => objectAt(row).name);
        }
    }

    property var sessionNames: Instantiator {
        model: sessionModel

        delegate: QtObject {
            required property string name
        }

        function list() {
            return Array.from({ length: count }, (_, row) => ({ name: objectAt(row).name }));
        }
    }

    // Every busy period ends in exactly one verdict, so the field never keeps a password the gate is not checking.
    property var daemon: Connections {
        target: sddm
        enabled: root.driving

        function onLoginFailed() {
            root.attempts += 1;
            root.busy = false;
            root.failed();
        }

        function onLoginSucceeded() {
            root.busy = false;
            root.succeeded();
        }

        function onInformationMessage(text) {
            root.message = text;
        }
    }

    signal succeeded
    signal failed
    signal reset

    function submit(password) {
        message = "";
        busy = true;
        sddm.login(userName, password, sessionIndex);
    }

    function cycleUser() {
        userName = GateLib.nextUser(users, userName);
    }

    function cycleLayout() {
        keyboard.switchToNextLayout();
    }

    function chooseSession(index) {
        sessionIndex = index;
    }

    function act(name) {
        if (name === "sleep") {
            sddm.suspend();
        } else if (name === "restart") {
            sddm.reboot();
        } else if (name === "power") {
            sddm.powerOff();
        }
    }
}
