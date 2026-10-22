// The telemetry lane's text, apart from QML so that node can test it. Its strings follow the reference mockup's
// step(). A widget's helper reaches the kit only through the GhostKit import, so the kit's formatters (Fmt in QML,
// lib/format.mjs in node) are handed to laneText rather than imported.

const CPU = 'cpu';
const GPU = 'gpu';
const MEM = 'mem';
const NET = 'net';
const DISK = 'disk';
const THERM = 'therm';

export const LANES = Object.freeze({
  [CPU]: Object.freeze({ name: 'CPU', tag: 'cpu' }),
  [GPU]: Object.freeze({ name: 'GPU', tag: 'gpu' }),
  [MEM]: Object.freeze({ name: 'MEMORY', tag: 'mem' }),
  [NET]: Object.freeze({ name: 'NETWORK', tag: 'net' }),
  [DISK]: Object.freeze({ name: 'DISK I/O', tag: 'disk' }),
  [THERM]: Object.freeze({ name: 'THERMALS', tag: 'sens' }),
});

// The mirror graphs' scale floors, as the mockup's `nmax` and `imax`: near-idle traffic stays low in the lane.
export const NET_FLOOR_BPS = 50_000;
export const DISK_FLOOR_BPS = 20_000_000;
// The thermal lanes' scale in °C, as the mockup draws them.
export const THERM_LO_C = 20;
export const THERM_HI_C = 100;

// The least a line of the label block shrinks to fit: at 0.8 the sub is still readable and the value still the
// block's dominant line, and below about 0.75 the sub would fall under the tag's size and stop reading as one block
// (user-confirmed 2026-10-22).
export const MIN_FIT_SCALE = 0.8;

// As the kit's formatters print a missing value.
export const MISSING = '--';
const SEPARATOR = ' · ';
const BYTES_PER_MB = 1e6;
const MS_PER_SECOND = 1000;
const LINK_UP = 'up';
const CHEVRON_DOWN = 'chevron-down';
const CHEVRON_UP = 'chevron-up';
// The mockup draws the network sub as `⌄ DOWN · ⌃ UP · VPN 2 UP`; laneSub gives the last part.
const NET_SUB_PREFIX = ['DOWN', 'UP'].join(SEPARATOR);
const CHEVRONED = new Set([NET, DISK]);
const CHEVRONS = Object.freeze([CHEVRON_DOWN, CHEVRON_UP]);

function isCount(value) {
  return Number.isFinite(value) && value >= 0;
}

function countText(value) {
  return isCount(value) ? String(value) : MISSING;
}

function wholeMb(bps) {
  return Number.isFinite(bps) ? String(Math.round(bps / BYTES_PER_MB)) : MISSING;
}

// The VPNs NetworkManager reports up, and Tailscale when it is up; null before the links arrive.
export function vpnUpCount(links) {
  if (links === null || links === undefined) {
    return null;
  }
  const vpns = links.vpns.filter((vpn) => vpn.state === LINK_UP).length;
  const tailscale = links.tailscale !== null && links.tailscale.state === LINK_UP ? 1 : 0;
  return vpns + tailscale;
}

export function spanSeconds(columns, stepMs) {
  return Math.round((columns * stepMs) / MS_PER_SECOND);
}

// The sub line as words, each with the Glyph name drawn before it, or '' for none. The mirror lanes put a down
// chevron before their first word and an up chevron before their second, as the mockup does.
export function subParts(metric, sub) {
  const text = metric === NET ? NET_SUB_PREFIX + SEPARATOR + sub : sub;
  return text.split(SEPARATOR).map((word, index) => ({
    glyph: CHEVRONED.has(metric) && index < CHEVRONS.length ? CHEVRONS[index] : '',
    text: word,
  }));
}

// fmt: the kit's pct, tempShort, gib and rateShort.
export function laneText(fmt) {
  // `d` holds what the metric reads: cpu { cpu, tctl }, gpu { gpu }, mem { mem }, net { net }, disk { read, write }
  // (bytes/s), therm { tctl, gpuT }. Net returns its two rates, which the widget draws with chevrons.
  function laneValue(metric, d) {
    switch (metric) {
      case CPU:
        return d.cpu ? fmt.pct(d.cpu.load) + SEPARATOR + fmt.tempShort(d.tctl) : MISSING;
      case GPU:
        return d.gpu && d.gpu.present ? fmt.pct(d.gpu.load) + SEPARATOR + fmt.tempShort(d.gpu.temp_c) : MISSING;
      case MEM:
        return d.mem ? fmt.gib(d.mem.used_kib, 1) + ' G' : MISSING;
      case NET:
        return { down: fmt.rateShort(d.net ? d.net.rx_bps : null), up: fmt.rateShort(d.net ? d.net.tx_bps : null) };
      case DISK:
        return 'R' + wholeMb(d.read) + ' W' + wholeMb(d.write);
      case THERM:
        return fmt.tempShort(d.tctl) + ' / ' + fmt.tempShort(d.gpuT);
      default:
        return MISSING;
    }
  }

  // `d` holds cpu { cores }, mem { mem }, net { links }.
  function laneSub(metric, d, spanS) {
    switch (metric) {
      case CPU:
        return ['HEAT', countText(d.cores) + ' CORES', '~' + countText(spanS) + ' S'].join(SEPARATOR);
      case GPU:
        return 'LOAD';
      case MEM:
        return '/ ' + fmt.gib(d.mem ? d.mem.total_kib : null, 1) + ' GiB';
      case NET:
        return 'VPN ' + countText(vpnUpCount(d.links)) + ' UP';
      case DISK:
        return ['READ', 'WRITE'].join(SEPARATOR);
      case THERM:
        return ['CPU', 'GPU', 'NVME'].join(SEPARATOR);
      default:
        return '';
    }
  }

  return { laneValue, laneSub };
}

// The scale a line lineWidth wide draws at to end by fitWidth, where the graph begins, and never below MIN_FIT_SCALE.
export function fitScale(lineWidth, fitWidth) {
  return lineWidth > fitWidth ? Math.max(MIN_FIT_SCALE, fitWidth / lineWidth) : 1;
}

// How many of a line's parts, by width and in order, show: a line too wide even at MIN_FIT_SCALE drops its trailing
// parts whole, since a glyph is never cut and the line never reaches into the graph.
export function fittingParts(widths, fitWidth) {
  const room = fitWidth / MIN_FIT_SCALE;
  let used = 0;
  let shown = 0;
  for (const width of widths) {
    if (used + width > room) {
      break;
    }
    used += width;
    shown += 1;
  }
  return shown;
}
