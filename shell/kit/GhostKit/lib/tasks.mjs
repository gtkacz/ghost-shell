const MAX_MARKS = 4;
const UNMATCHED_PREFIX = 'class:';
const NO_BADGE = Object.freeze({ count: null, progress: null, urgent: false });

// The app a window belongs to, by the plan's order: its desktop file name, an entry's startup class, the lower-cased
// resource class, and last the desktop entries' own guess. Lookups are injected so the order is testable.
export function matchOrder(window, { hasId, byStartupClass, heuristic }) {
  if (hasId(window.app_id)) {
    return window.app_id;
  }
  const fromStartupClass = byStartupClass(window.resource_class);
  if (fromStartupClass) {
    return fromStartupClass;
  }
  const lowered = window.resource_class.toLowerCase();
  if (hasId(lowered)) {
    return lowered;
  }
  return heuristic(window.resource_class) ?? null;
}

// Pinned apps in config order, then running unpinned apps in the order their first window appears; the windows
// arrive most recently used first and keep that order within an app.
export function buildTasks({ pinned, windows, findEntry }) {
  const tasks = [];
  const byApp = new Map();
  const taskOf = (appId, isPinned) => {
    if (!byApp.has(appId)) {
      const task = { appId, pinned: isPinned, windows: [], focused: false };
      byApp.set(appId, task);
      tasks.push(task);
    }
    return byApp.get(appId);
  };
  for (const appId of pinned) {
    taskOf(appId, true);
  }
  for (const window of windows) {
    const task = taskOf(findEntry(window) ?? UNMATCHED_PREFIX + window.resource_class, false);
    task.windows.push(window);
    task.focused = task.focused || window.active;
  }
  return tasks;
}

export function clickAction(task) {
  if (task.windows.length === 0) {
    return { kind: 'launch' };
  }
  if (task.windows.length > 1) {
    return { kind: 'list' };
  }
  const [only] = task.windows;
  return { kind: only.active ? 'minimize' : 'activate', id: only.id };
}

export function marks(task) {
  return { count: Math.min(task.windows.length, MAX_MARKS), focusedFirst: task.focused };
}

export function badge(entries, appId) {
  const entry = entries?.[appId];
  return entry ? { count: entry.count ?? null, progress: entry.progress ?? null, urgent: entry.urgent === true } : NO_BADGE;
}

// The pinned app a task shortcut asks for, or null past the end of the pins or for a press that names none.
export function pinnedTask(tasks, index) {
  const task = Number.isInteger(index) ? tasks[index] : undefined;
  return task?.pinned ? task : null;
}

const TAB_PX = 128;
const MORE_PX = 32;

// The windows that fit as tabs in the given width, keeping the feed's most-recently-used order. Once any window is
// hidden the "+N" label needs its own room, so a tab gives way to it.
export function tabsFor(windows, availablePx, tabPx = TAB_PX, morePx = MORE_PX) {
  const fitting = Math.max(0, Math.floor(availablePx / tabPx));
  if (windows.length <= fitting) {
    return { shown: windows, hidden: 0 };
  }
  const shownCount = Math.max(0, Math.floor((availablePx - morePx) / tabPx));
  return { shown: windows.slice(0, shownCount), hidden: windows.length - shownCount };
}

export function tabClick(window) {
  return { kind: window.active ? 'minimize' : 'activate', id: window.id };
}

export function pad2(number) {
  return String(number).padStart(2, '0');
}
