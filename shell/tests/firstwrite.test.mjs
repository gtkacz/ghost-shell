import test from 'node:test';
import assert from 'node:assert/strict';
import { forget, holdsClaims, hostScope, isWritten, positionKey, scopedKey, slotSignature, viewPrefix } from '../kit/GhostKit/lib/firstwrite.mjs';

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

test('a text without a key is known by its place, which no two places share', () => {
  assert.equal(positionKey([0, 3, 1]), positionKey([0, 3, 1]));
  assert.notEqual(positionKey([1, 12]), positionKey([11, 2]));
  assert.notEqual(positionKey([0]), positionKey([0, 0]));
  assert.ok(!positionKey([0]).startsWith('panel:') && !positionKey([0]).startsWith('lane:'));
});

test('a blank or a missing value is not yet a first write, and any other text is', () => {
  assert.equal(isWritten(''), false);
  assert.equal(isWritten('   '), false);
  assert.equal(isWritten('--'), false);
  assert.equal(isWritten('42%'), true);
  assert.equal(isWritten(' · '), true);
  assert.equal(isWritten('CPU'), true);
});

test('a host holds claims until its showing is forgotten, and a sibling it begins does not count', () => {
  const claimed = new Set([scopedKey(hostScope('V1', '[1]'), '@2.0'), scopedKey(hostScope('V1', '[10]'), '@2.0')]);
  assert.equal(holdsClaims(claimed, hostScope('V1', '[1]')), true);
  forget(claimed, scopedKey(hostScope('V1', '[1]'), ''));
  assert.equal(holdsClaims(claimed, hostScope('V1', '[1]')), false);
  assert.equal(holdsClaims(claimed, hostScope('V1', '[10]')), true);
});
