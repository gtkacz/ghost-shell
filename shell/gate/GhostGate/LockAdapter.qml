import QtQuick
import org.kde.plasma.private.keyboardindicator as KeyboardIndicator
import org.kde.plasma.private.sessions as Sessions
import org.kde.plasma.workspace.keyboardlayout as KeyboardLayouts

// The gate's adapter contract (see FakeAdapter) on kscreenlocker's greeter: its `authenticator` and
// `kscreenlocker_userName` context properties and the Plasma modules its stock screen uses. The only file that touches
// them, so a Plasma update breaks here; the greeter then falls back to KDE's own screen rather than unlocking.
QtObject {
    id: root

    readonly property string mode: "lock"
    readonly property var users: [kscreenlocker_userName]
    readonly property string userName: kscreenlocker_userName
    property bool busy: false
    property int attempts: 0
    property string message: ""
    readonly property bool capsLock: capsState.locked
    readonly property var layouts: keyboard.layoutsList.map((entry) => entry.shortName)
    readonly property string layout: layouts.length > keyboard.layout ? layouts[keyboard.layout] : ""
    readonly property var sessions: []
    readonly property int sessionIndex: 0
    readonly property var allowed: ({
            switchUser: session.canSwitchUser,
            sleep: session.canSuspend,
            restart: false,
            shutdown: false
        })

    // A password typed before PAM has asked for one waits here, because a response nobody asked for is dropped.
    property string pending: ""
    property bool asked: false

    // Matches the stock screen's pause, which outlasts pam_unix's delay after a failure.
    readonly property int restartDelayMs: 3000

    // The stock screen's kind for the password, as opposed to fingerprint or smartcard.
    readonly property int passwordKind: 0

    property var capsState: KeyboardIndicator.KeyState {
        key: Qt.Key_CapsLock
    }

    property var keyboard: KeyboardLayouts.KeyboardLayout {}

    property var session: Sessions.SessionManagement {}

    property var retry: Timer {
        interval: root.restartDelayMs
        onTriggered: authenticator.startAuthenticating()
    }

    property var pam: Connections {
        target: authenticator

        function onPromptForSecretChanged() {
            root.asked = true;
            if (root.pending !== "") {
                root.respond();
            }
        }

        function onErrorMessageChanged() {
            root.message = authenticator.errorMessage;
        }

        // Every busy period ends in exactly one verdict, so the field never keeps a password the gate is not checking.
        function onFailed(kind) {
            root.asked = false;
            root.retry.restart();
            if (kind !== root.passwordKind || !root.busy) {
                return;
            }
            root.pending = "";
            root.attempts += 1;
            root.busy = false;
            root.failed();
        }

        function onSucceeded() {
            root.pending = "";
            root.busy = false;
            root.succeeded();
        }
    }

    signal succeeded
    signal failed

    function respond() {
        const password = pending;
        pending = "";
        asked = false;
        authenticator.respond(password);
    }

    function submit(password) {
        message = "";
        busy = true;
        pending = password;
        if (asked) {
            respond();
        }
    }

    function cycleUser() {
    }

    function cycleLayout() {
        keyboard.switchToNextLayout();
    }

    function chooseSession(index) {
    }

    function act(name) {
        if (name === "switch-user") {
            session.switchUser();
        } else if (name === "sleep") {
            session.suspend();
        }
    }

    // The greeter exits on this only once PAM has unlocked; otherwise it logs a warning and stays locked.
    function finish() {
        Qt.quit();
    }

    Component.onCompleted: authenticator.startAuthenticating()
}
