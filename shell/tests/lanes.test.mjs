import test from 'node:test';
import assert from 'node:assert/strict';
import * as format from '../kit/GhostKit/lib/format.mjs';
import { LANES, laneText, vpnUpCount, spanSeconds, subParts } from '../../widgets/telemetry-lane/lanes.mjs';

const { laneValue, laneSub } = laneText(format);

const VPN_UP = { name: 'Work', kind: 'openvpn', iface: 'tun0', state: 'up' };
const VPN_DOWN = { name: 'Home', kind: 'wireguard', iface: null, state: 'down' };
const VPN_CONNECTING = { name: 'Lab', kind: 'wireguard', iface: null, state: 'connecting' };
const TAILSCALE_UP = { address: '100.64.0.1', peers_online: 2, peers_total: 5, state: 'up' };
const TAILSCALE_DOWN = { address: null, peers_online: 0, peers_total: 5, state: 'down' };
const LINKS_TWO_UP = { lan: [], vpns: [VPN_UP, VPN_DOWN], tailscale: TAILSCALE_UP, ping_ms: 12 };

test('lane values', () => {
  assert.equal(laneValue('cpu', { cpu: { load: 52.4 }, tctl: 58.2 }), '52% · 58°');
  assert.equal(laneValue('mem', { mem: { used_kib: 20761804 } }), '19.8 G');
  assert.equal(laneValue('disk', { read: 5.4e6, write: 2.1e6 }), 'R5 W2');
  assert.equal(laneValue('therm', { tctl: 58, gpuT: 41 }), '58° / 41°');
});

test('lane subs', () => {
  assert.equal(laneSub('cpu', { cores: 16 }, 150), 'HEAT · 16 CORES · ~150 S');
  assert.equal(laneSub('mem', { mem: { total_kib: 96364134 } }), '/ 91.9 GiB');
  assert.equal(laneSub('net', { links: LINKS_TWO_UP }), 'VPN 2 UP');
});

test('missing data reads --', () => {
  assert.equal(laneValue('gpu', { gpu: { present: false } }), '--');
});

test('span', () => assert.equal(spanSeconds(150, 1000), 150));

test('the gpu value is its load and temperature', () => {
  assert.equal(laneValue('gpu', { gpu: { present: true, load: 9.2, temp_c: 40.6 } }), '9% · 41°');
});

test('the network value is the short download and upload rates, drawn by the widget with chevrons', () => {
  assert.deepEqual(laneValue('net', { net: { rx_bps: 25400, tx_bps: 2100 } }), { down: '25K', up: '2K' });
  assert.deepEqual(laneValue('net', { net: null }), { down: '--', up: '--' });
});

test('a lane whose topic has not arrived reads --', () => {
  assert.equal(laneValue('cpu', { cpu: null, tctl: 58 }), '--');
  assert.equal(laneValue('gpu', { gpu: null }), '--');
  assert.equal(laneValue('mem', { mem: null }), '--');
});

test('a missing reading reads -- in its own place, so the others stay readable', () => {
  assert.equal(laneValue('cpu', { cpu: { load: 52.4 }, tctl: null }), '52% · --');
  assert.equal(laneValue('disk', { read: 5.4e6, write: null }), 'R5 W--');
  assert.equal(laneValue('therm', { tctl: 58, gpuT: null }), '58° / --');
});

test('the gpu, disk and thermal subs are fixed', () => {
  assert.equal(laneSub('gpu', {}), 'LOAD');
  assert.equal(laneSub('disk', {}), 'READ · WRITE');
  assert.equal(laneSub('therm', {}), 'CPU · GPU · NVME');
});

test('subs read -- for what has not arrived', () => {
  assert.equal(laneSub('cpu', { cores: null }, 150), 'HEAT · -- CORES · ~150 S');
  assert.equal(laneSub('mem', { mem: null }), '/ -- GiB');
  assert.equal(laneSub('net', { links: null }), 'VPN -- UP');
});

test('vpnUpCount counts the VPNs that are up, and Tailscale when it is up', () => {
  assert.equal(vpnUpCount(LINKS_TWO_UP), 2);
  assert.equal(vpnUpCount({ lan: [], vpns: [VPN_UP, VPN_CONNECTING], tailscale: TAILSCALE_DOWN, ping_ms: null }), 1);
  assert.equal(vpnUpCount({ lan: [], vpns: [], tailscale: null, ping_ms: null }), 0);
});

test('vpnUpCount is null before the links arrive', () => {
  assert.equal(vpnUpCount(null), null);
  assert.equal(vpnUpCount(undefined), null);
});

test('the span is the columns times the step, in whole seconds', () => {
  assert.equal(spanSeconds(222, 1000), 222);
  assert.equal(spanSeconds(150, 500), 75);
  assert.equal(spanSeconds(3, 333), 1);
});

test('the mirror lanes put a down chevron before the first word of the sub and an up chevron before the second', () => {
  assert.deepEqual(subParts('net', 'VPN 2 UP'), [
    { glyph: 'chevron-down', text: 'DOWN' },
    { glyph: 'chevron-up', text: 'UP' },
    { glyph: '', text: 'VPN 2 UP' },
  ]);
  assert.deepEqual(subParts('disk', 'READ · WRITE'), [
    { glyph: 'chevron-down', text: 'READ' },
    { glyph: 'chevron-up', text: 'WRITE' },
  ]);
});

test('the other lanes show their sub without chevrons', () => {
  const parts = subParts('cpu', 'HEAT · 16 CORES · ~150 S');
  assert.ok(parts.every((part) => part.glyph === ''));
  assert.equal(parts.map((part) => part.text).join(' · '), 'HEAT · 16 CORES · ~150 S');
  assert.deepEqual(subParts('gpu', 'LOAD'), [{ glyph: '', text: 'LOAD' }]);
});

test('every lane has the mockup name and tag key', () => {
  assert.deepEqual(
    Object.keys(LANES).map((metric) => [metric, LANES[metric].name, LANES[metric].tag]),
    [
      ['cpu', 'CPU', 'cpu'],
      ['gpu', 'GPU', 'gpu'],
      ['mem', 'MEMORY', 'mem'],
      ['net', 'NETWORK', 'net'],
      ['disk', 'DISK I/O', 'disk'],
      ['therm', 'THERMALS', 'sens'],
    ],
  );
});
