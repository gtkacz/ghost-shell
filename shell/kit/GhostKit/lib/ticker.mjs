import { activeAlerts } from './weather.mjs';

const SEPARATOR = ' // ';
const LINK_UP = 'up';
const DEAD = 'dead';
const MBPS_PER_GBPS = 1000;
const TENTHS_PER_UNIT = 10;
// The daemon reports no interface name for Tailscale; this is the one the client creates.
const TAILSCALE_IFACE = 'TAILSCALE0';
const KERNEL_ARCH = /\.(x86_64|aarch64|i686|arm64)$/i;
const NVME_NAMESPACE = /^(nvme\d+)n\d+$/i;

const ok = (text) => ({ text, level: 'ok' });
const known = (value) => value !== null && value !== undefined;

function speedText(mbps) {
  if (!Number.isFinite(mbps) || mbps <= 0) {
    return '';
  }
  if (mbps < MBPS_PER_GBPS) {
    return mbps + 'MB/S';
  }
  return Math.round((mbps / MBPS_PER_GBPS) * TENTHS_PER_UNIT) / TENTHS_PER_UNIT + 'GBE';
}

function words(...parts) {
  return parts.filter((part) => part !== null && part !== undefined && part !== '').join(' ');
}

function systemSegments(ticker) {
  const segments = [];
  if (known(ticker?.failed_units)) {
    segments.push({ text: 'SYSTEMD' + SEPARATOR + ticker.failed_units + ' FAILED UNITS', level: ticker.failed_units > 0 ? 'crit' : 'ok' });
  }
  if (known(ticker?.firewalld)) {
    segments.push(ticker.firewalld ? ok('FIREWALLD ACTIVE') : { text: 'FIREWALLD INACTIVE', level: 'warn' });
  }
  return segments;
}

function linkSegments(links) {
  if (!links) {
    return [];
  }
  const lans = (links.lan ?? [])
    .filter((lan) => lan.state === LINK_UP)
    .map((lan) => ok(words(lan.iface.toUpperCase(), lan.address, speedText(lan.speed_mbps))));
  const tailscale = links.tailscale?.state === LINK_UP && links.tailscale.address ? [ok(words(TAILSCALE_IFACE, links.tailscale.address))] : [];
  const vpns = (links.vpns ?? []).map((vpn) => ok(vpn.name.toUpperCase() + ' ' + (vpn.state === LINK_UP ? 'UP' : 'DOWN')));
  return [...lans, ...tailscale, ...vpns];
}

// A kernel name such as nvme1n1 is the drive's namespace; the controller is what dies.
function deadDriveSegments(disks) {
  return (disks?.drives ?? [])
    .filter((drive) => drive.state === DEAD)
    .map((drive) => ({ text: drive.kernel_name.replace(NVME_NAMESPACE, '$1').toUpperCase() + ' CONTROLLER DEAD', level: 'crit' }));
}

function alertSegments(alerts, nowS) {
  return activeAlerts(alerts?.alerts, nowS).map((alert) => ({ text: words(alert.title, alert.severity), level: alert.level }));
}

// Each part is left out while unknown, in the mockup's order.
export function tickerSegments({ ticker, host, links, disks, alerts, nowS }) {
  const segments = systemSegments(ticker);
  if (host?.kernel) {
    segments.push(ok('KERNEL ' + host.kernel.replace(KERNEL_ARCH, '').toUpperCase()));
  }
  if (ticker?.nvidia_driver) {
    segments.push(ok('NVIDIA ' + ticker.nvidia_driver));
  }
  segments.push(...linkSegments(links), ...deadDriveSegments(disks), ...alertSegments(alerts, nowS));
  if (known(ticker?.updates)) {
    segments.push(ok(ticker.updates + ' UPDATES PENDING'));
  }
  return segments;
}

export function tickerText(segments) {
  return segments.map((segment) => segment.text).join(SEPARATOR) + ' //';
}
