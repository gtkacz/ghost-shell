import test from 'node:test';
import assert from 'node:assert/strict';
import {
  acceptNotificationsSeq,
  bellState,
  clampSelection,
  countsLine,
  filterRecords,
  footerLeft,
  greetingSignals,
  isCountdown,
  isOpenableLink,
  stableActions,
  moveSelection,
  quietSince,
  rowMarkup,
  rowText,
  toastQueue,
  windowToRaise,
} from '../kit/GhostKit/lib/notifications.mjs';

const SHELL_START_S = 1000;
const record = (id, extra = {}) => ({
  id,
  app: 'Discord',
  desktop_entry: '',
  summary: 'hello',
  body_text: 'world',
  urgency: 'normal',
  level: 'none',
  pid: 0,
  arrived_s: SHELL_START_S + id,
  held: false,
  ...extra,
});
const critical = (id) => record(id, { urgency: 'critical', level: 'crit' });
const queue = (items, extra = {}) =>
  toastQueue({ items, shown: new Set(), shellStartS: SHELL_START_S, maxPopups: 3, suppressed: '', ...extra });
// The snapshot lists the newest first.
const newestFirst = (...records) => records.sort((a, b) => b.arrived_s - a.arrived_s);

test('bell is dim with no badge at zero and crit while an unread critical remains', () => {
  const quiet = bellState({ server: 'ghost-shell', unread: 0, unreadCrit: 0, dnd: false });
  assert.deepEqual([quiet.glyph, quiet.colour, quiet.badge], ['bell', 'dim', '']);
  assert.equal(quiet.tip, 'Notifications · 0 unread');
  const some = bellState({ server: 'ghost-shell', unread: 3, unreadCrit: 0, dnd: false });
  assert.deepEqual([some.colour, some.badge, some.badgeColour], ['signal', '3', 'signal']);
  const urgent = bellState({ server: 'ghost-shell', unread: 3, unreadCrit: 1, dnd: false });
  assert.deepEqual([urgent.colour, urgent.badgeColour], ['crit', 'crit']);
});

test('bell caps the badge at 99', () => {
  assert.equal(bellState({ server: 'ghost-shell', unread: 250, unreadCrit: 0, dnd: false }).badge, '99');
});

test('bell shows bell-off with a dim badge under do not disturb', () => {
  const bell = bellState({ server: 'ghost-shell', unread: 2, unreadCrit: 1, dnd: true });
  assert.deepEqual([bell.glyph, bell.colour, bell.badge, bell.badgeColour], ['bell-off', 'dim', '2', 'dim']);
  assert.equal(bellState({ server: 'ghost-shell', unread: 0, unreadCrit: 0, dnd: true }).badge, '');
});

test('bell shows plasma serving without the server', () => {
  for (const server of ['plasma', 'none']) {
    const bell = bellState({ server, unread: 4, unreadCrit: 1, dnd: false });
    assert.deepEqual([bell.glyph, bell.colour, bell.badge], ['bell', 'dim', '']);
    assert.equal(bell.tip, 'Notifications · Plasma serving');
  }
});

test('toasts only records after shell start', () => {
  const old = record(1, { arrived_s: SHELL_START_S - 1 });
  assert.deepEqual(queue([old]), { show: [], waiting: 0 });
  assert.deepEqual(queue([record(2, { arrived_s: SHELL_START_S })]).show, [2]);
});

test('held records never toast', () => {
  assert.deepEqual(queue([record(1, { held: true })]), { show: [], waiting: 0 });
});

test('records already shown or ended never toast again', () => {
  assert.deepEqual(queue([record(1), record(2)], { shown: new Set([1]) }).show, [2]);
});

test('at most max popups, criticals are never pushed out', () => {
  const normals = [record(1), record(2), record(3), record(4)];
  assert.deepEqual(queue(newestFirst(...normals)), { show: [3, 2, 1], waiting: 1 });
  assert.deepEqual(queue(newestFirst(...normals, critical(5))), { show: [5, 2, 1], waiting: 2 });
  const manyCriticals = queue(newestFirst(critical(1), critical(2), critical(3), critical(4), record(5)));
  assert.deepEqual(manyCriticals, { show: [3, 2, 1], waiting: 2 });
});

test('oldest visible normal yields to a critical', () => {
  const items = newestFirst(record(1), record(2), record(3), critical(4));
  assert.deepEqual(queue(items, { visible: [3, 2, 1] }), { show: [4, 3, 2], waiting: 1 });
});

test('two criticals over three normals', () => {
  const items = newestFirst(record(1), record(2), record(3), critical(4), critical(5));
  assert.deepEqual(queue(items, { visible: [3, 2, 1] }), { show: [5, 4, 3], waiting: 2 });
});

test('a critical on screen is never the one that yields', () => {
  const items = newestFirst(critical(1), record(2), record(3), critical(4));
  assert.deepEqual(queue(items, { visible: [3, 2, 1] }), { show: [4, 3, 1], waiting: 1 });
});

test('replacement of a visible record keeps its slot', () => {
  const replaced = record(1, { arrived_s: SHELL_START_S + 10 });
  const items = newestFirst(replaced, record(2), record(3), record(4));
  assert.deepEqual(queue(items, { visible: [3, 2, 1] }), { show: [1, 3, 2], waiting: 1 });
});

test('a freed slot goes to the oldest waiting record, criticals first', () => {
  const items = newestFirst(record(1), record(2), record(3), record(4), critical(5));
  assert.deepEqual(queue(items, { visible: [2, 1], maxPopups: 3 }), { show: [5, 2, 1], waiting: 2 });
});

test('zero or invalid max popups shows nothing', () => {
  for (const maxPopups of [0, -1, 1.5, NaN, undefined, '3']) {
    assert.deepEqual(queue([record(1), record(2)], { maxPopups }), { show: [], waiting: 2 }, String(maxPopups));
  }
});

test('a critical shows ahead of an older normal when slots are short', () => {
  assert.deepEqual(queue(newestFirst(record(1), record(2), critical(3)), { maxPopups: 1 }), { show: [3], waiting: 2 });
});

test('suppressed holds everything and waiting counts them', () => {
  for (const suppressed of ['dnd', 'inhibited', 'fullscreen', 'locked']) {
    assert.deepEqual(queue(newestFirst(record(1), record(2)), { suppressed }), { show: [], waiting: 2 });
  }
});

test('filter matches app summary and body case-insensitively', () => {
  const items = [
    record(1, { app: 'STEAM', summary: 'Download complete', body_text: '' }),
    record(2, { app: 'DNF', summary: 'Updates', body_text: 'kernel 7.2.12' }),
  ];
  assert.deepEqual(filterRecords(items, 'steam').map((item) => item.id), [1]);
  assert.deepEqual(filterRecords(items, 'DOWNLOAD').map((item) => item.id), [1]);
  assert.deepEqual(filterRecords(items, 'KERNEL').map((item) => item.id), [2]);
  assert.equal(filterRecords(items, '').length, 2);
  assert.equal(filterRecords(items, 'nothing').length, 0);
});

test('row text upper-cases the app and ages the record', () => {
  const now = 10_000;
  assert.deepEqual(rowText(record(1, { app: 'Steam', arrived_s: now - 120 }), now), {
    app: 'STEAM',
    summary: 'hello',
    body: 'world',
    age: '2M',
  });
  assert.equal(rowText(record(1, { arrived_s: now + 5 }), now).age, 'NOW');
});

test('counts line omits zero parts', () => {
  const items = [critical(1), record(2, { level: 'warn' }), record(3)];
  assert.equal(countsLine(items), '3 TOTAL · 1 CRIT · 1 WARN');
  assert.equal(countsLine([record(1), record(2)]), '2 TOTAL');
  assert.equal(countsLine([critical(1), record(2)]), '2 TOTAL · 1 CRIT');
  assert.equal(countsLine([]), '0 TOTAL');
});

test('footer reads shown and new', () => {
  assert.equal(footerLeft([record(1), record(2)], 1), '2 SHOWN · 1 NEW');
});

test('quiet since is the newest arrival, else the start', () => {
  const at = (hour, minute) => new Date(2026, 9, 24, hour, minute).getTime() / 1000;
  assert.equal(quietSince([record(1, { arrived_s: at(9, 5) })], at(7, 0)), 'QUIET SINCE 09:05');
  assert.equal(quietSince([], at(7, 30)), 'QUIET SINCE 07:30');
});

test('window to raise is null when none matches', () => {
  const windows = [{ id: 'a', app_id: 'org.kde.konsole', pid: 5, active: false }];
  assert.equal(windowToRaise(windows, record(1, { desktop_entry: 'discord', pid: 9 })), null);
  assert.equal(windowToRaise(windows, record(1)), null);
  assert.equal(windowToRaise([], record(1, { desktop_entry: 'discord' })), null);
});

test('window to raise prefers app id over pid', () => {
  const windows = [
    { id: 'by-pid', app_id: 'other', pid: 9, active: true },
    { id: 'by-app', app_id: 'discord', pid: 4, active: false },
  ];
  assert.equal(windowToRaise(windows, record(1, { desktop_entry: 'discord', pid: 9 })), 'by-app');
  assert.equal(windowToRaise(windows, record(1, { desktop_entry: 'missing', pid: 9 })), 'by-pid');
});

test('window to raise prefers the active match, then the first', () => {
  const windows = [
    { id: 'first', app_id: 'discord', pid: 1, active: false },
    { id: 'active', app_id: 'discord', pid: 1, active: true },
  ];
  assert.equal(windowToRaise(windows, record(1, { desktop_entry: 'discord' })), 'active');
  assert.equal(windowToRaise(windows.slice(0, 1), record(1, { desktop_entry: 'discord' })), 'first');
});

test('window to raise ignores an unknown pid', () => {
  assert.equal(windowToRaise([{ id: 'a', app_id: 'x', pid: 0, active: false }], record(1)), null);
});

test('notifications seq is a toggle only after the first value on a connection', () => {
  const first = acceptNotificationsSeq(null, { notifications_seq: 4 }, 1);
  assert.equal(first.toggle, false);
  const press = acceptNotificationsSeq(first.seen, { notifications_seq: 5 }, 1);
  assert.equal(press.toggle, true);
  const same = acceptNotificationsSeq(press.seen, { notifications_seq: 5 }, 1);
  assert.equal(same.toggle, false);
  const reconnect = acceptNotificationsSeq(press.seen, { notifications_seq: 0 }, 2);
  assert.equal(reconnect.toggle, false);
});

test('selection moves one row and stays inside the list', () => {
  assert.equal(moveSelection(1, 'Down', 3), 2);
  assert.equal(moveSelection(2, 'Down', 3), 2);
  assert.equal(moveSelection(0, 'Up', 3), 0);
  assert.equal(moveSelection(2, 'Up', 3), 1);
  assert.equal(moveSelection(1, 'Left', 3), 1);
  assert.equal(moveSelection(0, 'Down', 0), 0);
});

test('selection is clamped to a shorter list and never below the first row', () => {
  assert.equal(clampSelection(5, 3), 2);
  assert.equal(clampSelection(1, 3), 1);
  assert.equal(clampSelection(0, 0), 0);
  assert.equal(clampSelection(4, 0), 0);
});

test('a row is one escaped line with the body in its own colour', () => {
  const row = rowMarkup(record(1, { summary: 'a <b> & c', body_text: 'line one\nline  two' }), '#587170');
  assert.equal(row, 'a &lt;b&gt; &amp; c<font color="#587170"> \u2014 line one line two</font>');
  assert.equal(rowMarkup(record(2, { summary: 'only', body_text: '' }), '#587170'), 'only');
});

test('a countdown body is exactly HH:MM:SS', () => {
  assert.equal(isCountdown('00:42:10'), true);
  assert.equal(isCountdown('0:42:10'), false);
  assert.equal(isCountdown('in 00:42:10'), false);
});

test('greeting signals are the unread count only while ghost-shell serves', () => {
  assert.equal(greetingSignals({ server: 'ghost-shell', unread: 3 }), 3);
  assert.equal(greetingSignals({ server: 'plasma', unread: 3 }), 0);
  assert.equal(greetingSignals(null), 0);
  assert.equal(greetingSignals({ server: 'ghost-shell' }), 0);
});

test('only web and mail links open', () => {
  for (const url of ['http://a.b', 'HTTPS://a.b/x', 'mailto:a@b.c', 'MailTo:a@b.c']) {
    assert.equal(isOpenableLink(url), true, url);
  }
  for (const url of ['file:///etc/passwd', 'javascript:alert(1)', 'ftp://a.b', 'a.b', '', 'xhttp://a.b', ' http://a.b', null]) {
    assert.equal(isOpenableLink(url), false, String(url));
  }
});

test('actions keep their identity across a publish that changes none of them', () => {
  const before = [{ key: 'open', label: 'Open' }, { key: 'read', label: 'Mark read' }];
  const republished = [{ key: 'open', label: 'Open' }, { key: 'read', label: 'Mark read' }];
  assert.equal(stableActions(before, republished), before);
});

test('actions take the new list when a key, a label or the count changes', () => {
  const before = [{ key: 'open', label: 'Open' }];
  for (const next of [[{ key: 'read', label: 'Open' }], [{ key: 'open', label: 'Show' }], [], [...before, { key: 'x', label: 'X' }]]) {
    assert.equal(stableActions(before, next), next);
  }
});
