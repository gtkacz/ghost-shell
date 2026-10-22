// The network widget's text, apart from QML so that node can test it. Its strings follow the reference mockup's
// p-net panel and the lower half of its uplink scope.

// As the kit's formatters print a missing value.
export const MISSING = '--';
// The state word a link that is down shows, which the widget draws dim rather than in a colour.
export const DOWN_TEXT = 'DOWN';
// The mirror graph's scale floor, as the mockup's `nmax`: near-idle traffic stays low in the graph.
export const NET_FLOOR_BPS = 50_000;

const SEPARATOR = ' · ';
const MBPS_PER_GBPS = 1000;
const TENTHS_PER_UNIT = 10;
const TAILSCALE_NAME = 'TAILSCALE';
const LINK_UP = 'up';
const LINK_CONNECTING = 'connecting';
const STATE_TEXT = Object.freeze({ [LINK_UP]: 'UP', [LINK_CONNECTING]: 'CONNECTING' });
// NetworkManager's word for an up VPN is "activated", which the mockup prints as ACTIVE.
const VPN_UP_TEXT = 'ACTIVE';

function joined(parts) {
  return parts.filter((part) => part !== null && part !== undefined && part !== '').join(SEPARATOR);
}

function stateText(state, upText) {
  if (state === LINK_UP) {
    return upText;
  }
  return STATE_TEXT[state] ?? DOWN_TEXT;
}

// `1 GbE` from a gigabit up, to a tenth, and `<n> Mb/s` below; '' when the device reports no speed, which a link
// that is down does as 0 or -1.
function speedText(mbps) {
  if (!Number.isFinite(mbps) || mbps <= 0) {
    return '';
  }
  if (mbps < MBPS_PER_GBPS) {
    return mbps + ' Mb/s';
  }
  return Math.round((mbps / MBPS_PER_GBPS) * TENTHS_PER_UNIT) / TENTHS_PER_UNIT + ' GbE';
}

// Saved VPNs are often named after their kind ("OpenVPN Flxpoint"), which the row already shows as its name.
function withoutKind(name, kind) {
  const lowerName = name.toLowerCase();
  const lowerKind = kind.toLowerCase();
  if (lowerName === lowerKind) {
    return '';
  }
  if (lowerName.startsWith(lowerKind) && /\s/.test(name.charAt(kind.length))) {
    return name.slice(kind.length).trim();
  }
  return name;
}

function peersText(tailscale) {
  return tailscale.peers_online + '/' + tailscale.peers_total;
}

function lanOf(net, links) {
  return links ? links.lan.find((lan) => lan.iface === net.default_iface) : undefined;
}

export function metaLine(net, links) {
  if (!net || !net.default_iface) {
    return MISSING;
  }
  const lan = lanOf(net, links);
  return joined([net.default_iface.toUpperCase(), lan ? speedText(lan.speed_mbps) : '']);
}

// Whether the interface of the default route carries traffic.
export function defaultUp(net) {
  if (!net || !net.default_iface) {
    return false;
  }
  const iface = net.interfaces.find((candidate) => candidate.name === net.default_iface);
  return iface !== undefined && iface.oper_up;
}

export function linkRows(links) {
  if (!links) {
    return [];
  }
  const lans = links.lan.map((lan) => ({
    name: lan.iface.toUpperCase(),
    detail: joined([lan.address, speedText(lan.speed_mbps)]),
    state: stateText(lan.state, STATE_TEXT[LINK_UP]),
  }));
  const vpns = links.vpns.map((vpn) => ({
    name: vpn.kind.toUpperCase(),
    detail: joined([withoutKind(vpn.name, vpn.kind), vpn.iface]),
    state: stateText(vpn.state, VPN_UP_TEXT),
  }));
  const tailscale = links.tailscale
    ? [
        {
          name: TAILSCALE_NAME,
          detail: joined([links.tailscale.address, peersText(links.tailscale) + ' peers']),
          state: stateText(links.tailscale.state, STATE_TEXT[LINK_UP]),
        },
      ]
    : [];
  return [...lans, ...vpns, ...tailscale];
}

export function pingText(ms) {
  return Number.isFinite(ms) ? Math.round(ms) + ' ms' : MISSING;
}

export function vpnLine(links) {
  const vpn = links ? links.vpns.find((candidate) => candidate.state === LINK_UP) : undefined;
  if (!vpn) {
    return MISSING;
  }
  return joined([vpn.kind, withoutKind(vpn.name, vpn.kind)]).toUpperCase();
}

// A Tailscale that is not up reaches no peer, whatever its status still lists.
export function tsLine(links) {
  return links && links.tailscale && links.tailscale.state === LINK_UP ? peersText(links.tailscale) + ' PEERS' : MISSING;
}
