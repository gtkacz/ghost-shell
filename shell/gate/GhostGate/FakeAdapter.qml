import QtQuick
import "lib/gate.mjs" as GateLib

// Stands in for the real backends in tests and previews. The adapter contract is the properties, methods and signals
// below; the settable extras (holdBusy, submitCount) exist only here.
QtObject {
    id: root

    property string mode: "lock"
    property var users: ["gtkacz"]
    property string userName: users[0]
    property bool busy: false
    property int attempts: 0
    property string message: ""
    property bool capsLock: false
    property var layouts: ["us", "br"]
    property string layout: "us"
    property var sessions: [{ name: "Plasma (Wayland)" }]
    property int sessionIndex: 0
    property var allowed: ({ switchUser: true, sleep: true, restart: true, shutdown: true })

    // Stay busy after a submit, as PAM does while it thinks.
    property bool holdBusy: false
    property int submitCount: 0
    property string lastAction

    signal succeeded
    signal failed

    function submit(password) {
        submitCount += 1;
        if (holdBusy) {
            busy = true;
            return;
        }
        if (password === "ghost") {
            succeeded();
            return;
        }
        attempts += 1;
        failed();
    }

    function cycleUser() {
        userName = GateLib.nextUser(users, userName);
    }

    function cycleLayout() {
        layout = GateLib.nextLayout(layouts, layout);
    }

    function chooseSession(index) {
        sessionIndex = index;
    }

    function act(name) {
        lastAction = name;
    }
}
