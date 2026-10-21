import test from 'node:test';
import assert from 'node:assert/strict';
import { MAIN_MS, MONITOR_MS, PRINT_STEPS, acceptAction, applyAction, initialViews, reconcileViews, revealRows, stepDelay } from '../kit/GhostKit/lib/views.mjs';

const dash = { main_views: ['v1', 'v2'], monitor_view: 'monitor', default_view: 'v1' };
test('monitor toggles and returns to the main view it came from', () => { assert.deepEqual(applyAction({ current: 'v2', lastMain: 'v2' }, 'monitor', dash), { current: 'monitor', lastMain: 'v2', durationMs: 640 }); assert.deepEqual(applyAction({ current: 'monitor', lastMain: 'v2' }, 'monitor', dash), { current: 'v2', lastMain: 'v2', durationMs: 640 }); });
test('swap cycles main views at 380 ms', () => assert.deepEqual(applyAction({ current: 'v2', lastMain: 'v2' }, 'swap', dash), { current: 'v1', lastMain: 'v1', durationMs: 380 }));
test('swap from monitor prints the next main view at 640 ms', () => assert.deepEqual(applyAction({ current: 'monitor', lastMain: 'v1' }, 'swap', dash), { current: 'v2', lastMain: 'v2', durationMs: 640 }));
test('no monitor view or one main view is a no-op', () => { assert.equal(applyAction({ current: 'v1', lastMain: 'v1' }, 'monitor', { ...dash, monitor_view: '' }), null); assert.equal(applyAction({ current: 'v1', lastMain: 'v1' }, 'swap', { ...dash, main_views: ['v1'] }), null); });
test('a removed view falls back to the default', () => assert.deepEqual(reconcileViews({ current: 'v3', lastMain: 'v3' }, dash, ['v1', 'v2', 'monitor']), { current: 'v1', lastMain: 'v1' }));
test('reveal rows are whole pixels and end at the full height', () => { assert.equal(revealRows(1, 24, 666), 28); assert.equal(revealRows(24, 24, 666), 666); });

test('views start on the default view', () => assert.deepEqual(initialViews(dash), { current: 'v1', lastMain: 'v1' }));
test('the monitor view returns to the view it was opened from', () => assert.deepEqual(applyAction({ current: 'v2', lastMain: 'v1' }, 'monitor', dash), { current: 'monitor', lastMain: 'v2', durationMs: 640 }));

test('the first desktop value on each connection is only the baseline', () => {
  const first = acceptAction(null, { action: 'monitor', action_seq: 3 }, 1);
  assert.equal(first.action, null);
  assert.equal(acceptAction(first.seen, { action: 'monitor', action_seq: 3 }, 1).action, null);
  assert.equal(acceptAction(first.seen, { action: 'swap', action_seq: 4 }, 1).action, 'swap');
  assert.equal(acceptAction(first.seen, { action: 'swap', action_seq: 4 }, 2).action, null);
});

test('an action during a print is ignored, not queued', () => {
  const { seen } = acceptAction(null, { action: null, action_seq: 0 }, 1);
  const during = acceptAction(seen, { action: 'swap', action_seq: 1 }, 1);
  assert.equal(applyAction({ current: 'v1', lastMain: 'v1' }, during.action, dash, true), null);
  // The daemon's next publish, a report once the print has ended, still carries that seq.
  assert.equal(acceptAction(during.seen, { action: 'swap', action_seq: 1 }, 1).action, null);
});

test('step intervals add up to the exact print length in whole milliseconds', () => {
  for (const ms of [MONITOR_MS, MAIN_MS]) {
    let elapsed = 0;
    const intervals = Array.from({ length: PRINT_STEPS }, (_, step) => {
      const wait = stepDelay(step, PRINT_STEPS, ms, elapsed);
      elapsed += wait;
      return wait;
    });
    assert.equal(intervals.reduce((sum, interval) => sum + interval, 0), ms);
    assert.ok(intervals.every((interval) => interval === Math.floor(ms / PRINT_STEPS) || interval === Math.ceil(ms / PRINT_STEPS)), String(intervals));
  }
});

test('a late step shortens the next wait, so later steps stay on time', () => {
  assert.equal(stepDelay(0, PRINT_STEPS, MONITOR_MS, 0), 27);
  assert.equal(stepDelay(1, PRINT_STEPS, MONITOR_MS, 30), 23);
  assert.equal(stepDelay(1, PRINT_STEPS, MONITOR_MS, 80), 1);
});

test('a view no shortcut can reach returns to the last main view, or the default', () => {
  const noMonitor = { ...dash, monitor_view: '' };
  assert.deepEqual(reconcileViews({ current: 'monitor', lastMain: 'v2' }, noMonitor, ['v1', 'v2', 'monitor']), { current: 'v2', lastMain: 'v2' });
  assert.deepEqual(reconcileViews({ current: 'monitor', lastMain: 'v2' }, { ...noMonitor, main_views: ['v1'] }, ['v1', 'v2', 'monitor']), { current: 'v1', lastMain: 'v1' });
  assert.deepEqual(reconcileViews({ current: 'monitor', lastMain: 'v2' }, dash, ['v1', 'v2', 'monitor']), { current: 'monitor', lastMain: 'v2' });
});

test("a restarted daemon's first action is not dropped", () => {
  let { seen } = acceptAction(null, { action: null, action_seq: 0 }, 1);
  ({ seen } = acceptAction(seen, { action: 'monitor', action_seq: 5 }, 1));
  ({ seen } = acceptAction(seen, { action: null, action_seq: 0 }, 2));
  assert.equal(acceptAction(seen, { action: 'swap', action_seq: 1 }, 2).action, 'swap');
});

test('a wall clock stepped back mid-print waits only for the step, not for the size of the step', () => {
  assert.equal(stepDelay(12, PRINT_STEPS, MONITOR_MS, -3_600_000), 347);
  assert.equal(stepDelay(0, PRINT_STEPS, MAIN_MS, -5), 16);
});

test('a view missing from the layout falls back even while main_views still lists it', () => {
  const withoutV2 = ['v1', 'monitor'];
  assert.deepEqual(reconcileViews({ current: 'v2', lastMain: 'v2' }, dash, withoutV2), { current: 'v1', lastMain: 'v1' });
  assert.deepEqual(reconcileViews({ current: 'monitor', lastMain: 'v2' }, dash, withoutV2), { current: 'monitor', lastMain: 'v1' });
});
