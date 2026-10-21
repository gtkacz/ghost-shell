const MISSING = '--';
const BYTES_PER_KB = 1e3;
const BYTES_PER_MB = 1e6;
const KB_PER_MB = BYTES_PER_MB / BYTES_PER_KB;
const TENTHS_PER_UNIT = 10;
const KIB_PER_GIB = 1024 * 1024;
const MHZ_PER_GHZ = 1000;
const DATE_FIELD_WIDTH = 2;
const ZERO_PAD = '0';

function isMissing(value) {
  return !Number.isFinite(value);
}

function padded(number) {
  return (ZERO_PAD + number).slice(-DATE_FIELD_WIDTH);
}

export function pct(value) {
  return isMissing(value) ? MISSING : Math.round(value) + '%';
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

export function ghz(mhz) {
  return isMissing(mhz) ? MISSING : (mhz / MHZ_PER_GHZ).toFixed(2) + ' GHz';
}

export function rpm(value) {
  return isMissing(value) ? MISSING : Math.round(value) + ' RPM';
}

export function ddmmyy(date) {
  if (!(date instanceof Date) || isMissing(date.getTime())) {
    return MISSING;
  }
  const year = date.getFullYear() % 100;
  return padded(date.getDate()) + '.' + padded(date.getMonth() + 1) + '.' + padded(year);
}
