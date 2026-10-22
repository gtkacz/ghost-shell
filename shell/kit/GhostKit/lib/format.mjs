const MISSING = '--';
const BYTES_PER_KB = 1e3;
const BYTES_PER_MB = 1e6;
const KB_PER_MB = BYTES_PER_MB / BYTES_PER_KB;
const TENTHS_PER_UNIT = 10;
const KIB_PER_GIB = 1024 * 1024;
const MHZ_PER_GHZ = 1000;
const DATE_FIELD_WIDTH = 2;
const ZERO_PAD = '0';
const MS_PER_SECOND = 1000;
const SECONDS_PER_MINUTE = 60;
const MINUTES_PER_HOUR = 60;
const HOURS_PER_DAY = 24;
const SECONDS_PER_HOUR = SECONDS_PER_MINUTE * MINUTES_PER_HOUR;
const SECONDS_PER_DAY = SECONDS_PER_HOUR * HOURS_PER_DAY;
const CLOCK_HOURS_WIDTH = 2;
const AGO_NOW = 'NOW';
const MINUS_SIGN = '\u2212';
const WEEKDAYS = Object.freeze(['SUN', 'MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT']);

function isMissing(value) {
  return !Number.isFinite(value);
}

function padded(number) {
  return (ZERO_PAD + number).slice(-DATE_FIELD_WIDTH);
}

export function pct(value, dp = 0) {
  return isMissing(value) ? MISSING : value.toFixed(dp) + '%';
}

export function tempC(value) {
  return isMissing(value) ? MISSING : Math.round(value) + '°C';
}

export function tempShort(value) {
  return isMissing(value) ? MISSING : Math.round(value) + '°';
}

export function gib(kib, dp = 1) {
  return isMissing(kib) ? MISSING : (kib / KIB_PER_GIB).toFixed(dp);
}

// Rounding to a tenth happens before the tier is chosen, otherwise 999.95 KB/s would print as "1000.0 KB/s" and
// widen a field whose numbers must never shift.
function tenthsOf(bps, unitBytes) {
  return Math.round(bps / (unitBytes / TENTHS_PER_UNIT));
}

export function rate(bps) {
  if (isMissing(bps)) {
    return MISSING;
  }
  const bytes = Math.round(bps);
  if (bytes < BYTES_PER_KB) {
    return bytes + ' B/s';
  }
  const kbTenths = tenthsOf(bps, BYTES_PER_KB);
  if (kbTenths < KB_PER_MB * TENTHS_PER_UNIT) {
    return (kbTenths / TENTHS_PER_UNIT).toFixed(1) + ' KB/s';
  }
  return (tenthsOf(bps, BYTES_PER_MB) / TENTHS_PER_UNIT).toFixed(1) + ' MB/s';
}

// The mockup's `short`, for narrow fields: whole kilobytes, or megabytes to a tenth. As in rate, the tier follows
// the rounded value, so 999.5 KB/s reads "1.0M" rather than "1000K".
export function rateShort(bps) {
  if (isMissing(bps)) {
    return MISSING;
  }
  const kb = Math.round(bps / BYTES_PER_KB);
  if (kb < KB_PER_MB) {
    return kb + 'K';
  }
  return (tenthsOf(bps, BYTES_PER_MB) / TENTHS_PER_UNIT).toFixed(1) + 'M';
}

export function ghz(mhz) {
  return isMissing(mhz) ? MISSING : (mhz / MHZ_PER_GHZ).toFixed(2) + ' GHz';
}

export function rpm(value) {
  return isMissing(value) ? MISSING : Math.round(value) + ' RPM';
}

export function watts(value) {
  return isMissing(value) ? MISSING : Math.round(value) + ' W';
}

export function ddmmyy(date) {
  if (!(date instanceof Date) || isMissing(date.getTime())) {
    return MISSING;
  }
  const year = date.getFullYear() % 100;
  return padded(date.getDate()) + '.' + padded(date.getMonth() + 1) + '.' + padded(year);
}

// `padded` keeps the last two digits, which would wrap an uptime past 99 hours.
function paddedHours(hours) {
  const text = String(hours);
  return text.length < CLOCK_HOURS_WIDTH ? ZERO_PAD + text : text;
}

function clockText(totalSeconds) {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(seconds / SECONDS_PER_HOUR);
  const minutes = Math.floor(seconds / SECONDS_PER_MINUTE) % MINUTES_PER_HOUR;
  return paddedHours(hours) + ':' + padded(minutes) + ':' + padded(seconds % SECONDS_PER_MINUTE);
}

export function countdown(nowMs, startMs, endMs) {
  const targetMs = nowMs < startMs ? startMs : endMs;
  if (isMissing(nowMs) || isMissing(targetMs)) {
    return MISSING;
  }
  return clockText((targetMs - nowMs) / MS_PER_SECOND);
}

export function uptime(seconds) {
  return isMissing(seconds) ? MISSING : clockText(seconds);
}

// The host feed publishes 0 when it cannot read the boot time, which would otherwise read as decades of uptime.
export function uptimeSince(bootTimeS, nowS) {
  if (!Number.isFinite(bootTimeS) || bootTimeS <= 0) {
    return NaN;
  }
  return nowS - bootTimeS;
}

export function clockParts(date) {
  return {
    hours: padded(date.getHours()),
    minutes: padded(date.getMinutes()),
    seconds: padded(date.getSeconds()),
  };
}

export function ago(seconds) {
  if (isMissing(seconds)) {
    return MISSING;
  }
  if (seconds < SECONDS_PER_MINUTE) {
    return AGO_NOW;
  }
  if (seconds < SECONDS_PER_HOUR) {
    return Math.floor(seconds / SECONDS_PER_MINUTE) + 'M';
  }
  if (seconds < SECONDS_PER_DAY) {
    return Math.floor(seconds / SECONDS_PER_HOUR) + 'H';
  }
  return Math.floor(seconds / SECONDS_PER_DAY) + 'D';
}

export function utcOffset(minutesEast) {
  if (isMissing(minutesEast)) {
    return MISSING;
  }
  const sign = minutesEast < 0 ? MINUS_SIGN : '+';
  const absolute = Math.abs(minutesEast);
  const hours = Math.floor(absolute / MINUTES_PER_HOUR);
  const minutes = absolute % MINUTES_PER_HOUR;
  return 'UTC' + sign + hours + (minutes === 0 ? '' : ':' + padded(minutes));
}

export function dow(date) {
  if (!(date instanceof Date) || isMissing(date.getTime())) {
    return MISSING;
  }
  return WEEKDAYS[date.getDay()];
}
