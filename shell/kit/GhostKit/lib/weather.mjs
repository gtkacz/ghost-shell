import { pct, tempShort } from './format.mjs';

const MISSING = '--';
const SEPARATOR = ' · ';
const SECONDS_PER_MINUTE = 60;
const MINUTES_PER_HOUR = 60;
const SECONDS_PER_HOUR = SECONDS_PER_MINUTE * MINUTES_PER_HOUR;
const HOURS_PER_DAY = 24;
const SECONDS_PER_DAY = SECONDS_PER_HOUR * HOURS_PER_DAY;
const CLOCK_FIELD_WIDTH = 2;
const ZERO_PAD = '0';
const PERCENT = 100;
const CRIT = 'crit';
const CRIT_RANK = 0;
const OTHER_RANK = 1;

// The six hour cells fit the mockup's six-column grid.
const FORECAST_HOURS = 6;

// The INMET provider fetches every 10 minutes (spec §12). The shell cannot read a provider's cadence, so this
// repeats it; an external provider that publishes more rarely will read as stale sooner than it should.
export const ALERTS_CADENCE_S = 10 * SECONDS_PER_MINUTE;
// One missed fetch is an ordinary retry and two a bad stretch of network, so only a third marks the provider as
// stuck, which is when an alert it published can no longer be trusted to still be in force.
const ALERTS_MISSED_CADENCES = 3;
export const ALERTS_STALE_AFTER_S = ALERTS_MISSED_CADENCES * ALERTS_CADENCE_S;

function padded(number) {
  return (ZERO_PAD + number).slice(-CLOCK_FIELD_WIDTH);
}

// Providers are external programs, so an alert without a usable window must not reach the countdown or outlive its
// end.
function hasWindow(alert) {
  return alert !== null && typeof alert === 'object' && Number.isFinite(alert.start_s) && Number.isFinite(alert.end_s);
}

function rank(alert) {
  return alert.level === CRIT ? CRIT_RANK : OTHER_RANK;
}

// Takes the alerts array, so a topic that never published (null) and one that published no alerts read the same.
export function activeAlerts(alerts, nowS) {
  if (!Array.isArray(alerts)) {
    return [];
  }
  return alerts
    .filter(alert => hasWindow(alert) && alert.end_s > nowS)
    .sort((a, b) => rank(a) - rank(b) || a.start_s - b.start_s);
}

// A provider that has not published for this long is stuck, whatever the daemon's own link says.
export function alertsStale(alerts, nowS) {
  const updated = alerts === null || alerts === undefined ? undefined : alerts.updated_s;
  return !Number.isFinite(updated) || nowS - updated > ALERTS_STALE_AFTER_S;
}

// With no provider configured the daemon never claims weather.alerts, so the box has nothing to show; before the
// first config arrives that is also the answer.
export function alertsEnabled(config) {
  if (config === null || config === undefined || !config.location || !config.location.alerts) {
    return false;
  }
  return Boolean(config.location.alerts.provider);
}

export function alertLine(alert, source) {
  return [source, alert.title, alert.severity].filter(Boolean).join(SEPARATOR);
}

function localSecondOfDay(atS, offsetMin) {
  const local = atS + offsetMin * SECONDS_PER_MINUTE;
  return ((local % SECONDS_PER_DAY) + SECONDS_PER_DAY) % SECONDS_PER_DAY;
}

function localHour(atS, offsetMin) {
  return Math.floor(localSecondOfDay(atS, offsetMin) / SECONDS_PER_HOUR);
}

function clockText(atS, offsetMin) {
  const second = localSecondOfDay(atS, offsetMin);
  const minute = Math.floor(second / SECONDS_PER_MINUTE) % MINUTES_PER_HOUR;
  return padded(localHour(atS, offsetMin)) + ':' + padded(minute);
}

export function hourLabel(atS, offsetMin) {
  return Number.isFinite(atS) ? padded(localHour(atS, offsetMin)) + 'H' : MISSING;
}

export function hiLo(w) {
  return 'H ' + tempShort(w.high_c) + SEPARATOR + 'L ' + tempShort(w.low_c);
}

function windText(kmh) {
  return Number.isFinite(kmh) ? Math.round(kmh) + ' KM/H' : MISSING;
}

export function detailLine(w, offsetMin) {
  if (!w) {
    return '';
  }
  const parts = ['HUM ' + pct(w.humidity_pct), 'WIND ' + windText(w.wind_kmh)];
  if (w.rain_peak) {
    parts.push('RAIN ' + pct(w.rain_peak.pct) + ' AT ' + hourLabel(w.rain_peak.at_s, offsetMin));
  }
  return parts.join(SEPARATOR);
}

export function skyLine(sky, offsetMin) {
  if (!sky) {
    return '';
  }
  const parts = [];
  if (Number.isFinite(sky.sunrise_s) && Number.isFinite(sky.sunset_s)) {
    parts.push('SUN ' + clockText(sky.sunrise_s, offsetMin) + ' / ' + clockText(sky.sunset_s, offsetMin));
  }
  parts.push(['MOON ' + pct(sky.moon_illumination * PERCENT), sky.moon_name].filter(Boolean).join(' '));
  return parts.join(SEPARATOR);
}

// The daemon cuts the hours that have passed, and a rain peak that has, only when it fetches, which can be a quarter
// of an hour before they are shown. An hour is over an hour after its start.
function hasNotEnded(atS, nowS) {
  return atS + SECONDS_PER_HOUR > nowS;
}

export function forecastFrom(w, nowS) {
  if (!w) {
    return null;
  }
  const hours = Array.isArray(w.hours) ? w.hours.filter(hour => hasNotEnded(hour.at_s, nowS)).slice(0, FORECAST_HOURS) : [];
  const peak = w.rain_peak && hasNotEnded(w.rain_peak.at_s, nowS) ? w.rain_peak : null;
  // Object.assign rather than a spread, which Qt's JavaScript engine cannot parse.
  return Object.assign({}, w, { hours, rain_peak: peak });
}
