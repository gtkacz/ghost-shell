import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const SCRIPT_URL = new URL('../../kwin/ghost-shell-desktop/contents/code/main.js', import.meta.url);
// Stands in for KWin's ClientAreaOption, which main.qml passes in.
const MAXIMIZE_AREA = 'MaximizeArea';
const DESKTOP_1 = { id: 'desktop-1' };
const DESKTOP_2 = { id: 'desktop-2' };
const ACTIVITY = 'activity-1';
const OTHER_ACTIVITY = 'activity-2';
// DP-1 at scale 1.15, as on the user's machine: logical coordinates are fractional.
const DP1_WIDTH = 2560 / 1.15;
const DP1_HEIGHT = 1440 / 1.15;
const PANEL_HEIGHT = 44;
const DP1_SCREEN = { x: 0, y: 0, width: DP1_WIDTH, height: DP1_HEIGHT };
const DP1 = { name: 'DP-1', area: { x: 0, y: 0, width: DP1_WIDTH, height: DP1_HEIGHT - PANEL_HEIGHT } };
const HDMI = { name: 'HDMI-A-1', area: { x: DP1_WIDTH, y: 0, width: 1920, height: 1080 } };
const WORKSPACE_SIGNALS = ['windowAdded', 'windowRemoved', 'windowActivated', 'currentDesktopChanged', 'currentActivityChanged', 'screensChanged'];
const COVER_SIGNALS = ['frameGeometryChanged', 'minimizedChanged', 'opacityChanged', 'desktopsChanged', 'activitiesChanged', 'fullScreenChanged'];
const WINDOW_SIGNALS = [...COVER_SIGNALS, 'hiddenByShowDesktopChanged', 'stackingOrderChanged'];

// QML binds a connection to the object passed before the handler and drops it when that object is destroyed.
function makeSignal() {
  const connections = [];
  return {
    connect(...args) {
      connections.push({ owner: args.length > 1 ? args[0] : null, handler: args.at(-1) });
    },
    emit(...values) {
      connections.filter(({ owner }) => !owner?.destroyed).forEach(({ handler }) => handler(...values));
    },
  };
}

function makeWindow(fields = {}) {
  const window = {
    normalWindow: true,
    dialog: false,
    desktopWindow: false,
    dock: false,
    deleted: false,
    minimized: false,
    fullScreen: false,
    opacity: 1,
    onAllDesktops: false,
    desktops: [DESKTOP_1],
    activities: [ACTIVITY],
    resourceClass: 'org.kde.konsole',
    frameGeometry: { x: 100, y: 100, width: 800, height: 600 },
    ...fields,
  };
  for (const name of WINDOW_SIGNALS) {
    window[name] = makeSignal();
  }
  return window;
}

const desktopWindow = (resourceClass, fields = {}) =>
  makeWindow({ normalWindow: false, desktopWindow: true, onAllDesktops: true, desktops: [], resourceClass, frameGeometry: DP1_SCREEN, ...fields });

function makeKWin({ windows = [], screens = [DP1], activeWindow = null } = {}) {
  const kwin = { owner: { destroyed: false }, maximizeArea: MAXIMIZE_AREA, reports: [], raised: [], showingDesktopQueries: 0 };
  kwin.workspace = {
    currentDesktop: DESKTOP_1,
    currentActivity: ACTIVITY,
    activeWindow,
    stackingOrder: windows,
    screens,
    clientArea(option, output, desktop) {
      assert.equal(option, MAXIMIZE_AREA);
      assert.equal(desktop, kwin.workspace.currentDesktop);
      return output.area;
    },
    raiseWindow(window) {
      kwin.raised.push(window);
      kwin.workspace.stackingOrder = [...kwin.workspace.stackingOrder.filter((other) => other !== window), window];
    },
  };
  for (const name of WORKSPACE_SIGNALS) {
    kwin.workspace[name] = makeSignal();
  }
  kwin.sendReport = (covered, active) => kwin.reports.push({ covered, active });
  kwin.queryShowingDesktop = () => {
    kwin.showingDesktopQueries += 1;
  };
  return kwin;
}

function loadScript(kwin) {
  const script = vm.createContext({});
  vm.runInContext(readFileSync(SCRIPT_URL, 'utf8'), script, { filename: SCRIPT_URL.pathname });
  script.start(kwin);
  return script;
}

const lastReport = (kwin) => kwin.reports.at(-1);
const halves = (gap) => [
  makeWindow({ frameGeometry: { x: 0, y: 0, width: DP1_WIDTH / 2 - gap, height: DP1_HEIGHT } }),
  makeWindow({ frameGeometry: { x: DP1_WIDTH / 2, y: 0, width: DP1_WIDTH / 2, height: DP1_HEIGHT } }),
];

test('a fullscreen window covers its output', () => {
  const fullscreen = makeWindow({ fullScreen: true, frameGeometry: DP1_SCREEN });
  const kwin = makeKWin({ windows: [fullscreen], activeWindow: fullscreen });
  loadScript(kwin);
  assert.deepEqual(lastReport(kwin), { covered: 'DP-1', active: false });
});

test('two half-screen windows together cover', () => {
  const kwin = makeKWin({ windows: halves(0) });
  loadScript(kwin);
  assert.equal(lastReport(kwin).covered, 'DP-1');
});

test('a one-pixel gap is not covered', () => {
  const kwin = makeKWin({ windows: halves(1) });
  loadScript(kwin);
  assert.equal(lastReport(kwin).covered, '');
});

test('four windows around a hole do not cover', () => {
  const script = loadScript(makeKWin());
  const area = { x: 0, y: 0, width: 300, height: 300 };
  const frame = [
    { x: 0, y: 0, width: 300, height: 100 },
    { x: 0, y: 200, width: 300, height: 100 },
    { x: 0, y: 0, width: 100, height: 300 },
    { x: 200, y: 0, width: 100, height: 300 },
  ];
  assert.equal(script.coveredBy(area, frame), false);
  assert.equal(script.coveredBy(area, [...frame, { x: 90, y: 90, width: 120, height: 120 }]), true);
});

test('minimised, translucent and other-desktop windows do not cover', () => {
  const kwin = makeKWin({
    windows: [
      makeWindow({ frameGeometry: DP1_SCREEN, minimized: true }),
      makeWindow({ frameGeometry: DP1_SCREEN, opacity: 0.9 }),
      makeWindow({ frameGeometry: DP1_SCREEN, desktops: [DESKTOP_2] }),
      makeWindow({ frameGeometry: DP1_SCREEN, activities: [OTHER_ACTIVITY] }),
    ],
  });
  loadScript(kwin);
  assert.equal(lastReport(kwin).covered, '');
});

test('a window on every desktop and activity covers', () => {
  const kwin = makeKWin({ windows: [makeWindow({ frameGeometry: DP1_SCREEN, onAllDesktops: true, desktops: [], activities: [] })] });
  loadScript(kwin);
  assert.equal(lastReport(kwin).covered, 'DP-1');
});

test('a dialog covers like a normal window', () => {
  const kwin = makeKWin({ windows: [makeWindow({ frameGeometry: DP1_SCREEN, normalWindow: false, dialog: true })] });
  loadScript(kwin);
  assert.equal(lastReport(kwin).covered, 'DP-1');
});

test('desktop windows and docks never cover', () => {
  const kwin = makeKWin({
    windows: [
      desktopWindow('plasmashell'),
      desktopWindow('quickshell'),
      makeWindow({ normalWindow: false, dock: true, frameGeometry: DP1_SCREEN }),
    ],
  });
  loadScript(kwin);
  assert.equal(lastReport(kwin).covered, '');
});

test('a closing window does not cover', () => {
  const kwin = makeKWin({ windows: [makeWindow({ frameGeometry: DP1_SCREEN, deleted: true })] });
  loadScript(kwin);
  assert.equal(lastReport(kwin).covered, '');
});

test('each output is judged on its own', () => {
  const spanning = makeWindow({ frameGeometry: { x: 0, y: 0, width: DP1_WIDTH + 1920, height: DP1_HEIGHT } });
  const kwin = makeKWin({ windows: [makeWindow({ frameGeometry: DP1_SCREEN })], screens: [DP1, HDMI] });
  loadScript(kwin);
  assert.equal(lastReport(kwin).covered, 'DP-1');

  kwin.workspace.stackingOrder.push(spanning);
  kwin.workspace.windowAdded.emit(spanning);
  assert.equal(lastReport(kwin).covered, 'DP-1,HDMI-A-1');
});

test('show desktop uncovers and counts as on the desktop', () => {
  const maximised = makeWindow({ frameGeometry: DP1.area });
  const kwin = makeKWin({ windows: [maximised], activeWindow: maximised });
  const script = loadScript(kwin);
  assert.deepEqual(lastReport(kwin), { covered: 'DP-1', active: false });

  script.setShowingDesktop(true);
  assert.deepEqual(lastReport(kwin), { covered: '', active: true });

  script.setShowingDesktop(false);
  assert.deepEqual(lastReport(kwin), { covered: 'DP-1', active: false });
});

test('show desktop is asked of KWin on load, when a window opens and when show desktop hides one', () => {
  const window = makeWindow();
  const kwin = makeKWin({ windows: [window] });
  loadScript(kwin);
  assert.equal(kwin.showingDesktopQueries, 1);

  window.hiddenByShowDesktopChanged.emit();
  assert.equal(kwin.showingDesktopQueries, 2);

  const opened = makeWindow();
  kwin.workspace.stackingOrder.push(opened);
  kwin.workspace.windowAdded.emit(opened);
  assert.equal(kwin.showingDesktopQueries, 3);

  opened.hiddenByShowDesktopChanged.emit();
  assert.equal(kwin.showingDesktopQueries, 4);
});

test('active when nothing or a desktop window is focused', () => {
  const window = makeWindow();
  const plasma = desktopWindow('plasmashell');
  const kwin = makeKWin({ windows: [plasma, window] });
  loadScript(kwin);
  assert.equal(lastReport(kwin).active, true);

  kwin.workspace.activeWindow = window;
  kwin.workspace.windowActivated.emit(window);
  assert.equal(lastReport(kwin).active, false);

  kwin.workspace.activeWindow = plasma;
  kwin.workspace.windowActivated.emit(plasma);
  assert.equal(lastReport(kwin).active, true);
});

test('reports only on change', () => {
  const window = makeWindow();
  const kwin = makeKWin({ windows: [window] });
  loadScript(kwin);
  assert.equal(kwin.reports.length, 1);

  window.frameGeometryChanged.emit(window.frameGeometry);
  window.frameGeometryChanged.emit(window.frameGeometry);
  assert.equal(kwin.reports.length, 1);

  window.frameGeometry = DP1_SCREEN;
  window.frameGeometryChanged.emit();
  window.frameGeometryChanged.emit();
  assert.equal(kwin.reports.length, 2);
});

test('reports the current state once on load', () => {
  const fullscreen = makeWindow({ fullScreen: true, frameGeometry: DP1_SCREEN });
  const kwin = makeKWin({ windows: [fullscreen], activeWindow: fullscreen });
  loadScript(kwin);
  assert.deepEqual(kwin.reports, [{ covered: 'DP-1', active: false }]);
});

test('the first report is sent even when it matches the daemon default', () => {
  const window = makeWindow();
  const kwin = makeKWin({ windows: [window], activeWindow: window });
  loadScript(kwin);
  assert.deepEqual(kwin.reports, [{ covered: '', active: false }]);
});

test('every trigger reports a change', () => {
  const window = makeWindow();
  const kwin = makeKWin({ windows: [window] });
  loadScript(kwin);
  const triggers = [
    ...WORKSPACE_SIGNALS.map((name) => kwin.workspace[name]),
    ...COVER_SIGNALS.map((name) => window[name]),
  ];
  for (const trigger of triggers) {
    const before = kwin.reports.length;
    kwin.workspace.activeWindow = kwin.workspace.activeWindow === null ? window : null;
    trigger.emit(makeWindow());
    assert.equal(kwin.reports.length, before + 1);
  }
});

test('keeper raises quickshell desktop windows above plasmashell', () => {
  const dashboard = desktopWindow('quickshell');
  const plasma = desktopWindow('plasmashell');
  const kwin = makeKWin({ windows: [dashboard, plasma] });
  loadScript(kwin);
  assert.deepEqual(kwin.raised, [dashboard]);

  kwin.workspace.stackingOrder = [plasma, dashboard];
  plasma.stackingOrderChanged.emit();
  assert.deepEqual(kwin.raised, [dashboard]);

  kwin.workspace.stackingOrder = [dashboard, plasma];
  dashboard.stackingOrderChanged.emit();
  assert.deepEqual(kwin.raised, [dashboard, dashboard]);
});

test('keeper leaves other quickshell windows and closing ones alone', () => {
  const closing = desktopWindow('quickshell', { deleted: true });
  const popup = makeWindow({ resourceClass: 'quickshell' });
  const kwin = makeKWin({ windows: [closing, popup, desktopWindow('plasmashell')] });
  loadScript(kwin);
  assert.deepEqual(kwin.raised, []);
});

test('unloading reports nothing covered and off the desktop, then stays quiet', () => {
  const dashboard = desktopWindow('quickshell');
  const plasma = desktopWindow('plasmashell');
  const window = makeWindow({ frameGeometry: DP1_SCREEN });
  const kwin = makeKWin({ windows: [plasma, dashboard, window], activeWindow: plasma });
  const script = loadScript(kwin);
  assert.deepEqual(lastReport(kwin), { covered: 'DP-1', active: true });
  const reportsBefore = kwin.reports.length;

  script.stop();
  assert.deepEqual(kwin.reports.slice(reportsBefore), [{ covered: '', active: false }]);

  kwin.owner.destroyed = true;
  kwin.workspace.activeWindow = null;
  window.minimized = true;
  kwin.workspace.stackingOrder = [dashboard, plasma, window];
  for (const name of WORKSPACE_SIGNALS) {
    kwin.workspace[name].emit(makeWindow());
  }
  for (const name of WINDOW_SIGNALS) {
    window[name].emit();
    dashboard[name].emit();
  }
  assert.equal(kwin.reports.length, reportsBefore + 1);
  assert.deepEqual(kwin.raised, []);
  assert.equal(kwin.showingDesktopQueries, 1);
});
