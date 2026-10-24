import test from 'node:test';
import assert from 'node:assert/strict';
import { buildTasks, clickAction, marks, badge, matchOrder, pinnedTask, tabsFor, tabClick, pad2 } from '../kit/GhostKit/lib/tasks.mjs';

const win = (id, appId, extra = {}) => ({ id, app_id: appId, resource_class: appId, caption: id, active: false, minimized: false, ...extra });
const byAppId = (window) => window.app_id;

test('pinned first then running unpinned', () => {
  const tasks = buildTasks({ pinned: ['a', 'b'], windows: [win('1', 'c'), win('2', 'a')], findEntry: byAppId });
  assert.deepEqual(tasks.map((task) => [task.appId, task.pinned, task.windows.length]), [['a', true, 1], ['b', true, 0], ['c', false, 1]]);
});

test('window joins its pinned app through startup class', () => {
  const steam = win('1', 'Steam');
  const tasks = buildTasks({ pinned: ['steam'], windows: [steam], findEntry: (window) => (window.app_id === 'Steam' ? 'steam' : null) });
  assert.deepEqual(tasks.map((task) => [task.appId, task.windows.length]), [['steam', 1]]);
});

test('unmatched window is its own task', () => {
  const tasks = buildTasks({ pinned: [], windows: [win('1', 'x', { resource_class: 'Odd' })], findEntry: () => null });
  assert.equal(tasks[0].appId, 'class:Odd');
  assert.equal(tasks[0].pinned, false);
});

test('windows of one app stay in most-recently-used order and flag focus', () => {
  const tasks = buildTasks({ pinned: [], windows: [win('1', 'a'), win('2', 'b'), win('3', 'a', { active: true })], findEntry: byAppId });
  assert.deepEqual(tasks.map((task) => task.appId), ['a', 'b']);
  assert.deepEqual(tasks[0].windows.map((w) => w.id), ['1', '3']);
  assert.deepEqual(tasks.map((task) => task.focused), [true, false]);
});

test('click on an app with no window launches', () => {
  assert.deepEqual(clickAction({ windows: [] }), { kind: 'launch' });
});

test('click on a single window activates it', () => {
  assert.deepEqual(clickAction({ windows: [win('1', 'a')] }), { kind: 'activate', id: '1' });
});

test('click on the focused single window minimizes it', () => {
  assert.deepEqual(clickAction({ windows: [win('1', 'a', { active: true })] }), { kind: 'minimize', id: '1' });
});

test('click on an app with two windows opens the list, focused or not', () => {
  assert.deepEqual(clickAction({ windows: [win('1', 'a'), win('2', 'a')] }), { kind: 'list' });
  assert.deepEqual(clickAction({ windows: [win('1', 'a', { active: true }), win('2', 'a')] }), { kind: 'list' });
});

test('marks cap at four and flag the focused app', () => {
  const many = Array.from({ length: 6 }, (_, i) => win(String(i), 'a'));
  assert.deepEqual(marks({ windows: many, focused: true }), { count: 4, focusedFirst: true });
  assert.deepEqual(marks({ windows: many.slice(0, 2), focused: false }), { count: 2, focusedFirst: false });
});

test('badge hides a count the app made invisible', () => {
  const entries = { a: { count: null, progress: 0.5, urgent: true }, b: { count: 3, progress: null, urgent: false } };
  assert.deepEqual(badge(entries, 'a'), { count: null, progress: 0.5, urgent: true });
  assert.deepEqual(badge(entries, 'b'), { count: 3, progress: null, urgent: false });
  assert.deepEqual(badge(entries, 'zzz'), { count: null, progress: null, urgent: false });
  assert.deepEqual(badge(null, 'a'), { count: null, progress: null, urgent: false });
});

test('task shortcut beyond the pinned list does nothing', () => {
  const tasks = buildTasks({ pinned: ['a', 'b', 'c'], windows: [win('1', 'z')], findEntry: byAppId });
  assert.equal(pinnedTask(tasks, 5), null);
  assert.equal(pinnedTask(tasks, 3), null);
  assert.equal(pinnedTask(tasks, 1).appId, 'b');
  assert.equal(pinnedTask(tasks, null), null);
});

test('matching tries the desktop file name, the startup class, the lower-cased class, then the heuristic', () => {
  const lookups = {
    hasId: (id) => id === 'org.kde.dolphin' || id === 'firefox',
    byStartupClass: (cls) => (cls.toLowerCase() === 'steamwebhelper' ? 'steam' : null),
    heuristic: (cls) => (cls === 'Weird' ? 'weird-app' : null),
  };
  const of = (appId, resourceClass) => matchOrder({ app_id: appId, resource_class: resourceClass }, lookups);
  assert.equal(of('org.kde.dolphin', 'dolphin'), 'org.kde.dolphin');
  assert.equal(of('SteamWebHelper', 'SteamWebHelper'), 'steam');
  assert.equal(of('Firefox', 'Firefox'), 'firefox');
  assert.equal(of('Weird', 'Weird'), 'weird-app');
  assert.equal(of('nothing', 'nothing'), null);
});

const windowsOf = (count) => Array.from({ length: count }, (_, i) => win(String(i + 1), 'a'));

test('tabs keep the windows in the order the feed gives, most recently activated first', () => {
  const { shown } = tabsFor([win('3', 'a'), win('1', 'a'), win('2', 'a')], 1000);
  assert.deepEqual(shown.map((window) => window.id), ['3', '1', '2']);
});

test('tabs overflow counts hidden windows', () => {
  const { shown, hidden } = tabsFor(windowsOf(20), 600);
  assert.equal(shown.length, 4);
  assert.equal(hidden, 16);
});

test('no overflow label when everything fits', () => {
  const { shown, hidden } = tabsFor(windowsOf(4), 4 * 128);
  assert.equal(shown.length, 4);
  assert.equal(hidden, 0);
});

test('tabs leave room for the overflow label once one is hidden', () => {
  const { shown, hidden } = tabsFor(windowsOf(5), 4 * 128);
  assert.equal(shown.length, 3);
  assert.equal(hidden, 2);
});

test('a bar too narrow for one tab and the label shows none', () => {
  assert.deepEqual(tabsFor(windowsOf(3), 100), { shown: [], hidden: 3 });
});

test('clicking the active tab minimizes it', () => {
  assert.deepEqual(tabClick(win('1', 'a', { active: true })), { kind: 'minimize', id: '1' });
  assert.deepEqual(tabClick(win('2', 'a')), { kind: 'activate', id: '2' });
});

test('index is two digits', () => {
  assert.equal(pad2(1), '01');
  assert.equal(pad2(12), '12');
});

test('a pinned app claims the window its entry names by startup class, over a decoy with the same class', () => {
  const lookups = {
    hasId: (id) => id === 'thorium-browser' || id === 'thorium-shell',
    byStartupClass: (cls) => (cls.toLowerCase() === 'thorium' ? 'thorium-shell' : null),
    heuristic: () => null,
    pinned: [{ id: 'thorium-browser', startupClass: 'thorium' }],
  };
  const thorium = win('1', 'thorium');
  const tasks = buildTasks({ pinned: ['thorium-browser'], windows: [thorium], findEntry: (window) => matchOrder(window, lookups) });
  assert.deepEqual(tasks.map((task) => [task.appId, task.pinned, task.windows.length]), [['thorium-browser', true, 1]]);
});

test('a pinned app also claims a window whose id or lower-cased class equals its id', () => {
  const lookups = { hasId: () => false, byStartupClass: () => null, heuristic: () => 'other', pinned: [{ id: 'kate', startupClass: '' }] };
  assert.equal(matchOrder({ app_id: 'Kate', resource_class: 'x' }, lookups), 'kate');
  assert.equal(matchOrder({ app_id: 'y', resource_class: 'KATE' }, lookups), 'kate');
});

test('duplicate pins keep first occurrences and do not shift task shortcut indices', () => {
  const tasks = buildTasks({ pinned: ['a', 'b', 'a', 'c', 'b'], windows: [], findEntry: byAppId });
  assert.deepEqual(tasks.map((task) => task.appId), ['a', 'b', 'c']);
  assert.equal(pinnedTask(tasks, 2).appId, 'c');
});
