import test from 'node:test';
import assert from 'node:assert/strict';
import { metaLine, linkRows, pingText, vpnLine, tsLine, defaultUp } from '../../widgets/network/network.mjs';

const NET = { default_iface: 'enp14s0', rx_bps: 21000, tx_bps: 3000, interfaces: [] };
const LINKS = {
  lan: [{ iface: 'enp14s0', address: '192.168.15.5/24', speed_mbps: 1000, state: 'up' }],
  vpns: [{ name: 'OpenVPN Flxpoint', kind: 'openvpn', iface: 'tun0', state: 'up' }],
  tailscale: { address: '100.122.159.93', peers_online: 0, peers_total: 1, state: 'up' },
  ping_ms: 12,
};

test('meta line', () => {
  assert.equal(metaLine(NET, LINKS), 'ENP14S0 · 1 GbE');
  assert.equal(metaLine({ default_iface: null }, LINKS), '--');
});

test('link rows match the mockup', () =>
  assert.deepEqual(linkRows(LINKS), [
    { name: 'ENP14S0', detail: '192.168.15.5/24 · 1 GbE', state: 'UP' },
    { name: 'OPENVPN', detail: 'Flxpoint · tun0', state: 'ACTIVE' },
    { name: 'TAILSCALE', detail: '100.122.159.93 · 0/1 peers', state: 'UP' },
  ]));

test('a down vpn and no tailscale', () => {
  const rows = linkRows({ lan: [], vpns: [{ name: 'Work', kind: 'openvpn', iface: null, state: 'down' }], tailscale: null });
  assert.deepEqual(rows, [{ name: 'OPENVPN', detail: 'Work', state: 'DOWN' }]);
});

test('scope lines', () => {
  assert.equal(vpnLine(LINKS), 'OPENVPN · FLXPOINT');
  assert.equal(tsLine(LINKS), '0/1 PEERS');
  assert.equal(pingText(null), '--');
});

test('link speeds read in GbE from a gigabit and in Mb/s below', () => {
  const at = (speed) => metaLine(NET, { ...LINKS, lan: [{ ...LINKS.lan[0], speed_mbps: speed }] });
  assert.equal(at(10000), 'ENP14S0 · 10 GbE');
  assert.equal(at(2500), 'ENP14S0 · 2.5 GbE');
  assert.equal(at(100), 'ENP14S0 · 100 Mb/s');
  assert.equal(at(null), 'ENP14S0');
});

test('the meta line names the default interface before the links arrive', () => {
  assert.equal(metaLine(NET, null), 'ENP14S0');
  assert.equal(metaLine(null, LINKS), '--');
});

test('a connecting link and an active vpn read their states', () => {
  const rows = linkRows({
    lan: [{ iface: 'wlp3s0', address: null, speed_mbps: null, state: 'connecting' }],
    vpns: [{ name: 'Home', kind: 'wireguard', iface: 'wg0', state: 'connecting' }],
    tailscale: { address: null, peers_online: 2, peers_total: 5, state: 'down' },
  });
  assert.deepEqual(rows, [
    { name: 'WLP3S0', detail: '', state: 'CONNECTING' },
    { name: 'WIREGUARD', detail: 'Home · wg0', state: 'CONNECTING' },
    { name: 'TAILSCALE', detail: '2/5 peers', state: 'DOWN' },
  ]);
});

test('a vpn name keeps a kind that is not its first word', () => {
  const rows = linkRows({ lan: [], vpns: [{ name: 'openvpnish office', kind: 'openvpn', iface: null, state: 'up' }], tailscale: null });
  assert.equal(rows[0].detail, 'openvpnish office');
  assert.equal(vpnLine({ lan: [], vpns: [{ name: 'OPENVPN', kind: 'openvpn', iface: null, state: 'up' }], tailscale: null }), 'OPENVPN');
});

test('the vpn line takes the first vpn that is up', () => {
  const vpns = [
    { name: 'Lab', kind: 'wireguard', iface: null, state: 'down' },
    { name: 'WireGuard Home', kind: 'wireguard', iface: 'wg0', state: 'up' },
  ];
  assert.equal(vpnLine({ lan: [], vpns, tailscale: null }), 'WIREGUARD · HOME');
  assert.equal(vpnLine({ lan: [], vpns: [vpns[0]], tailscale: null }), '--');
});

test('missing links read --', () => {
  assert.deepEqual(linkRows(null), []);
  assert.equal(vpnLine(null), '--');
  assert.equal(tsLine(null), '--');
  assert.equal(tsLine({ lan: [], vpns: [], tailscale: null }), '--');
});

test('ping rounds to whole milliseconds', () => {
  assert.equal(pingText(12), '12 ms');
  assert.equal(pingText(9.6), '10 ms');
  assert.equal(pingText(undefined), '--');
});

test('the link is online only while the default interface is up', () => {
  const up = { name: 'enp14s0', oper_up: true, rx_bps: 0, tx_bps: 0 };
  assert.equal(defaultUp({ ...NET, interfaces: [up] }), true);
  assert.equal(defaultUp({ ...NET, interfaces: [{ ...up, oper_up: false }] }), false);
  assert.equal(defaultUp({ ...NET, default_iface: null, interfaces: [up] }), false);
  assert.equal(defaultUp(null), false);
});

test('a link reporting no speed, as a down one reports 0 or -1, shows none', () => {
  const at = (speed) => metaLine(NET, { ...LINKS, lan: [{ ...LINKS.lan[0], speed_mbps: speed }] });
  assert.equal(at(0), 'ENP14S0');
  assert.equal(at(-1), 'ENP14S0');
  const down = linkRows({ ...LINKS, lan: [{ ...LINKS.lan[0], speed_mbps: 0, state: 'down' }] });
  assert.equal(down[0].detail, '192.168.15.5/24');
});

test('the scope counts peers only while Tailscale is up', () => {
  const at = (state) => tsLine({ ...LINKS, tailscale: { ...LINKS.tailscale, state } });
  assert.equal(at('up'), '0/1 PEERS');
  assert.equal(at('down'), '--');
  assert.equal(at('connecting'), '--');
});
