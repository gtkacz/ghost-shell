import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

// The launch and popup checks. run.sh writes "<seq> <command>" to SPIKE_COMMAND_FILE, and each new line is
// acted on once:
//   launch-execute | launch-gtk | launch-kio   start SPIKE_ENTRY from a launcher-like surface that holds
//                                              the keyboard, then close that surface, as the launcher will
//   show popup | show dock | show utility      show one red sample surface of that kind at the same place
//   show none                                  hide every sample
ShellRoot {
    id: root

    readonly property string commandFile: Quickshell.env("SPIKE_COMMAND_FILE") ?? ""
    readonly property string entryId: Quickshell.env("SPIKE_ENTRY") ?? ""
    readonly property string entryPath: Quickshell.env("SPIKE_ENTRY_PATH") ?? ""
    readonly property color ground: "#0a0f11"
    readonly property color red: "#ff0000"
    readonly property int pollMs: 50
    readonly property int focusSettleMs: 300
    readonly property int barHeight: 40
    readonly property int sampleX: 400
    readonly property int sampleSize: 300

    property string lastCommand: ""
    property string shown: "none"
    property string pendingLaunch: ""
    property bool entryFound: false

    function log(text) {
        console.info("spike shell " + text + " at " + Date.now());
    }

    function handle(content) {
        const line = content.trim();
        if (line === "" || line === lastCommand) {
            return;
        }
        lastCommand = line;
        const words = line.split(" ");
        const command = words[1];
        if (command === "show") {
            shown = words[2];
            log("show " + shown);
        } else if (command.startsWith("launch-")) {
            pendingLaunch = command;
            launcher.visible = true;
            log("launcher shown for " + command);
            launchTimer.restart();
        } else {
            log("unknown command " + line);
        }
    }

    function launch(route) {
        if (route === "launch-execute") {
            const entry = DesktopEntries.byId(entryId);
            if (entry === null) {
                log("no entry " + entryId);
                return;
            }
            entry.execute();
        } else if (route === "launch-gtk") {
            Quickshell.execDetached(["gtk-launch", entryId]);
        } else if (route === "launch-kio") {
            Quickshell.execDetached(["kioclient", "exec", entryPath]);
        }
        log("launched " + route);
    }

    Component.onCompleted: {
        if (commandFile === "" || entryId === "" || entryPath === "") {
            console.error("SPIKE_COMMAND_FILE, SPIKE_ENTRY and SPIKE_ENTRY_PATH must all be set");
            Qt.quit();
            return;
        }
        log("ready");
    }

    FileView {
        id: commandView

        path: root.commandFile
        printErrors: false
        onLoaded: root.handle(text())
    }

    // FileView does not report a file that appears after it first looked, so it is asked again. DesktopEntries
    // fills in after startup, so the entry is looked for until it is there.
    Timer {
        interval: root.pollMs
        repeat: true
        running: true
        onTriggered: {
            commandView.reload();
            if (!root.entryFound && DesktopEntries.byId(root.entryId) !== null) {
                root.entryFound = true;
                root.log("entry found");
            }
        }
    }

    // The launcher closes as it launches, so the launched window never meets a surface holding the keyboard.
    Timer {
        id: launchTimer

        interval: root.focusSettleMs
        onTriggered: {
            root.launch(root.pendingLaunch);
            launcher.visible = false;
        }
    }

    PanelWindow {
        id: launcher

        visible: false
        anchors {
            top: true
            left: true
        }
        implicitWidth: root.sampleSize
        implicitHeight: root.sampleSize
        exclusionMode: ExclusionMode.Ignore
        color: root.ground
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "dock"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
    }

    PanelWindow {
        id: bar

        anchors {
            top: true
            left: true
            right: true
        }
        implicitHeight: root.barHeight
        exclusionMode: ExclusionMode.Ignore
        color: root.ground
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "dock"
    }

    PopupWindow {
        id: popup

        visible: root.shown === "popup"
        anchor.window: bar
        anchor.rect.x: root.sampleX
        anchor.rect.y: 0
        anchor.rect.width: 1
        anchor.rect.height: root.barHeight
        anchor.edges: Edges.Bottom | Edges.Left
        anchor.gravity: Edges.Bottom | Edges.Right
        implicitWidth: root.sampleSize
        implicitHeight: root.sampleSize
        color: root.red
    }

    Variants {
        model: ["dock", "utility"]

        PanelWindow {
            required property string modelData

            visible: root.shown === modelData
            anchors {
                top: true
                left: true
            }
            margins {
                top: root.barHeight
                left: root.sampleX
            }
            implicitWidth: root.sampleSize
            implicitHeight: root.sampleSize
            exclusionMode: ExclusionMode.Ignore
            color: root.red
            WlrLayershell.layer: WlrLayer.Top
            WlrLayershell.namespace: modelData
        }
    }
}
