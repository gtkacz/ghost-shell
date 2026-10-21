import test from 'node:test';
import assert from 'node:assert/strict';
import { acceptAction, applyAction, initialViews, reconcileViews, revealRows } from '../kit/GhostKit/lib/views.mjs';

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

test("a restarted daemon's first action is not dropped", () => {
  let { seen } = acceptAction(null, { action: null, action_seq: 0 }, 1);
  ({ seen } = acceptAction(seen, { action: 'monitor', action_seq: 5 }, 1));
  ({ seen } = acceptAction(seen, { action: null, action_seq: 0 }, 2));
  assert.equal(acceptAction(seen, { action: 'swap', action_seq: 1 }, 2).action, 'swap');
});
