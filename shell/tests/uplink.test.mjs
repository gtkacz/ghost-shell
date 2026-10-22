import test from 'node:test';
import assert from 'node:assert/strict';
import { footerText, homeLabel, hasHome, shownEndpoints, labelX, shownLabels, MAX_ENDPOINTS } from '../../widgets/uplink/uplink.mjs';
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

const rect = (x, y, w = 40, h = 10) => ({ x, y, w, h });

test('a label that overlaps a busier shown label is hidden, and one that clears every label is shown', () => {
  const rects = [rect(0, 0), rect(30, 5), rect(100, 0), rect(35, 8)];
  assert.deepEqual(shownLabels(rects, [], 1), [true, false, true, false]);
});

test('the first label, home, always shows, and a hidden label does not hide the ones below it', () => {
  const rects = [rect(0, 0), rect(10, 0), rect(45, 0)];
  assert.deepEqual(shownLabels(rects, [], 1), [true, false, true]);
});

test('a label with no rect, behind the globe, is not shown and hides nothing', () => {
  assert.deepEqual(shownLabels([null, rect(0, 0), rect(5, 0)], [], 1), [false, true, false]);
});

test('labels that only touch both show', () => {
  assert.deepEqual(shownLabels([rect(0, 0), rect(40, 0)], [], 0), [true, true]);
});

test('the margin keeps a label in its state through sub-pixel motion', () => {
  const touching = [rect(0, 0), rect(39.5, 0)];
  assert.deepEqual(shownLabels(touching, [true, true], 1), [true, true]);
  assert.deepEqual(shownLabels(touching, [true, false], 1), [true, false]);
  const clear = [rect(0, 0), rect(40.5, 0)];
  assert.deepEqual(shownLabels(clear, [true, false], 1), [true, false]);
  assert.deepEqual(shownLabels([rect(0, 0), rect(41.5, 0)], [true, false], 1), [true, true]);
  assert.deepEqual(shownLabels([rect(0, 0), rect(38.5, 0)], [true, true], 1), [true, false]);
});
