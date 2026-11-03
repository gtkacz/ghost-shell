import assert from 'node:assert/strict';
import test from 'node:test';
import { collapseIdle, fitRows, projectOf, providerTag, resetText, sortSessions, stateLabel, waitingCount, weeklyOf, windowLabel } from '../../widgets/agents/agents.mjs';

const s = (id, state, since_s, extra = {}) => ({ provider: 'claude', id, name: id, cwd: '/home/u/p', state, waiting_for: null, since_s, pid: null, tmux: null, ...extra });

test('tags are CC and CX, unknown providers abbreviate', () => {
  assert.equal(providerTag('claude'), 'CC');
  assert.equal(providerTag('codex'), 'CX');
  assert.equal(providerTag('gemini'), 'GE');
  assert.equal(providerTag(''), '');
});

test('windows are labelled by length', () => {
  assert.equal(windowLabel(18000), '5H');
  assert.equal(windowLabel(604800), '7D');
  assert.equal(windowLabel(7200), '2H');
});

test('the weekly window is found by its length', () => {
  const weekly = { length_s: 604800, used_pct: 10, resets_at_s: null };
  assert.equal(weeklyOf({ windows: [{ length_s: 18000 }, weekly] }), weekly);
  assert.equal(weeklyOf({ windows: [{ length_s: 18000 }] }), null);
  assert.equal(weeklyOf(null), null);
  assert.equal(weeklyOf({}), null);
});

test('waiting first, then busy, then idle, newest first within a group', () => {
  const input = [s('a', 'idle', 10), s('b', 'busy', 20), s('c', 'waiting', 5), s('d', 'busy', 30)];
  const sorted = sortSessions(input);
  assert.deepEqual(sorted.map(x => x.id), ['c', 'd', 'b', 'a']);
  assert.deepEqual(input.map(x => x.id), ['a', 'b', 'c', 'd']);
});

test('equal ages keep their input order', () => {
  const sorted = sortSessions([s('x', 'busy', 1), s('y', 'busy', 1)]);
  assert.deepEqual(sorted.map(x => x.id), ['x', 'y']);
});

test('state labels name the wait', () => {
  assert.equal(stateLabel(s('a', 'waiting', 0, { waiting_for: 'permission prompt' })), 'PERMISSION PROMPT');
  assert.equal(stateLabel(s('a', 'waiting', 0)), 'WAITING');
  assert.equal(stateLabel(s('a', 'busy', 0)), 'BUSY');
  assert.equal(stateLabel(s('a', 'idle', 0)), 'IDLE');
  assert.equal(stateLabel(s('a', 'dozing', 0)), 'DOZING');
  assert.equal(waitingCount([s('a', 'waiting', 0), s('b', 'idle', 0)]), 1);
});

test('reset reads as a countdown under a day and a weekday beyond', () => {
  const now = 1792797010;
  assert.equal(resetText(null, now), '--');
  assert.equal(resetText(Number.NaN, now), '--');
  assert.equal(resetText(now + 2 * 3600 + 14 * 60, now), '2H 14M');
  assert.equal(resetText(now + 14 * 60, now), '14M');
  assert.equal(resetText(now + 59, now), 'NOW');
  const later = now + 3 * 86400;
  const d = new Date(later * 1000);
  const hhmm = String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0');
  assert.match(resetText(later, now), /^(SUN|MON|TUE|WED|THU|FRI|SAT) \d\d:\d\d$/);
  assert.ok(resetText(later, now).endsWith(hhmm));
});

test('idle rows collapse past five', () => {
  const sorted = sortSessions([s('w', 'waiting', 1), ...Array.from({ length: 8 }, (_, i) => s('i' + i, 'idle', i))]);
  const { rows, hiddenIdle } = collapseIdle(sorted);
  assert.equal(rows.length, 6);
  assert.equal(hiddenIdle, 3);
  assert.equal(collapseIdle(sorted, 8).hiddenIdle, 0);
});

test('project is the last path segment', () => {
  assert.equal(projectOf('/home/u/Codes/ghost-shell'), 'ghost-shell');
  assert.equal(projectOf(''), '');
});

test('rows that do not fit are counted on a line of their own', () => {
  const sorted = [s('a', 'waiting', 1), s('b', 'busy', 2), s('c', 'idle', 3)];

  assert.deepEqual(fitRows(sorted, 4), { rows: sorted, hidden: 0 });
  assert.deepEqual(fitRows(sorted, 3), { rows: sorted, hidden: 0 });
  assert.deepEqual(fitRows(sorted, 2), { rows: [sorted[0]], hidden: 2 });
  assert.deepEqual(fitRows(sorted, 1), { rows: [], hidden: 3 });
  assert.deepEqual(fitRows(sorted, 0), { rows: [], hidden: 3 });
  assert.deepEqual(fitRows(sorted, -2), { rows: [], hidden: 3 });
  assert.deepEqual(fitRows([], 0), { rows: [], hidden: 0 });
});
