import test from 'node:test';
import assert from 'node:assert/strict';
import { isValidTopic, requiresOf, subscriptionFor } from '../kit/GhostKit/lib/subscriptions.mjs';

test('visible subscribes to every slot, wanted topics, desktop and perf', () => assert.deepEqual(subscriptionFor({ requires: ['mem', 'cpu', 'cpu'], wanted: ['audio.spectrum'], covered: false, barRequires: [] }), ['audio.spectrum', 'cpu', 'desktop', 'mem', 'perf']));
test('covered keeps only desktop, perf and the bar', () => assert.deepEqual(subscriptionFor({ requires: ['cpu'], wanted: ['audio.spectrum'], covered: true, barRequires: ['weather'] }), ['desktop', 'perf', 'weather']));
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
