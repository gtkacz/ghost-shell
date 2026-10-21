import test from 'node:test';
import assert from 'node:assert/strict';
import {
  ALERTS_STALE_AFTER_S, activeAlerts, alertLine, alertsEnabled, alertsStale, detailLine, forecastFrom, hiLo, hourLabel,
  skyLine,
} from '../kit/GhostKit/lib/weather.mjs';

const HOUR_S = 3600;
const NOON_UTC_S = 1792584000;

test('expired alerts drop and crit sorts first', () => assert.deepEqual(activeAlerts([{ id: 'a', level: 'warn', start_s: 1, end_s: 99 }, { id: 'b', level: 'crit', start_s: 5, end_s: 99 }, { id: 'c', level: 'crit', start_s: 0, end_s: 10 }], 10).map(a => a.id), ['b', 'a']));
test('alert line', () => assert.equal(alertLine({ title: 'TEMPESTADE', severity: 'PERIGO' }, 'INMET'), 'INMET · TEMPESTADE · PERIGO'));
test('detail line', () => assert.equal(detailLine({ humidity_pct: 88, wind_kmh: 31.2, rain_peak: { pct: 95, at_s: 1792630800 } }, -180), 'HUM 88% · WIND 31 KM/H · RAIN 95% AT 22H'));
test('sky line', () => assert.equal(skyLine({ sunrise_s: 1792570620, sunset_s: 1792619520, moon_illumination: 0.64, moon_name: 'WAXING GIBBOUS' }, -180), 'SUN 05:17 / 18:52 · MOON 64% WAXING GIBBOUS'));
test('hour labels use local time', () => assert.equal(hourLabel(1792616400, -180), '18H'));
test('hi and lo', () => assert.equal(hiLo({ high_c: 22.4, low_c: 13.6 }), 'H 22° · L 14°'));

test('an alert ends at its end second, not after it', () => {
  const alert = { id: 'a', level: 'warn', start_s: 0, end_s: 50 };
  assert.deepEqual(activeAlerts([alert], 49), [alert]);
  assert.deepEqual(activeAlerts([alert], 50), []);
});

test('an alert that has not started is still active, so its countdown can run to the start', () => {
  const ahead = { id: 'a', level: 'warn', start_s: 500, end_s: 900 };
  assert.deepEqual(activeAlerts([ahead], 10), [ahead]);
});

test('same-level alerts sort by the earliest start', () => {
  const alerts = [
    { id: 'late', level: 'warn', start_s: 30, end_s: 99 },
    { id: 'early', level: 'warn', start_s: 10, end_s: 99 },
    { id: 'mid', level: 'warn', start_s: 20, end_s: 99 },
  ];
  assert.deepEqual(activeAlerts(alerts, 0).map(a => a.id), ['early', 'mid', 'late']);
});

test('a later crit outranks an earlier warn', () => {
  const alerts = [{ id: 'w', level: 'warn', start_s: 1, end_s: 99 }, { id: 'c', level: 'crit', start_s: 80, end_s: 99 }];
  assert.deepEqual(activeAlerts(alerts, 0).map(a => a.id), ['c', 'w']);
});

test('sorting does not reorder the topic value it was given', () => {
  const alerts = [{ id: 'w', level: 'warn', start_s: 1, end_s: 99 }, { id: 'c', level: 'crit', start_s: 2, end_s: 99 }];
  activeAlerts(alerts, 0);
  assert.deepEqual(alerts.map(a => a.id), ['w', 'c']);
});

test('a topic that never published, or published nothing, has no active alerts', () => {
  assert.deepEqual(activeAlerts(null, 10), []);
  assert.deepEqual(activeAlerts(undefined, 10), []);
  assert.deepEqual(activeAlerts([], 10), []);
  assert.deepEqual(activeAlerts({}, 10), []);
});

test('an alert without a usable window is dropped rather than shown forever', () => {
  const sound = { id: 'ok', level: 'warn', start_s: 1, end_s: 99 };
  const alerts = [{ id: 'no-end', level: 'crit', start_s: 1 }, null, { id: 'no-start', level: 'crit', end_s: 99 }, sound];
  assert.deepEqual(activeAlerts(alerts, 10), [sound]);
});

test('alert line leaves out a part the provider did not send', () => {
  assert.equal(alertLine({ title: 'TEMPESTADE', severity: '' }, 'INMET'), 'INMET · TEMPESTADE');
  assert.equal(alertLine({ title: 'TEMPESTADE' }, undefined), 'TEMPESTADE');
});

test('alerts go stale once they are older than three provider cadences', () => {
  const updated = { updated_s: 1000 };
  assert.equal(alertsStale(updated, 1000 + ALERTS_STALE_AFTER_S), false);
  assert.equal(alertsStale(updated, 1000 + ALERTS_STALE_AFTER_S + 1), true);
});

test('alerts with no update time cannot be shown as fresh', () => {
  assert.equal(alertsStale(null, 1000), true);
  assert.equal(alertsStale({}, 1000), true);
});

test('alerts are enabled only while the config names a provider', () => {
  assert.equal(alertsEnabled({ location: { alerts: { provider: 'inmet', area: '3550308' } } }), true);
  assert.equal(alertsEnabled({ location: { alerts: { provider: '', area: '' } } }), false);
});

test('alerts stay off until a config has arrived, and for a config without the section', () => {
  assert.equal(alertsEnabled(null), false);
  assert.equal(alertsEnabled({}), false);
  assert.equal(alertsEnabled({ location: {} }), false);
});

test('the rain part of the detail line is left out without a peak', () => {
  assert.equal(detailLine({ humidity_pct: 88, wind_kmh: 31.2, rain_peak: null }, -180), 'HUM 88% · WIND 31 KM/H');
  assert.equal(detailLine({ humidity_pct: 88, wind_kmh: 31.2 }, -180), 'HUM 88% · WIND 31 KM/H');
});

test('the detail line rounds wind to whole kilometres an hour', () => {
  assert.equal(detailLine({ humidity_pct: 50, wind_kmh: 31.5 }, 0), 'HUM 50% · WIND 32 KM/H');
});

test('a missing reading reads as dashes in the detail line', () => {
  assert.equal(detailLine({}, 0), 'HUM -- · WIND --');
  assert.equal(detailLine(null, 0), '');
});

test('the sun part of the sky line is left out when the sun does not cross the horizon', () => {
  assert.equal(skyLine({ sunrise_s: null, sunset_s: null, moon_illumination: 0.64, moon_name: 'WAXING GIBBOUS' }, -180), 'MOON 64% WAXING GIBBOUS');
});

test('the sky line shows the moon without a name when there is none', () => {
  assert.equal(skyLine({ sunrise_s: null, sunset_s: null, moon_illumination: 1, moon_name: '' }, 0), 'MOON 100%');
});

test('a sky that never published has no line', () => assert.equal(skyLine(null, -180), ''));

test('sun times follow the offset it is given', () => {
  const sky = { sunrise_s: 1792570620, sunset_s: 1792619520, moon_illumination: 0, moon_name: '' };
  assert.equal(skyLine(sky, 0), 'SUN 08:17 / 21:52 · MOON 0%');
  assert.equal(skyLine(sky, 330), 'SUN 13:47 / 03:22 · MOON 0%');
});

test('hour labels wrap around midnight in both directions', () => {
  assert.equal(hourLabel(0, -180), '21H');
  assert.equal(hourLabel(0, 0), '00H');
  assert.equal(hourLabel(23 * HOUR_S, 120), '01H');
});

test('hi and lo round to whole degrees and keep the sign', () => {
  assert.equal(hiLo({ high_c: 5.5, low_c: -3.6 }), 'H 6° · L -4°');
  assert.equal(hiLo({ high_c: 0.4, low_c: -0.4 }), 'H 0° · L 0°');
});

test('hi and lo read as dashes without a forecast', () => assert.equal(hiLo({}), 'H -- · L --'));

test('hours that have ended drop and the current hour stays', () => {
  const forecast = { hours: [{ at_s: NOON_UTC_S - HOUR_S }, { at_s: NOON_UTC_S }, { at_s: NOON_UTC_S + HOUR_S }], rain_peak: null };
  const now = NOON_UTC_S + 1800;
  assert.deepEqual(forecastFrom(forecast, now).hours.map(h => h.at_s), [NOON_UTC_S, NOON_UTC_S + HOUR_S]);
});

test('an hour ends exactly an hour after it starts', () => {
  const forecast = { hours: [{ at_s: NOON_UTC_S }], rain_peak: null };
  assert.equal(forecastFrom(forecast, NOON_UTC_S + HOUR_S - 1).hours.length, 1);
  assert.equal(forecastFrom(forecast, NOON_UTC_S + HOUR_S).hours.length, 0);
});

test('hours with gaps keep their own times and order', () => {
  const forecast = { hours: [{ at_s: NOON_UTC_S + HOUR_S }, { at_s: NOON_UTC_S + 4 * HOUR_S }], rain_peak: null };
  assert.deepEqual(forecastFrom(forecast, NOON_UTC_S).hours.map(h => h.at_s), [NOON_UTC_S + HOUR_S, NOON_UTC_S + 4 * HOUR_S]);
});

test('no more than six hours are kept', () => {
  const hours = Array.from({ length: 9 }, (_, i) => ({ at_s: NOON_UTC_S + i * HOUR_S }));
  assert.equal(forecastFrom({ hours, rain_peak: null }, NOON_UTC_S).hours.length, 6);
});

test('a rain peak that has passed is ignored and one ahead is kept', () => {
  const ahead = { pct: 95, at_s: NOON_UTC_S + 2 * HOUR_S };
  const passed = { pct: 95, at_s: NOON_UTC_S - 2 * HOUR_S };
  assert.deepEqual(forecastFrom({ hours: [], rain_peak: ahead }, NOON_UTC_S).rain_peak, ahead);
  assert.equal(forecastFrom({ hours: [], rain_peak: passed }, NOON_UTC_S).rain_peak, null);
});

test('the rest of the forecast passes through and the input is not changed', () => {
  const forecast = { temp_c: 18, condition: 'RAIN', hours: [{ at_s: NOON_UTC_S - 2 * HOUR_S }], rain_peak: null };
  const trimmed = forecastFrom(forecast, NOON_UTC_S);
  assert.equal(trimmed.temp_c, 18);
  assert.equal(trimmed.condition, 'RAIN');
  assert.equal(forecast.hours.length, 1);
});

test('a forecast without hours trims to none, and no forecast stays none', () => {
  assert.deepEqual(forecastFrom({ temp_c: 18 }, NOON_UTC_S).hours, []);
  assert.equal(forecastFrom(null, NOON_UTC_S), null);
});
