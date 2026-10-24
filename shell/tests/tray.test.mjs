import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { networkState, bluetoothState, volumeState, kdeConnectState, brightnessState, showsInline, stepVolume, APPLETS } from '../kit/GhostKit/lib/tray.mjs';

const ACTIONS_RS = new URL('../../crates/ghostd/src/feeds/actions.rs', import.meta.url);
const lan = (state) => ({ iface: 'e0', address: null, speed_mbps: null, state });
const phone = { id: 'kdeconnect:a', label: 'PHONE', kind: 'phone', pct: 80, charging: false };

test('network is dim with no LAN link up', () => {
  assert.deepEqual(networkState({ lan: [lan('down'), lan('connecting')] }), { glyph: 'network', dim: true });
  assert.deepEqual(networkState({ lan: [lan('down'), lan('up')] }), { glyph: 'network', dim: false });
  assert.equal(networkState(null).dim, true);
});

test('bluetooth is dim while the adapter is off', () => {
  assert.deepEqual(bluetoothState({ enabled: false, connected: 0 }), { glyph: 'bluetooth', dim: true });
  assert.deepEqual(bluetoothState({ enabled: true, connected: 2 }), { glyph: 'bluetooth', dim: false });
});

test('volume steps by five percent and clamps', () => {
  assert.equal(stepVolume(0.98, 1), 1);
  assert.equal(stepVolume(0.5, -2), 0.4);
  assert.equal(stepVolume(0.02, -1), 0);
});

test('muted volume is dim', () => {
  assert.deepEqual(volumeState({ volume: 0.64, muted: true }), { glyph: 'volume', dim: true, label: 'VOLUME 64%' });
  assert.equal(volumeState({ volume: 0.64, muted: false }).dim, false);
});

test('kde connect is dim without a phone', () => {
  assert.deepEqual(kdeConnectState({ devices: [{ ...phone, kind: 'mouse' }] }), { glyph: 'phone', dim: true });
  assert.deepEqual(kdeConnectState({ devices: [phone] }), { glyph: 'phone', dim: false });
  assert.equal(kdeConnectState(null).dim, true);
});

test('every applet name is one the daemon accepts', () => {
  const source = readFileSync(ACTIONS_RS, 'utf8');
  const list = source.match(/POPUP_APPLETS: \[&str; \d+\] = \[([^\]]*)\]/)[1];
  const accepted = [...list.matchAll(/"([^"]+)"/g)].map((match) => match[1]);
  for (const name of Object.values(APPLETS)) {
    assert.ok(accepted.includes(name), name);
  }
});

test('brightness is dim while Night Light is not active', () => {
  assert.deepEqual(brightnessState({ nightLight: false }), { glyph: 'brightness', dim: true });
  assert.deepEqual(brightnessState({ nightLight: true }), { glyph: 'brightness', dim: false });
});

test('passive status items stay behind the chevron', () => {
  const passive = 0;
  const active = 1;
  const needsAttention = 2;
  assert.equal(showsInline(passive), false);
  assert.equal(showsInline(active), true);
  assert.equal(showsInline(needsAttention), true);
});
