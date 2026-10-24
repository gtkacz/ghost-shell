import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { networkState, bluetoothState, volumeState, kdeConnectState, brightnessState, showsInline, stepVolume, stepBrightness, cellBrightness, nightLightSwitch, brightnessFloor, displayedBrightness, pendingAfter, wheelTarget, APPLETS } from '../kit/GhostKit/lib/tray.mjs';

const ACTIONS_RS = new URL('../../crates/ghostd/src/feeds/actions.rs', import.meta.url);
const WIDGET_QML = new URL('../../widgets/tray/Widget.qml', import.meta.url);
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

const MAX = 10000;

test('brightness wheel steps five percent of the range and clamps', () => {
  assert.equal(stepBrightness(5000, MAX, 1), 5500);
  assert.equal(stepBrightness(5000, MAX, -2), 4000);
  assert.equal(stepBrightness(9800, MAX, 1), MAX);
});

test('brightness never goes below one percent of the range, rounded up', () => {
  assert.equal(brightnessFloor(MAX), 100);
  assert.equal(brightnessFloor(255), 3);
  assert.equal(brightnessFloor(7), 1);
  assert.equal(brightnessFloor(0), 1);
  assert.equal(stepBrightness(300, MAX, -1), 100);
  assert.equal(stepBrightness(100, MAX, -3), 100);
});

test('brightness step moves a display with a coarse range by at least one unit', () => {
  assert.equal(stepBrightness(10, 20, 1), 11);
  assert.equal(stepBrightness(1, 7, 1), 2);
});

test('a slider cell sets the brightness that lights up to it', () => {
  const cells = 20;
  const cellStep = 6;
  assert.equal(cellBrightness(0, cells, cellStep, MAX), 500);
  assert.equal(cellBrightness(5.9, cells, cellStep, MAX), 500);
  assert.equal(cellBrightness(6, cells, cellStep, MAX), 1000);
  assert.equal(cellBrightness(1000, cells, cellStep, MAX), MAX);
  assert.equal(cellBrightness(-4, cells, cellStep, MAX), 500);
  assert.equal(cellBrightness(10, 0, cellStep, MAX), 100);
  assert.equal(cellBrightness(0, 400, cellStep, MAX), 100);
});

test('the night light switch reads on unless it is suspended, and works only while enabled', () => {
  const base = { available: true, enabled: true, inhibited: false, running: true };
  assert.deepEqual(nightLightSwitch(base), { on: true, usable: true, status: 'ON' });
  assert.deepEqual(nightLightSwitch({ ...base, running: false }), { on: true, usable: true, status: 'ON' });
  assert.deepEqual(nightLightSwitch({ ...base, inhibited: true, running: false }), { on: false, usable: true, status: 'SUSPENDED' });
  assert.deepEqual(nightLightSwitch({ ...base, enabled: false, running: false }), { on: false, usable: false, status: 'DISABLED' });
  assert.deepEqual(nightLightSwitch({ ...base, available: false }), { on: false, usable: false, status: 'UNAVAILABLE' });
  assert.deepEqual(nightLightSwitch(null), { on: false, usable: false, status: 'UNAVAILABLE' });
});

test('the retired Brightness and Color applet is no longer opened through Plasma', () => {
  assert.equal('brightness' in APPLETS, false);
});

test('a row shows the drag, then the sent value until the topic confirms it, then the topic', () => {
  assert.equal(displayedBrightness(3000, 2000, 1000), 3000);
  assert.equal(displayedBrightness(-1, 2000, 1000), 2000);
  assert.equal(displayedBrightness(-1, -1, 1000), 1000);
});

test('a sent value is held until the published one matches it', () => {
  assert.equal(pendingAfter(2000, 1500), 2000);
  assert.equal(pendingAfter(2000, 2000), -1);
  assert.equal(pendingAfter(-1, 2000), -1);
});

test('wheel notches add up from the pending target, not from the last published value', () => {
  assert.equal(wheelTarget(-1, 5000, MAX, 1), 5500);
  assert.equal(wheelTarget(5500, 5000, MAX, 1), 6000);
  assert.equal(wheelTarget(150, 5000, MAX, -1), 100);
});

test('the tray ends with the bell before the chevron', () => {
  const source = readFileSync(WIDGET_QML, 'utf8');
  const sniItems = source.indexOf('model: SystemTray.items');
  const bell = source.indexOf('Notifications.bell.glyph');
  const chevron = source.indexOf('"chevron-up"');
  assert.ok(sniItems !== -1 && bell !== -1 && chevron !== -1, 'the markers are in Widget.qml');
  assert.ok(sniItems < bell && bell < chevron);
});
