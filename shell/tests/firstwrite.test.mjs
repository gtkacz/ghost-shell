import test from 'node:test';
import assert from 'node:assert/strict';
import { forget, hostScope, scopedKey, slotSignature, viewPrefix } from '../kit/GhostKit/lib/firstwrite.mjs';

test('a key outside any host stays as it is', () => {
  assert.equal(scopedKey('', 'panel:UPLINK'), 'panel:UPLINK');
});

test('a host scopes its keys by view and slot path', () => {
  assert.equal(scopedKey(hostScope('V1', '[0][2]'), 'panel:UPLINK'), 'V1/[0][2]|panel:UPLINK');
});

test('forgetting a host leaves its siblings, its view and other views claimed', () => {
  const claimed = new Set([
    scopedKey(hostScope('V1', '[1]'), 'panel:UPLINK'),
    scopedKey(hostScope('V1', '[1]'), 'lane:CPU'),
    scopedKey(hostScope('V1', '[10]'), 'panel:NET'),
    scopedKey(hostScope('V2', '[1]'), 'panel:UPLINK'),
    'gallery',
  ]);
  forget(claimed, scopedKey(hostScope('V1', '[1]'), ''));
  assert.deepEqual([...claimed], ['V1/[10]|panel:NET', 'V2/[1]|panel:UPLINK', 'gallery']);
});

test('forgetting a view leaves every other view, even one whose name it begins', () => {
  const claimed = new Set([
    scopedKey(hostScope('V1', '[0]'), 'panel:CPU'),
    scopedKey(hostScope('V1', '[1][0]'), 'panel:NET'),
    scopedKey(hostScope('V10', '[0]'), 'panel:CPU'),
    scopedKey(hostScope('MONITOR', '[0]'), 'lane:CPU'),
  ]);
  forget(claimed, viewPrefix('V1'));
  assert.deepEqual([...claimed], ['V10/[0]|panel:CPU', 'MONITOR/[0]|lane:CPU']);
});

test('a slot signature follows the widget and its settings, not the slot object or its place', () => {
  const slot = { widget: 'clock', source: 'file:///w/clock/Widget.qml', settings: { style: 'hero' }, x: 0, y: 0, w: 10, h: 10 };
  const moved = { ...slot, settings: { style: 'hero' }, x: 40, w: 20 };
  const restyled = { ...slot, settings: { style: 'band' } };
  const otherTab = { ...slot, widget: 'network', source: 'file:///w/network/Widget.qml' };
  assert.equal(slotSignature(moved), slotSignature(slot));
  assert.notEqual(slotSignature(restyled), slotSignature(slot));
  assert.notEqual(slotSignature(otherTab), slotSignature(slot));
});
