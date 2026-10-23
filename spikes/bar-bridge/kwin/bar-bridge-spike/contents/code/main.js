// The ghost-shell-desktop KWin script's logic. ../ui/main.qml passes KWin's objects to start(), so
// the tests can load this file with stand-ins.

const DASHBOARD_CLASS = "quickshell";
const PLASMA_DESKTOP_CLASS = "plasmashell";
const OUTPUT_SEPARATOR = ",";
const OPAQUE = 1;
const COVER_CHANGES = [
    "frameGeometryChanged",
    "minimizedChanged",
    "opacityChanged",
    "desktopsChanged",
    "activitiesChanged",
    "fullScreenChanged",
];
const WORKSPACE_CHANGES = [
    "windowRemoved",
    "windowActivated",
    "currentDesktopChanged",
    "currentActivityChanged",
    "screensChanged",
];
const TASK_CHANGES = [
    "captionChanged",
    "minimizedChanged",
    "fullScreenChanged",
    "demandsAttentionChanged",
    "skipTaskbarChanged",
];

let kwin = null;
// Taskbar windows, most recently activated first.
let mru = [];
let lastWindows = null;
let showingDesktop = false;
let lastReport = null;

// KWin's logical coordinates are fractional under fractional scaling; rounding each edge rather than
// each size keeps edges that meet meeting.
function edges(rect) {
    return {
        left: Math.round(rect.x),
        top: Math.round(rect.y),
        right: Math.round(rect.x + rect.width),
        bottom: Math.round(rect.y + rect.height),
    };
}

function isEmpty(piece) {
    return piece.left >= piece.right || piece.top >= piece.bottom;
}

// The parts of `piece` outside `cut`: the bands above and below it, then left and right of it.
function subtract(piece, cut) {
    const overlap = {
        left: Math.max(piece.left, cut.left),
        top: Math.max(piece.top, cut.top),
        right: Math.min(piece.right, cut.right),
        bottom: Math.min(piece.bottom, cut.bottom),
    };
    if (isEmpty(overlap)) {
        return [piece];
    }
    return [
        { left: piece.left, top: piece.top, right: piece.right, bottom: overlap.top },
        { left: piece.left, top: overlap.bottom, right: piece.right, bottom: piece.bottom },
        { left: piece.left, top: overlap.top, right: overlap.left, bottom: overlap.bottom },
        { left: overlap.right, top: overlap.top, right: piece.right, bottom: overlap.bottom },
    ].filter((part) => !isEmpty(part));
}

function coveredBy(area, rects) {
    let uncovered = [edges(area)];
    for (const rect of rects) {
        const cut = edges(rect);
        uncovered = uncovered.reduce((parts, piece) => parts.concat(subtract(piece, cut)), []);
    }
    return uncovered.length === 0;
}

function isOnCurrentDesktop(window) {
    return window.onAllDesktops || window.desktops.includes(kwin.workspace.currentDesktop);
}

function isOnCurrentActivity(window) {
    return window.activities.length === 0 || window.activities.includes(kwin.workspace.currentActivity);
}

function countsAsCover(window) {
    // A closing window stays in the stacking order until its close animation ends.
    return (window.normalWindow || window.dialog)
        && !window.deleted
        && !window.minimized
        && window.opacity >= OPAQUE
        && isOnCurrentDesktop(window)
        && isOnCurrentActivity(window);
}

function currentState() {
    const workspace = kwin.workspace;
    const focused = workspace.activeWindow;
    const active = showingDesktop || focused === null || focused.desktopWindow;
    if (showingDesktop) {
        return { covered: [], active };
    }
    const covers = workspace.stackingOrder.filter(countsAsCover).map((window) => window.frameGeometry);
    const covered = workspace.screens
        .filter((output) => {
            const area = workspace.clientArea(kwin.maximizeArea, output, workspace.currentDesktop);
            return coveredBy(area, covers);
        })
        .map((output) => output.name);
    return { covered, active };
}

function report() {
    const state = currentState();
    const covered = state.covered.join(OUTPUT_SEPARATOR);
    if (lastReport !== null && lastReport.covered === covered && lastReport.active === state.active) {
        return;
    }
    lastReport = { covered, active: state.active };
    kwin.sendReport(covered, state.active);
}

function setShowingDesktop(showing) {
    showingDesktop = showing;
    report();
}

function isDashboard(window) {
    return window.desktopWindow && window.resourceClass === DASHBOARD_CLASS;
}

function isPlasmaDesktop(window) {
    return window.desktopWindow && window.resourceClass === PLASMA_DESKTOP_CLASS;
}

// While the dashboard shares KWin's desktop layer with Plasma's desktop, a click on the desktop (input
// passes through the dashboard's mask) raises Plasma's window over the dashboard. KWin raises only
// after emitting windowActivated, so this follows the desktop windows' own stacking changes.
function keepDashboardOnTop() {
    const order = kwin.workspace.stackingOrder.filter((window) => !window.deleted);
    const topPlasma = order.map(isPlasmaDesktop).lastIndexOf(true);
    for (let i = 0; i < topPlasma; i++) {
        if (isDashboard(order[i])) {
            kwin.workspace.raiseWindow(order[i]);
        }
    }
}

// Bound to the script's QML object, which KWin destroys on unload, so no connection outlives the
// script; the workspace and the windows do.
function connect(signal, handler) {
    signal.connect(kwin.owner, handler);
}

function watch(window) {
    if (isDashboard(window) || isPlasmaDesktop(window)) {
        connect(window.stackingOrderChanged, keepDashboardOnTop);
        keepDashboardOnTop();
    }
    // A window's type never changes, so no other window can ever count as cover.
    if (window.normalWindow || window.dialog) {
        for (const change of COVER_CHANGES) {
            connect(window[change], report);
        }
        connect(window.hiddenByShowDesktopChanged, kwin.queryShowingDesktop);
    }
}

// The spike's timing source: the KWin side of each latency is the moment its signal reaches the script.
function log(text) {
    console.info(`bar-bridge ${text} at ${Date.now()}`);
}

function isTask(window) {
    return (window.normalWindow || window.dialog) && !window.deleted;
}

function idOf(window) {
    return window.internalId.toString();
}

function describe(window) {
    return {
        id: idOf(window),
        caption: window.caption,
        appId: window.desktopFileName || window.resourceClass,
        resourceClass: window.resourceClass,
        pid: window.pid,
        active: window === kwin.workspace.activeWindow,
        minimized: window.minimized,
        fullScreen: window.fullScreen,
        skipTaskbar: window.skipTaskbar,
        demandsAttention: window.demandsAttention,
    };
}

function reportWindows() {
    const json = JSON.stringify(mru.map(describe));
    if (json === lastWindows) {
        return;
    }
    lastWindows = json;
    kwin.sendWindows(json);
}

function resendWindows() {
    lastWindows = null;
    reportWindows();
}

function watchTask(window) {
    for (const change of TASK_CHANGES) {
        connect(window[change], () => {
            log(`${change} ${idOf(window)}`);
            reportWindows();
        });
    }
}

function onTaskAdded(window) {
    log(`windowAdded ${idOf(window)}`);
    // KWin activates a new window before it emits windowAdded, so the window may already lead the list.
    if (!mru.includes(window)) {
        mru.push(window);
    }
    watchTask(window);
    reportWindows();
}

function onWindowActivated(window) {
    if (window === null || !isTask(window)) {
        log("windowActivated none");
        reportWindows();
        return;
    }
    log(`windowActivated ${idOf(window)}`);
    mru = [window].concat(mru.filter((other) => other !== window));
    reportWindows();
}

function onWindowRemoved(window) {
    if (!mru.includes(window)) {
        return;
    }
    log(`windowRemoved ${idOf(window)}`);
    mru = mru.filter((other) => other !== window);
    reportWindows();
}

const COMMANDS = {
    activate: (window) => {
        window.minimized = false;
        kwin.workspace.activeWindow = window;
    },
    minimize: (window) => {
        window.minimized = true;
    },
    close: (window) => window.closeWindow(),
};

function handleCommand(json) {
    if (json === "") {
        return;
    }
    let command;
    try {
        command = JSON.parse(json);
    } catch (error) {
        log(`malformed command ${json}`);
        return;
    }
    const act = COMMANDS[command.action];
    const window = kwin.workspace.stackingOrder.find((candidate) => isTask(candidate) && idOf(candidate) === command.id);
    if (act === undefined || window === undefined) {
        log(`ignored command ${json}`);
        return;
    }
    log(`command ${command.action} ${command.id}`);
    act(window);
}

function start(host) {
    kwin = host;
    const active = kwin.workspace.activeWindow;
    const tasks = kwin.workspace.stackingOrder.filter(isTask).reverse();
    mru = tasks.filter((window) => window === active).concat(tasks.filter((window) => window !== active));
    mru.forEach(watchTask);
    connect(kwin.workspace.windowActivated, onWindowActivated);
    connect(kwin.workspace.windowRemoved, onWindowRemoved);
    connect(kwin.workspace.windowAdded, (window) => {
        if (isTask(window)) {
            onTaskAdded(window);
        }
        watch(window);
        // KWin leaves show desktop when a window opens, and no window says so if none was hidden.
        kwin.queryShowingDesktop();
        report();
    });
    for (const change of WORKSPACE_CHANGES) {
        connect(kwin.workspace[change], report);
    }
    kwin.workspace.stackingOrder.forEach(watch);
    kwin.queryShowingDesktop();
    report();
    reportWindows();
}

// An unloaded script can't correct its last report, and ghostd hears from it over KWin's own bus
// connection, which stays.
function stop() {
    kwin.sendReport("", false);
}
