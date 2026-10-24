import test from 'node:test';
import assert from 'node:assert/strict';
import {
  acceptLauncherSeq,
  category,
  dedupe,
  disarm,
  highlight,
  isActivationRepeat,
  initialPower,
  move,
  power,
  rows,
  rowOf,
  rank,
  sections,
} from '../kit/GhostKit/lib/launcher.mjs';

const USER_DIR = '/home/u/.local/share';
const app = (id, name, extra = {}) => ({
  id,
  name,
  genericName: '',
  keywords: [],
  categories: [],
  command: id,
  noDisplay: false,
  path: `/usr/share/applications/${id}.desktop`,
  ...extra,
});
const names = (entries) => entries.map((entry) => entry.name);

const APPS = [
  app('firefox', 'Firefox', { genericName: 'Web Browser' }),
  app('firewall', 'Firewall Config'),
  app('thorium', 'Thorium', { genericName: 'Web Browser' }),
  app('chrome', 'Google Chrome', { keywords: ['browser'] }),
  app('safe', 'Wildfire Tools'),
  app('kate', 'Kate', { genericName: 'Text Editor' }),
];

test('name prefix beats word prefix beats keyword beats substring', () => {
  const entries = [app('d', 'Wildfire'), app('c', 'Tool', { keywords: ['fireplace'] }), app('b', 'My Firebox'), app('a', 'Firefox')];
  assert.deepEqual(names(rank(entries, 'fire')), ['Firefox', 'My Firebox', 'Tool', 'Wildfire']);
});

test('name prefix ties sort by name', () => {
  assert.deepEqual(names(rank(APPS, 'fire')).slice(0, 2), ['Firefox', 'Firewall Config']);
  assert.equal(names(rank(APPS, 'fire')).at(-1), 'Wildfire Tools');
});

test('the query is matched case-insensitively', () => {
  assert.deepEqual(names(rank(APPS, 'FIREF')), ['Firefox']);
});

test('generic name and keywords are searched', () => {
  assert.deepEqual(names(rank(APPS, 'browser')), ['Firefox', 'Google Chrome', 'Thorium']);
});

test('an app that matches nothing is left out', () => {
  assert.deepEqual(rank(APPS, 'zzz'), []);
});

test('duplicates merge user over system over flatpak', () => {
  const flatpak = app('org.kde.dolphin', 'Dolphin', { command: 'dolphin', path: '/var/lib/flatpak/exports/share/applications/org.kde.dolphin.desktop' });
  const system = app('dolphin-sys', 'Dolphin', { command: 'dolphin' });
  const user = app('dolphin-user', 'dolphin', { command: 'dolphin', path: `${USER_DIR}/applications/dolphin-user.desktop` });
  for (const order of [[flatpak, system, user], [user, flatpak, system], [system, user, flatpak]]) {
    assert.deepEqual(dedupe(order, USER_DIR).map((entry) => entry.id), ['dolphin-user']);
  }
  assert.deepEqual(dedupe([flatpak, system], USER_DIR).map((entry) => entry.id), ['dolphin-sys']);
});

test('a user flatpak export ranks as flatpak', () => {
  const exported = app('a', 'Thing', { command: 'thing', path: `${USER_DIR}/flatpak/exports/share/applications/a.desktop` });
  const system = app('b', 'Thing', { command: 'thing' });
  assert.deepEqual(dedupe([exported, system], USER_DIR).map((entry) => entry.id), ['b']);
});

test('apps with the same name but another command stay apart', () => {
  const entries = [app('a', 'Terminal', { command: 'konsole' }), app('b', 'Terminal', { command: 'xterm' })];
  assert.equal(dedupe(entries, USER_DIR).length, 2);
});

test('noDisplay entries are dropped', () => {
  assert.deepEqual(dedupe([app('a', 'Shown'), app('b', 'Hidden', { noDisplay: true })], USER_DIR).map((entry) => entry.id), ['a']);
});

test('categories map to chips', () => {
  assert.equal(category(['Qt', 'KDE', 'Game']), 'GAME');
  assert.equal(category(['Utility']), 'UTILITY');
  assert.equal(category([]), 'UTILITY');
  assert.equal(category(['System', 'Network']), 'NETWORK');
  assert.equal(category(['Audio']), 'MEDIA');
  assert.equal(category(['Video']), 'MEDIA');
  assert.equal(category(['AudioVideo']), 'MEDIA');
  assert.equal(category(['Development', 'Game']), 'GAME');
  for (const [name, chip] of [['Development', 'DEV'], ['Graphics', 'GRAPHICS'], ['Office', 'OFFICE'], ['Settings', 'SETTINGS'], ['System', 'SYSTEM']]) {
    assert.equal(category([name]), chip);
  }
});

const pins = [app('p1', 'P1'), app('p2', 'P2')];
const recent = [{ name: 'a.txt', path: '/a.txt' }];

test('empty query lists pins then recent then all', () => {
  const items = sections({ query: '', pinned: pins, recent, all: [app('z', 'Zed'), app('a', 'Alpha')] });
  assert.deepEqual(items.map((item) => item.kind), ['pin', 'pin', 'file', 'app', 'app']);
  assert.deepEqual(items.filter((item) => item.kind === 'app').map((item) => item.entry.name), ['Alpha', 'Zed']);
});

test('a query lists only ranked apps', () => {
  const items = sections({ query: 'fire', pinned: pins, recent, all: APPS });
  assert.deepEqual(items.map((item) => item.kind), ['app', 'app', 'app']);
  assert.equal(items[0].entry.name, 'Firefox');
});

const grid = (index, pinCount, total, key) => move({ index, pinCount, total }, key);

test('down moves a row in the pin grid then one row at a time', () => {
  assert.equal(grid(0, 11, 30, 'Down'), 6);
  assert.equal(grid(6, 11, 30, 'Down'), 11);
  assert.equal(grid(5, 11, 30, 'Down'), 11);
  assert.equal(grid(11, 11, 30, 'Down'), 12);
  assert.equal(grid(29, 11, 30, 'Down'), 29);
});

test('up moves a row in the grid and from the list back to the last pin', () => {
  assert.equal(grid(6, 11, 30, 'Up'), 0);
  assert.equal(grid(3, 11, 30, 'Up'), 3);
  assert.equal(grid(11, 11, 30, 'Up'), 10);
  assert.equal(grid(12, 11, 30, 'Up'), 11);
  assert.equal(grid(0, 0, 5, 'Up'), 0);
});

test('left and right move one pin and stop at the grid edges', () => {
  assert.equal(grid(2, 11, 30, 'Right'), 3);
  assert.equal(grid(10, 11, 30, 'Right'), 10);
  assert.equal(grid(0, 11, 30, 'Left'), 0);
  assert.equal(grid(4, 11, 30, 'Left'), 3);
  assert.equal(grid(15, 11, 30, 'Left'), 15);
});

test('down in a grid with no list after it stays put', () => {
  assert.equal(grid(3, 4, 4, 'Down'), 3);
});

const MS = 1000;
test('power needs a second press within three seconds', () => {
  const armed = power(initialPower, 'power', 0);
  assert.equal(armed.fire, null);
  assert.equal(armed.state.armed, 'power');
  assert.equal(power(armed.state, 'power', 2999).fire, 'power');
  const late = power(armed.state, 'power', 3001);
  assert.equal(late.fire, null);
  assert.equal(late.state.armed, 'power');
  assert.equal(late.state.armedAtMs, 3001);
  assert.equal(power(armed.state, 'power', 3 * MS).fire, 'power');
});

test('a second press inside the guard window does not confirm', () => {
  const armed = power(initialPower, 'restart', 0).state;
  const repeated = power(armed, 'restart', 100);
  assert.equal(repeated.fire, null);
  assert.equal(repeated.state.armedAtMs, 0);
  assert.equal(power(repeated.state, 'restart', 249).fire, null);
  assert.equal(power(repeated.state, 'restart', 250).fire, 'restart');
});

test('a held Return never confirms, at 0, 30 and 600 ms', () => {
  let state = initialPower;
  let fired = null;
  for (const [atMs, isAutoRepeat] of [[0, false], [30, true], [600, true]]) {
    if (!isActivationRepeat('Return', isAutoRepeat)) {
      const result = power(state, 'shutdown', atMs);
      state = result.state;
      fired = result.fire ?? fired;
    }
  }
  assert.equal(fired, null);
  assert.equal(state.armed, 'shutdown');
});

test('only a repeated Return is an activation repeat', () => {
  assert.equal(isActivationRepeat('Return', true), true);
  assert.equal(isActivationRepeat('Return', false), false);
  assert.equal(isActivationRepeat('Down', true), false);
});

test('a different action re-arms', () => {
  const armed = power(initialPower, 'restart', 0).state;
  const other = power(armed, 'power', 100);
  assert.equal(other.fire, null);
  assert.equal(other.state.armed, 'power');
});

test('lock fires at once', () => {
  const result = power(initialPower, 'lock', 0);
  assert.equal(result.fire, 'lock');
  assert.equal(result.state.armed, null);
});

test('firing disarms', () => {
  const fired = power(power(initialPower, 'logout', 0).state, 'logout', 500);
  assert.equal(fired.fire, 'logout');
  assert.equal(fired.state.armed, null);
});

test('closing disarms', () => {
  const armed = power(initialPower, 'power', 0).state;
  const reopened = disarm(armed);
  assert.equal(power(reopened, 'power', 1000).fire, null);
});

test('highlight splits the matched part of a name', () => {
  assert.deepEqual(highlight('Firefox', 'fire'), ['', 'Fire', 'fox']);
  assert.deepEqual(highlight('Wildfire', 'FIRE'), ['Wild', 'fire', '']);
  assert.deepEqual(highlight('Kate', 'zz'), ['Kate', '', '']);
  assert.deepEqual(highlight('Kate', ''), ['Kate', '', '']);
});

test('the first desktop value on a connection only sets the baseline', () => {
  const first = acceptLauncherSeq(null, { launcher_seq: 4 }, 1);
  assert.equal(first.toggle, false);
  const next = acceptLauncherSeq(first.seen, { launcher_seq: 5 }, 1);
  assert.equal(next.toggle, true);
  assert.equal(acceptLauncherSeq(next.seen, { launcher_seq: 5 }, 1).toggle, false);
  assert.equal(acceptLauncherSeq(next.seen, { launcher_seq: 1 }, 2).toggle, false);
});

test('rows give each group a heading and the pins one grid row', () => {
  const items = sections({ query: '', pinned: pins, recent, all: [app('z', 'Zed'), app('a', 'Alpha')] });
  const list = rows(items, '');
  assert.deepEqual(list.map((row) => row.type), ['head', 'pins', 'head', 'file', 'head', 'app', 'app']);
  assert.deepEqual(list[1], { type: 'pins', from: 0, count: 2 });
  assert.equal(rowOf(list, 1), 1);
  assert.equal(rowOf(list, 2), 3);
  assert.equal(rowOf(list, 4), 6);
});

test('rows for a query are one results group with its count', () => {
  const list = rows(sections({ query: 'fire', pinned: pins, recent, all: APPS }), 'fire');
  assert.deepEqual(list[0], { type: 'head', label: 'RESULTS', tag: 'results', count: 3 });
  assert.equal(list.length, 4);
});

test('rows are empty when nothing matches', () => {
  assert.deepEqual(rows([], 'zzz'), []);
});
