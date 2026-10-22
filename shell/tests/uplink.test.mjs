import test from 'node:test';
import assert from 'node:assert/strict';
import { footerText, homeLabel, hasHome } from '../../widgets/uplink/uplink.mjs';

test('footer', () => {
  assert.equal(footerText({ total: 6, attribution: 'IP GEOLOCATION BY DB-IP' }), '6 ACTIVE ENDPOINTS · IP GEOLOCATION BY DB-IP');
  assert.equal(footerText({ total: 0, attribution: '' }), 'NO GEO-IP DATABASE · SEE README');
});

test('home', () => {
  assert.equal(homeLabel({ hostname: 'tkacz-workstation' }), 'TKACZ-WORKSTATION');
  assert.equal(hasHome({ location: { lat: -23.55, lon: null } }), false);
  assert.equal(hasHome({ location: { lat: -23.55, lon: -46.63 } }), true);
});
