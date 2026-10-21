const MISSING = '--';
const BYTES_PER_KB = 1e3;
const BYTES_PER_MB = 1e6;
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

export function rate(bps) {
  if (isMissing(bps)) {
    return MISSING;
  }
  if (bps < BYTES_PER_KB) {
    return Math.round(bps) + ' B/s';
  }
  if (bps < BYTES_PER_MB) {
    return (bps / BYTES_PER_KB).toFixed(1) + ' KB/s';
  }
  return (bps / BYTES_PER_MB).toFixed(1) + ' MB/s';
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
