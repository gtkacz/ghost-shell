pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import "lib/notifications.mjs" as NotificationsLib

// The notification centre's state and the actions on the history: whether it is open, what is typed, which row is
// selected. Anything can open it; the panel in the host only draws what is here and forwards the keys. The bell wants
// the topic for its whole life, so this singleton never does.
Singleton {
    id: root

    readonly property bool open: core.open
    property string query: ""
    property int index: 0
    // Written by the dashboard widget while it is shown and the desktop is uncovered; the toasts stay quiet behind it.
    property bool widgetVisible: false
    // The bell's rect in screen coordinates, which the centre is placed from.
    property rect anchorRect: Qt.rect(0, 0, 0, 0)
    // Seconds, captured once: only what arrives after this shell started may pop up.
    readonly property real shellStartS: core.startS

    readonly property var snapshot: Data.topic("notifications").value
    // Scalars apart from the snapshot: each republish rebinds them but QML signals only a changed value, so the bell
    // stays still while only the items change.
    readonly property string server: snapshot?.server ?? "none"
    readonly property bool serving: server === NotificationsLib.SERVER
    readonly property int widgetReadDwellMs: NotificationsLib.WIDGET_READ_DWELL_MS
    readonly property int unread: snapshot?.unread ?? 0
    readonly property int unreadCrit: snapshot?.unread_crit ?? 0
    readonly property bool dnd: snapshot?.dnd ?? false
    readonly property bool inhibited: snapshot?.inhibited ?? false
    readonly property string suppressed: snapshot?.suppressed ?? ""
    readonly property var bell: NotificationsLib.bellState({
        server: server,
        unread: unread,
        unreadCrit: unreadCrit,
        dnd: dnd
    })
    readonly property var records: core.open ? NotificationsLib.filterRecords(snapshot?.items ?? [], query) : []

    onRecordsChanged: index = NotificationsLib.clampSelection(index, records.length)

    function countsLine(items) {
        return NotificationsLib.countsLine(items);
    }

    function widgetReadDue(state) {
        return NotificationsLib.widgetReadDue(state);
    }

    function toggle() {
        if (core.open) {
            close();
        } else {
            show();
        }
    }

    // Not open(): that name belongs to the property.
    function show() {
        if (core.open) {
            return;
        }
        query = "";
        index = 0;
        // The header scrambles once per opening.
        FirstWrite.showView("centre");
        FrameClock.acquire();
        core.open = true;
        console.info("centre opened");
    }

    function close() {
        if (!core.open) {
            return;
        }
        core.open = false;
        FrameClock.release();
        markAllRead();
        console.info("centre closed");
    }

    // Asks the sender to act, then brings its window forward when one is found.
    function invoke(id, key) {
        const record = (snapshot?.items ?? []).find(item => item.id === id);
        Data.call("notifications.invoke", {
            id: id,
            key: key
        });
        const window = record ? NotificationsLib.windowToRaise(Data.topic("windows").value?.windows ?? [], record) : null;
        if (window !== null) {
            Data.call("windows.activate", {
                id: window
            });
        }
        close();
    }

    function dismiss(id) {
        Data.call("notifications.dismiss", {
            id: id
        });
    }

    function markAllRead() {
        Data.call("notifications.read", {
            all: true
        });
    }

    function clear() {
        Data.call("notifications.clear", {});
    }

    function toggleDnd() {
        Data.call("notifications.dnd", {
            on: !dnd
        });
    }

    function type(text) {
        if (!core.open) {
            return;
        }
        query += text;
        index = 0;
    }

    // Takes Qt-style names: Escape, Return, Delete, Backspace, Up and Down.
    function key(name) {
        if (!core.open) {
            return;
        }
        const selected = records[index];
        switch (name) {
        case "Escape":
            close();
            break;
        case "Return":
            if (selected !== undefined) {
                invoke(selected.id, "default");
            }
            break;
        case "Delete":
            if (selected !== undefined) {
                dismiss(selected.id);
            }
            break;
        case "Backspace":
            query = query.slice(0, -1);
            index = 0;
            break;
        case "Up":
        case "Down":
            index = NotificationsLib.moveSelection(index, name, records.length);
            break;
        }
    }

    // A reading of "open" or "closed" with the query and the selection, for the headless smoke.
    function status() {
        return (core.open ? "open" : "closed") + " query=" + query + " index=" + index + " shown=" + records.length;
    }

    // The first desktop value on a connection is the state as it was, not a press.
    function takeDesktop(desktop) {
        const accepted = NotificationsLib.acceptNotificationsSeq(core.seen, desktop, core.connection);
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
        readonly property real startS: Math.floor(Date.now() / 1000)
        // Counts the connections to ghostd, whose first desktop value each is only a baseline.
        property int connection: 0
        property var seen: null
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

    // Lets scripts/qml-smoke.sh drive the centre without a keyboard or pointer in its headless KWin. It lives in this
    // singleton because Quickshell 0.2.1 keeps a destroyed handler registered, and calling it then crashes qs.
    IpcHandler {
        enabled: Quickshell.env("GHOST_DEBUG") === "1"
        target: "notifications"

        function toggle(): void {
            root.toggle();
        }

        function type(text: string): void {
            root.type(text);
        }

        function key(name: string): void {
            root.key(name);
        }

        function status(): string {
            return root.status();
        }
    }
}
