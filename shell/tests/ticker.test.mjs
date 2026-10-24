import test from 'node:test';
import assert from 'node:assert/strict';
import { tickerSegments, tickerText } from '../kit/GhostKit/lib/ticker.mjs';

const NOW_S = 1000;
const MOCKUP_FIXTURE = {
  ticker: { failed_units: 0, firewalld: true, updates: 16, nvidia_driver: '615.78.08' },
  host: { kernel: '7.2.11-200.fc44.x86_64' },
  links: {
    lan: [{ iface: 'enp14s0', address: '192.168.15.5', speed_mbps: 1000, state: 'up' }],
    vpns: [],
    tailscale: { address: '100.122.159.93', peers_online: 3, peers_total: 5, state: 'up' },
  },
  disks: { drives: [{ kernel_name: 'nvme0n1', state: 'live' }, { kernel_name: 'nvme1n1', state: 'dead' }] },
  alerts: { alerts: [] },
  nowS: NOW_S,
};

test('ticker reads like the mockup', () => assert.equal(tickerText(tickerSegments(MOCKUP_FIXTURE)), 'SYSTEMD // 0 FAILED UNITS // FIREWALLD ACTIVE // KERNEL 7.2.11-200.FC44 // NVIDIA 615.78.08 // ENP14S0 192.168.15.5 1GBE // TAILSCALE0 100.122.159.93 // NVME1 CONTROLLER DEAD // 16 UPDATES PENDING //'));

test('failed units turn crit', () => {
  const [systemd] = tickerSegments({ ...MOCKUP_FIXTURE, ticker: { ...MOCKUP_FIXTURE.ticker, failed_units: 2 } });
  assert.deepEqual(systemd, { text: 'SYSTEMD // 2 FAILED UNITS', level: 'crit' });
});

test('an inactive firewall warns', () => {
  const segments = tickerSegments({ ...MOCKUP_FIXTURE, ticker: { firewalld: false } });
  assert.deepEqual(segments[0], { text: 'FIREWALLD INACTIVE', level: 'warn' });
});

test('unknown parts are left out', () => {
  const texts = tickerSegments({ ticker: {}, nowS: NOW_S }).map((segment) => segment.text);
  assert.deepEqual(texts, []);
  const withHost = tickerSegments({ ticker: {}, host: MOCKUP_FIXTURE.host, nowS: NOW_S }).map((segment) => segment.text);
  assert.deepEqual(withHost, ['KERNEL 7.2.11-200.FC44']);
});

test('a dead drive is crit and a down link is left out', () => {
  const segments = tickerSegments({ ...MOCKUP_FIXTURE, links: { lan: [{ iface: 'eth0', address: '10.0.0.2', speed_mbps: -1, state: 'down' }], vpns: [], tailscale: null } });
  assert.deepEqual(segments.filter((segment) => segment.level === 'crit').map((segment) => segment.text), ['NVME1 CONTROLLER DEAD']);
  assert.ok(!segments.some((segment) => segment.text.startsWith('ETH0')));
});

test('vpns read up or down', () => {
  const links = { lan: [], vpns: [{ name: 'Work', state: 'up' }, { name: 'Home', state: 'down' }], tailscale: null };
  const texts = tickerSegments({ ticker: {}, links, nowS: NOW_S }).map((segment) => segment.text);
  assert.deepEqual(texts, ['WORK UP', 'HOME DOWN']);
});

test('active weather alerts carry their level and expired ones drop', () => {
  const alerts = { alerts: [
    { id: 'a', title: 'STORM', severity: 'DANGER', level: 'crit', start_s: 0, end_s: NOW_S + 10 },
    { id: 'b', title: 'HEAT', severity: 'WATCH', level: 'warn', start_s: 0, end_s: NOW_S - 1 },
  ] };
  assert.deepEqual(tickerSegments({ ticker: {}, alerts, nowS: NOW_S }), [{ text: 'STORM DANGER', level: 'crit' }]);
});

test('a single segment gets its trailing separator', () => assert.equal(tickerText([{ text: 'A', level: 'ok' }]), 'A //'));
