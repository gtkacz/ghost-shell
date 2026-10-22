import test from 'node:test';
import assert from 'node:assert/strict';
import { footerText, homeLabel, hasHome, shownEndpoints, labelX, MAX_ENDPOINTS } from '../../widgets/uplink/uplink.mjs';
import { MAX_ENDPOINTS as GLOBE_MAX_ENDPOINTS } from '../kit/GhostKit/lib/globe.mjs';

test('footer', () => {
  assert.equal(footerText({ total: 6, attribution: 'IP GEOLOCATION BY DB-IP' }), '6 ACTIVE ENDPOINTS · IP GEOLOCATION BY DB-IP');
  assert.equal(footerText({ total: 0, attribution: '' }), 'NO GEO-IP DATABASE · SEE README');
});

test('home', () => {
  assert.equal(homeLabel({ hostname: 'tkacz-workstation' }), 'TKACZ-WORKSTATION');
  assert.equal(hasHome({ location: { lat: -23.55, lon: null } }), false);
  assert.equal(hasHome({ location: { lat: -23.55, lon: -46.63 } }), true);
});

test('endpoint labels read in capitals, as the mockup prints them', () => {
  const shown = shownEndpoints({ endpoints: [{ label: '2a03:2880:f12f::1', lat: 1, lon: 2 }, { label: 'GITHUB.COM', lat: 3, lon: 4 }] });
  assert.deepEqual(shown.map((endpoint) => endpoint.label), ['2A03:2880:F12F::1', 'GITHUB.COM']);
  assert.deepEqual(shown[0], { label: '2A03:2880:F12F::1', lat: 1, lon: 2 });
});

test('endpoints stop at the globe cap, which matches the kit globe', () => {
  const endpoints = Array.from({ length: MAX_ENDPOINTS + 3 }, (_, i) => ({ label: 'E' + i, lat: 0, lon: i }));
  assert.equal(shownEndpoints({ endpoints }).length, MAX_ENDPOINTS);
  assert.equal(MAX_ENDPOINTS, GLOBE_MAX_ENDPOINTS);
  assert.deepEqual(shownEndpoints(null), []);
});

test('a label that would run past the body turns to the left of its square', () => {
  assert.equal(labelX(100, 6, 50, 300), 106);
  assert.equal(labelX(250, 6, 50, 300), 194);
  assert.equal(labelX(244, 6, 50, 300), 250);
});
