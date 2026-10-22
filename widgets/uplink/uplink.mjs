// The uplink widget's text and its home, apart from QML so that node can test them. The strings follow the reference
// mockup's drawGlobe(), with the footer's licence credit taken from the daemon.

// As the kit's formatters print a missing value.
export const MISSING = '--';
// The daemon's attribution is empty while it has no city database, and README says how to fetch one.
export const NO_DATABASE_TEXT = 'NO GEO-IP DATABASE · SEE README';

// The kit Globe's count of endpoint uniforms, restated because a widget cannot import the kit's modules; a node
// test pins the two together.
export const MAX_ENDPOINTS = 8;

const SEPARATOR = ' · ';

// An address the databases cannot name is its own label, and an IPv6 one would otherwise print in lower case.
export function shownEndpoints(snapshot) {
  const endpoints = snapshot && snapshot.endpoints ? snapshot.endpoints : [];
  return endpoints.slice(0, MAX_ENDPOINTS).map((endpoint) => Object.assign({}, endpoint, { label: String(endpoint.label).toUpperCase() }));
}

export function footerText(snapshot) {
  if (!snapshot) {
    return MISSING;
  }
  if (!snapshot.attribution) {
    return NO_DATABASE_TEXT;
  }
  return snapshot.total + ' ACTIVE ENDPOINTS' + SEPARATOR + snapshot.attribution;
}

export function homeLabel(host) {
  return host && host.hostname ? host.hostname.toUpperCase() : MISSING;
}

// The arcs start at home, so both coordinates must be set; a city alone is only the weather's.
export function hasHome(config) {
  const location = config ? config.location : null;
  return Boolean(location) && Number.isFinite(location.lat) && Number.isFinite(location.lon);
}

// A label sits `gap` right of its square, or as far left of it when it would run past the body's right edge, since
// the spec draws no text cut through a glyph.
export function labelX(anchorX, gap, textWidth, areaWidth) {
  const right = anchorX + gap;
  return right + textWidth > areaWidth ? anchorX - gap - textWidth : right;
}
