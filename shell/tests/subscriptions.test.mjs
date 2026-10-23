import test from 'node:test';
import assert from 'node:assert/strict';
import { isValidTopic, requiresOf, barRequiresOf, subscriptionFor } from '../kit/GhostKit/lib/subscriptions.mjs';

test('visible subscribes to every slot, wanted topics, desktop, perf and session', () => assert.deepEqual(subscriptionFor({ requires: ['mem', 'cpu', 'cpu'], wanted: ['audio.spectrum'], covered: false, barRequires: [] }), ['audio.spectrum', 'cpu', 'desktop', 'mem', 'perf', 'session']));
test('requires span every view and tab', () => assert.deepEqual(requiresOf({ views: { v1: { requires: ['cpu'], widget: 'processor' }, v3: { tabs: [{ requires: ['gpu'] }, { requires: ['mem', 'gpu'] }] } } }), ['cpu', 'gpu', 'mem']));

test('requires reach the slots of nested splits', () => {
  const layout = { views: { v1: { direction: 'row', children: [{ requires: ['disks'] }, { direction: 'column', children: [{ requires: ['sensors'] }] }] } } };
  assert.deepEqual(requiresOf(layout), ['disks', 'sensors']);
});

test('a topic name is dot-separated lowercase segments of at most 64 characters', () => {
  for (const valid of ['cpu', 'audio.spectrum', 'weather.alerts', 'a_1.b2', 'a'.repeat(64)]) {
    assert.equal(isValidTopic(valid), true, valid);
  }
  for (const invalid of ['', 'CPU', '1cpu', '_cpu', 'audio.', '.audio', 'audio..spectrum', 'audio-spectrum', 'a'.repeat(65), null, undefined, 7]) {
    assert.equal(isValidTopic(invalid), false, String(invalid));
  }
});

// The dashboard's widgets are destroyed while covered, so every want left is the bar's own.
test('covered keeps wanted topics and bar topics', () => assert.deepEqual(subscriptionFor({ requires: ['cpu'], wanted: ['net'], covered: true, barRequires: ['windows'] }), ['desktop', 'net', 'perf', 'session', 'windows']));

test('bar requires collect bar and dock slots', () => {
  const layout = {
    bar: {
      left: [{ requires: ['host'] }, { requires: [] }],
      centre: [{ requires: ['windows', 'host'] }],
      right: [{ requires: ['cpu'] }],
      dock: [{ requires: ['launcher.entries', 'windows'] }],
    },
  };
  assert.deepEqual(barRequiresOf(layout), ['cpu', 'host', 'launcher.entries', 'windows']);
});

test('a layout without a bar requires nothing of it', () => {
  assert.deepEqual(barRequiresOf({ views: {} }), []);
  assert.deepEqual(barRequiresOf(null), []);
});
