pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import Qt.labs.folderlistmodel
import "lib/launcher.mjs" as LauncherLib

// The launcher's state and rules: whether it is open, what is typed, which row is selected and whether a power action
// is armed. Anything can open it; the panel in the host only draws what is here and forwards the keys.
Singleton {
    id: root

    readonly property int burnMs: 140
    readonly property int maxRecent: 5
    readonly property string flatpakCommand: "flatpak"
    readonly property string dataHome: Quickshell.env("XDG_DATA_HOME") || Quickshell.env("HOME") + "/.local/share"
    // The actions in display order, each with the daemon method that carries it out.
    readonly property var powerActions: [
        {
            id: "lock",
            label: "LOCK",
            glyph: "lock",
            method: "session.lock"
        },
        {
            id: "logout",
            label: "LOG OUT",
            glyph: "logout",
            method: "session.logout"
        },
        {
            id: "sleep",
            label: "SLEEP",
            glyph: "sleep",
            method: "session.suspend"
        },
        {
            id: "restart",
            label: "RESTART",
            glyph: "restart",
            method: "session.reboot"
        },
        {
            id: "shutdown",
            label: "SHUT DOWN",
            glyph: "power",
            method: "session.shutdown"
        }
    ]

    readonly property bool open: core.open
    readonly property string query: core.query
    readonly property int index: core.index
    // The row showing the launch colour for the burnMs before the panel closes, or -1.
    readonly property int burning: core.burning
    readonly property string armed: core.power.armed ?? ""
    // The power button the keyboard is on, or -1 while it is on the list.
    readonly property int powerFocus: core.powerFocus
    readonly property var items: core.open ? LauncherLib.sections({
        query: core.query,
        pinned: pinnedApps,
        recent: recentFiles,
        all: apps
    }) : []
    readonly property int pinCount: items.filter(item => item.kind === "pin").length
    readonly property int appCount: apps.length

    readonly property var userIds: {
        const ids = new Set();
        const folder = userFolder.item;
        for (let at = 0; folder && at < folder.count; at++) {
            ids.add(folder.get(at, "fileName").replace(/\.desktop$/, ""));
        }
        return ids;
    }
    readonly property var apps: {
        if (!core.open) {
            return [];
        }
        return LauncherLib.dedupe(Array.from(DesktopEntries.applications.values).map(adapt), dataHome);
    }
    readonly property var pinnedApps: {
        if (!core.open) {
            return [];
        }
        return (Data.config?.bar?.pinned ?? []).map(id => DesktopEntries.byId(id)).filter(entry => entry !== null).map(adapt);
    }
    readonly property var recentFiles: core.open ? (Data.topic("recent").value?.files ?? []).slice(0, maxRecent) : []

    // The plain object the rules work on. DesktopEntry exposes no path, so one is made from where the entry must
    // have come from: the user's applications directory, a Flatpak export, or else the system's.
    function adapt(desktop) {
        const command = Array.from(desktop.command ?? []);
        let path = "/usr/share/applications/" + desktop.id + ".desktop";
        if (command[0] && command[0].split("/").pop() === flatpakCommand) {
            path = dataHome + "/flatpak/exports/share/applications/" + desktop.id + ".desktop";
        } else if (userIds.has(desktop.id)) {
            path = dataHome + "/applications/" + desktop.id + ".desktop";
        }
        return {
            id: desktop.id,
            name: desktop.name,
            genericName: desktop.genericName,
            keywords: Array.from(desktop.keywords ?? []),
            categories: Array.from(desktop.categories ?? []),
            command: command.join(" "),
            noDisplay: desktop.noDisplay,
            icon: desktop.icon,
            path: path,
            desktop: desktop
        };
    }

    function toggle() {
        if (core.open) {
            close();
        } else {
            show();
        }
    }

    function show() {
        if (core.open) {
            return;
        }
        core.query = "";
        core.index = 0;
        core.burning = -1;
        core.powerFocus = -1;
        core.power = LauncherLib.disarm(core.power);
        FirstWrite.showView("launcher");
        Data.want("recent");
        FrameClock.acquire();
        core.open = true;
        console.info("launcher opened");
    }

    function close() {
        if (!core.open) {
            return;
        }
        core.open = false;
        core.power = LauncherLib.disarm(core.power);
        burnTimer.stop();
        armTimer.stop();
        FrameClock.release();
        Data.release("recent");
        console.info("launcher closed");
    }

    function type(text) {
        if (!core.open || core.burning >= 0) {
            return;
        }
        core.query += text;
        core.index = 0;
        core.powerFocus = -1;
    }

    // Takes Qt-style names: Escape, Return, Backspace, Tab, Backtab and the four arrows. An auto-repeat of Return is dropped here too, so no route confirms a power action from one held press.
    function key(name, isAutoRepeat) {
        if (!core.open || core.burning >= 0 || LauncherLib.isActivationRepeat(name, isAutoRepeat === true)) {
            return;
        }
        switch (name) {
        case "Escape":
            close();
            break;
        case "Return":
            activate();
            break;
        case "Backspace":
            core.query = core.query.slice(0, -1);
            core.index = 0;
            break;
        case "Tab":
            focusPower(core.powerFocus + 1);
            break;
        case "Backtab":
            focusPower(core.powerFocus < 0 ? powerActions.length - 1 : core.powerFocus - 1);
            break;
        case "Up":
        case "Down":
        case "Left":
        case "Right":
            core.powerFocus = -1;
            core.index = LauncherLib.move({
                index: core.index,
                pinCount: pinCount,
                total: items.length
            }, name);
            break;
        }
    }

    // One step past the last button, or before the first, goes back to the list.
    function focusPower(next) {
        core.powerFocus = next >= powerActions.length ? -1 : next;
    }

    function activate() {
        if (core.powerFocus >= 0) {
            press(powerActions[core.powerFocus].id);
            return;
        }
        launch(core.index);
    }

    function launch(at) {
        if (!core.open || core.burning >= 0) {
            return;
        }
        const item = items[at];
        if (item === undefined) {
            if (core.query !== "") {
                Data.call("krunner.query", {
                    text: core.query
                });
                console.info("launcher krunner query");
                close();
            }
            return;
        }
        core.index = at;
        if (item.kind === "file") {
            Quickshell.execDetached(["xdg-open", item.file.path]);
            console.info("launcher opened file");
        } else {
            Launch.app(item.entry.desktop);
            console.info("launched " + item.entry.id);
        }
        core.burning = at;
        burnTimer.restart();
    }

    function press(actionId) {
        if (!core.open || !powerActions.some(candidate => candidate.id === actionId)) {
            return;
        }
        const result = LauncherLib.power(core.power, actionId, Date.now(), Qt.styleHints.mouseDoubleClickInterval);
        core.power = result.state;
        if (result.fire !== null) {
            armTimer.stop();
            const action = powerActions.find(candidate => candidate.id === result.fire);
            console.info("launcher power " + action.id);
            Data.call(action.method, {});
            close();
        } else if (result.state.armed !== null) {
            armTimer.restart();
        }
    }

    // A reading of "open" or "closed" with the query and the selection, for the headless smoke.
    function status() {
        return (core.open ? "open" : "closed") + " query=" + core.query + " index=" + core.index + " armed=" + (core.power.armed ?? "none");
    }

    // The first desktop value on a connection is the state as it was, not a press.
    function takeDesktop(desktop) {
        const accepted = LauncherLib.acceptLauncherSeq(core.seen, desktop, core.connection);
        core.seen = accepted.seen;
        if (accepted.toggle) {
            toggle();
        }
    }

    Component.onCompleted: {
        if (Data.topic("desktop").value !== null) {
            takeDesktop(Data.topic("desktop").value);
        }
    }

    QtObject {
        id: core

        property bool open: false
        property string query: ""
        property int index: 0
        property int burning: -1
        property int powerFocus: -1
        property var power: LauncherLib.initialPower
        // Counts the connections to ghostd, whose first desktop value each is only a baseline.
        property int connection: 0
        property var seen: null
    }

    Timer {
        id: burnTimer

        interval: root.burnMs
        onTriggered: root.close()
    }

    // The armed look ends with the window in which a second press confirms.
    Timer {
        id: armTimer

        interval: LauncherLib.ARM_WINDOW_MS
        onTriggered: core.power = LauncherLib.disarm(core.power)
    }

    Loader {
        id: userFolder

        active: core.open

        sourceComponent: FolderListModel {
            folder: "file://" + root.dataHome + "/applications"
            nameFilters: ["*.desktop"]
            showDirs: false
        }
    }

    Connections {
        target: Data.topic("desktop")

        function onValueChanged() {
            root.takeDesktop(Data.topic("desktop").value);
        }
    }

    Connections {
        target: Data

        function onConnectedChanged() {
            if (Data.connected) {
                core.connection += 1;
            }
        }
    }

    // Lets scripts/qml-smoke.sh drive the launcher without a keyboard or pointer in its headless KWin. It lives in this
    // singleton because Quickshell 0.2.1 keeps a destroyed handler registered, and calling it then crashes qs.
    IpcHandler {
        enabled: Quickshell.env("GHOST_DEBUG") === "1"
        target: "launcher"

        function toggle(): void {
            root.toggle();
        }

        function type(text: string): void {
            root.type(text);
        }

        function repeat(name: string): void {
            root.key(name, true);
        }

        function key(name: string): void {
            root.key(name);
        }

        function power(action: string): void {
            root.press(action);
        }

        function status(): string {
            return root.status();
        }
    }
}
