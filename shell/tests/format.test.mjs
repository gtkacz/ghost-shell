import test from 'node:test';
import assert from 'node:assert/strict';
import {
  pct, tempC, tempShort, gib, rate, ghz, rpm, watts, ddmmyy, countdown, uptime, ago, utcOffset, dow, clockParts, uptimeSince,
} from '../kit/GhostKit/lib/format.mjs';

test('pct rounds', () => assert.equal(pct(22.6), '23%'));

test('pct keeps the requested decimals', () => {
  assert.equal(pct(12.44, 1), '12.4%');
  assert.equal(pct(0.06, 1), '0.1%');
});

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

test('rate picks the tier from the rounded value so the field never widens', () => {
  assert.equal(rate(999.4), '999 B/s');
  assert.equal(rate(999.6), '1.0 KB/s');
  assert.equal(rate(999949), '999.9 KB/s');
  assert.equal(rate(999950), '1.0 MB/s');
  assert.equal(rate(999999), '1.0 MB/s');
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

test('watts round to whole watts', () => assert.equal(watts(54.4), '54 W'));

test('ddmmyy is zero-padded local time', () => {
  assert.equal(ddmmyy(new Date(2026, 9, 20)), '20.10.26');
  assert.equal(ddmmyy(new Date(2026, 0, 5)), '05.01.26');
});

test('ddmmyy renders an invalid date as dashes', () => {
  assert.equal(ddmmyy(new Date('not a date')), '--');
});

test('every formatter renders a missing value as dashes', () => {
  const formatters = [
    pct, tempC, tempShort, gib, rate, ghz, rpm, watts, ddmmyy, countdown, uptime, ago, utcOffset, dow,
  ];
  for (const format of formatters) {
    assert.equal(format(null), '--');
    assert.equal(format(undefined), '--');
    assert.equal(format(NaN), '--');
  }
});

test('countdown to start then end', () => {
  assert.equal(countdown(0, 3661000, 7200000), '01:01:01');
  assert.equal(countdown(3661000, 3661000, 7200000), '00:58:59');
  assert.equal(countdown(9e6, 0, 7200000), '00:00:00');
  assert.equal(countdown(0, 360000000, 4e8), '100:00:00');
});

test('countdown drops the partial second so the field never runs ahead', () => {
  assert.equal(countdown(0, 1999, 7200000), '00:00:01');
  assert.equal(countdown(0, 999, 7200000), '00:00:00');
});

test('countdown is dashes without a current time or a target', () => {
  assert.equal(countdown(0, 5000, NaN), '00:00:05');
  assert.equal(countdown(10000, 5000, NaN), '--');
  assert.equal(countdown(NaN, 5000, 9000), '--');
});

test('ago buckets', () => {
  assert.equal(ago(59), 'NOW');
  assert.equal(ago(60), '1M');
  assert.equal(ago(3599), '59M');
  assert.equal(ago(3600), '1H');
  assert.equal(ago(86399), '23H');
  assert.equal(ago(86400), '1D');
  assert.equal(ago(86400 * 3 + 5000), '3D');
});

test('ago treats a time in the future as now', () => assert.equal(ago(-30), 'NOW'));

test('utc offsets', () => {
  assert.equal(utcOffset(-180), 'UTC\u22123');
  assert.equal(utcOffset(330), 'UTC+5:30');
  assert.equal(utcOffset(0), 'UTC+0');
});

test('utc offsets keep a zero-padded minute and a real minus sign', () => {
  assert.equal(utcOffset(-210), 'UTC\u22123:30');
  assert.equal(utcOffset(545), 'UTC+9:05');
  assert.equal(utcOffset(840), 'UTC+14');
  assert.notEqual(utcOffset(-180), 'UTC-3');
});

test('uptime keeps counting hours', () => {
  assert.equal(uptime(180727), '50:12:07');
  assert.equal(uptime(0), '00:00:00');
  assert.equal(uptime(8047), '02:14:07');
  assert.equal(uptime(360000), '100:00:00');
});

test('dow is the three-letter weekday of local time', () => {
  assert.equal(dow(new Date(2026, 9, 21)), 'WED');
  assert.equal(dow(new Date(2026, 9, 18)), 'SUN');
  assert.equal(dow(new Date(2026, 9, 24)), 'SAT');
});

test('dow renders an invalid date as dashes', () => assert.equal(dow(new Date('not a date')), '--'));

test('clock parts are zero-padded local time on a 24-hour clock', () => {
  assert.deepEqual(clockParts(new Date(2026, 9, 21, 7, 5, 9)), { hours: '07', minutes: '05', seconds: '09' });
  assert.deepEqual(clockParts(new Date(2026, 9, 21, 0, 0, 0)), { hours: '00', minutes: '00', seconds: '00' });
  assert.deepEqual(clockParts(new Date(2026, 9, 21, 23, 59, 59)), { hours: '23', minutes: '59', seconds: '59' });
});

test('uptime is the seconds since boot', () => {
  assert.equal(uptimeSince(1000, 9047), 8047);
  assert.equal(uptimeSince(1000, 1000), 0);
});

test('uptime is missing before the host arrives and when the boot time could not be read', () => {
  assert.ok(Number.isNaN(uptimeSince(null, 9047)));
  assert.ok(Number.isNaN(uptimeSince(undefined, 9047)));
  assert.ok(Number.isNaN(uptimeSince(0, 9047)));
});

test('an uptime behind the boot time reads as zero', () => assert.equal(uptime(uptimeSince(9047, 1000)), '00:00:00'));

test('a missing uptime reads as dashes', () => assert.equal(uptime(uptimeSince(null, 9047)), '--'));
