pragma Singleton

import Quickshell
import "lib/tasks.mjs" as TasksLib

// Re-exports the task rules so the tasks widget reaches them through the GhostKit import instead of by file path.
Singleton {
    function build(pinned, windows, findEntry) {
        return TasksLib.buildTasks({
            pinned: pinned,
            windows: windows,
            findEntry: findEntry
        });
    }

    function matchOrder(window, lookups) {
        return TasksLib.matchOrder(window, lookups);
    }

    function click(task) {
        return TasksLib.clickAction(task);
    }

    function marks(task) {
        return TasksLib.marks(task);
    }

    function badge(entries, appId) {
        return TasksLib.badge(entries, appId);
    }

    function pinnedTask(tasks, index) {
        return TasksLib.pinnedTask(tasks, index);
    }
}
