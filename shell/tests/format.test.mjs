import test from 'node:test';
import assert from 'node:assert/strict';
import { pct, tempC, tempShort, gib, rate, ghz, rpm, ddmmyy } from '../kit/GhostKit/lib/format.mjs';

test('pct rounds', () => assert.equal(pct(22.6), '23%'));

test('rate tiers', () => {
  assert.equal(rate(830), '830 B/s');
  assert.equal(rate(830000), '830.0 KB/s');
  assert.equal(rate(12400000), '12.4 MB/s');
});

test('rate switches tier exactly at the decimal boundaries', () => {
  assert.equal(rate(999), '999 B/s');
  assert.equal(rate(1000), '1.0 KB/s');
  assert.equal(rate(1000000), '1.0 MB/s');
});

test('temperatures round to whole degrees', () => {
  assert.equal(tempC(57.9), '58°C');
  assert.equal(tempShort(64), '64°');
});

test('gib converts kibibytes to a bare number string', () => {
  assert.equal(gib(96388184), '91.9');
  assert.equal(gib(96388184, 0), '92');
  assert.equal(gib(96388184, 2), '91.92');
});

test('ghz and rpm append their units', () => {
  assert.equal(ghz(5060), '5.06 GHz');
  assert.equal(rpm(1670), '1670 RPM');
});

test('ddmmyy is zero-padded local time', () => {
  assert.equal(ddmmyy(new Date(2026, 9, 20)), '20.10.26');
  assert.equal(ddmmyy(new Date(2026, 0, 5)), '05.01.26');
});

test('every formatter renders a missing value as dashes', () => {
  const formatters = [pct, tempC, tempShort, gib, rate, ghz, rpm, ddmmyy];
  for (const format of formatters) {
    assert.equal(format(null), '--');
    assert.equal(format(undefined), '--');
    assert.equal(format(NaN), '--');
  }
});
