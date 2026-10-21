pragma Singleton

import QtQuick
import Quickshell
import "lib/views.mjs" as ViewsLib

// Which view the dashboard shows, changed by the view shortcuts (through the daemon's desktop topic)
// and by toggleMonitor() and swap(). The print between two views is the host's to draw.
Singleton {
    id: root

    readonly property string current: views.current
    readonly property string lastMain: views.lastMain
    readonly property bool isMonitor: current !== "" && dashboard !== null && current === dashboard.monitor_view
    // How long the latest change prints for; 0 for a change shown at once.
    readonly property int durationMs: views.durationMs

    // True while the host prints a change, which ignores every action until it ends. With no
    // dashboard on screen nothing prints, so a change is immediate. Read-only here so that one widget
    // cannot lock the views; the host writes it through hostControl.
    readonly property bool busy: hostControl.busy
    readonly property QtObject hostControl: QtObject {
        property bool busy: false
    }

    readonly property var dashboard: Data.config ? Data.config.dashboard : null

    function toggleMonitor() {
        views.perform(ViewsLib.MONITOR_ACTION);
    }

    function swap() {
        views.perform(ViewsLib.SWAP_ACTION);
    }

    // Views are destroyed when left, so the tab each tabs node shows is kept here for the session.
    function tabIndex(view, path, fallback) {
        const chosen = views.tabChoices.get(view + path);
        return chosen !== undefined ? chosen : fallback;
    }

    function setTabIndex(view, path, index) {
        views.tabChoices.set(view + path, index);
    }

    onDashboardChanged: views.reconcile()
    // A singleton is created on first use, which may come after the first config and desktop value.
    Component.onCompleted: {
        views.reconcile();
        if (Data.topic("desktop").value !== null) {
            views.take(Data.topic("desktop").value);
        }
    }

    // The state lives in here so widgets see only the properties and functions above.
    QtObject {
        id: views

        property string current
        property string lastMain
        property int durationMs: 0
        // Counts the connections to ghostd, whose first desktop value each is only a baseline.
        property int connection: 0
        property var seen: null
        // Read only when a tabs host is created, so it is updated in place and notifies nothing.
        readonly property var tabChoices: new Map()

        // The duration goes first and the view last, so whoever follows the view already sees how
        // long its print takes.
        function show(next, ms) {
            durationMs = ms;
            lastMain = next.lastMain;
            current = next.current;
        }

        function perform(action) {
            if (root.busy || root.dashboard === null || current === "") {
                return;
            }
            const next = ViewsLib.applyAction({
                current,
                lastMain
            }, action, root.dashboard);
            if (next === null) {
                return;
            }
            console.info("view " + current + " -> " + next.current + " (" + next.durationMs + " ms)");
            show(next, next.durationMs);
        }

        function reconcile() {
            if (root.dashboard === null) {
                return;
            }
            if (current === "") {
                show(ViewsLib.initialViews(root.dashboard), 0);
                return;
            }
            if (!Data.layout) {
                return;
            }
            const next = ViewsLib.reconcileViews({
                current,
                lastMain
            }, root.dashboard, Object.keys(Data.layout.views));
            if (next.current !== current) {
                console.info("view " + current + " -> " + next.current + " (0 ms)");
            }
            if (next.current !== current || next.lastMain !== lastMain) {
                show(next, 0);
            }
        }

        function take(desktop) {
            const accepted = ViewsLib.acceptAction(seen, desktop, connection);
            seen = accepted.seen;
            if (accepted.action !== null) {
                perform(accepted.action);
            }
        }
    }

    Connections {
        target: Data

        function onConnectedChanged() {
            if (Data.connected) {
                views.connection += 1;
            }
        }

        function onLayoutChanged() {
            views.reconcile();
        }
    }

    Connections {
        target: Data.topic("desktop")

        function onValueChanged() {
            views.take(Data.topic("desktop").value);
        }
    }
}
