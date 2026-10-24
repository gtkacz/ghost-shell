import QtQuick
import Quickshell
import GhostKit

// The pinned and running apps. This file holds what the icon and tab styles share: which apps there are, which
// window of an app a click means, and the calls that act on them.
Item {
    id: root

    anchors.fill: parent

    readonly property string genericIcon: "application-x-executable"
    readonly property string unmatchedPrefix: "class:"

    property var settings

    readonly property var windows: Data.topic("windows").value?.windows ?? []
    readonly property var launcherEntries: Data.topic("launcher.entries").value?.entries ?? null
    readonly property var desktop: Data.topic("desktop").value
    readonly property var pinned: Data.config?.bar?.pinned ?? []

    // Entries by lower-cased startup class. Reading the list makes this look again once the first scan has ended.
    readonly property var startupClasses: {
        const byClass = new Map();
        for (const entry of DesktopEntries.applications.values) {
            if (entry.startupClass !== "" && !byClass.has(entry.startupClass.toLowerCase())) {
                byClass.set(entry.startupClass.toLowerCase(), entry.id);
            }
        }
        return byClass;
    }
    readonly property var pinnedEntries: {
        return pinned.map(id => ({
                    id: id,
                    startupClass: entryOf(id)?.startupClass ?? ""
                }));
    }
    readonly property var lookups: ({
            pinned: pinnedEntries,
            hasId: id => entryOf(id) !== null,
            byStartupClass: windowClass => startupClasses.get(windowClass.toLowerCase()) ?? null,
            heuristic: windowClass => DesktopEntries.heuristicLookup(windowClass)?.id ?? null
        })

    readonly property var tasks: Tasks.build(pinned, windows, window => Tasks.matchOrder(window, lookups))
    readonly property var byApp: {
        const byId = ({});
        for (const task of tasks) {
            byId[task.appId] = task;
        }
        return byId;
    }
    // Changes only when an app appears, goes or moves, so a window's title changing does not rebuild the icons or close
    // a list that is open.
    readonly property string appKey: tasks.map(task => task.appId).join("\n")
    property var appIds: []

    // Asks the cell of an app to open its title list, which a task shortcut on an app with several windows means.
    signal listRequested(string appId)

    implicitWidth: view.item ? view.item.implicitWidth : 0

    onAppKeyChanged: appIds = tasks.map(task => task.appId)
    Component.onCompleted: appIds = tasks.map(task => task.appId)

    // The first value is the state as it was, not a press.
    property var handledSeq: null

    onDesktopChanged: {
        if (desktop === null) {
            return;
        }
        const seq = desktop.task_seq;
        const isPress = handledSeq !== null && seq !== handledSeq;
        handledSeq = seq;
        if (isPress) {
            pressShortcut(desktop.task);
        }
    }

    function entryOf(appId) {
        DesktopEntries.applications.values;
        return DesktopEntries.byId(appId);
    }

    function iconOf(task) {
        if (task.appId.startsWith(unmatchedPrefix)) {
            return genericIcon;
        }
        return entryOf(task.appId)?.icon || task.appId;
    }

    function nameOf(task) {
        const entry = entryOf(task.appId);
        if (entry) {
            return entry.name;
        }
        return task.appId.startsWith(unmatchedPrefix) ? task.appId.slice(unmatchedPrefix.length) : task.appId;
    }

    function pressShortcut(index) {
        const task = Tasks.pinnedTask(tasks, index);
        if (task === null) {
            return;
        }
        const action = Tasks.click(task);
        if (action.kind === "list") {
            listRequested(task.appId);
        } else {
            perform(task, action);
        }
    }

    // A window that has since gone is ignored by the KWin script, so a click on a stale list does no harm.
    function perform(task, action) {
        if (action.kind === "launch") {
            Launch.app(entryOf(task.appId));
        } else {
            Data.call("windows." + action.kind, {
                id: action.id
            });
        }
    }

    function launch(task) {
        Launch.app(entryOf(task.appId));
    }

    function activate(id) {
        Data.call("windows.activate", {
            id: id
        });
    }

    function closeApp(task) {
        for (const window of task.windows) {
            Data.call("windows.close", {
                id: window.id
            });
        }
    }

    function setPinned(task, wanted) {
        Data.call(wanted ? "bar.pin" : "bar.unpin", {
            app: task.appId
        });
    }

    Loader {
        id: view

        anchors.fill: parent
        sourceComponent: root.settings?.style === "tabs" ? tabs : icons
    }

    Component {
        id: icons

        IconTasks {
            host: root
        }
    }

    Component {
        id: tabs

        TabTasks {
            host: root
        }
    }
}
