// The uplink widget's text and its home, apart from QML so that node can test them. The strings follow the reference
// mockup's drawGlobe(), with the footer's licence credit taken from the daemon.

// As the kit's formatters print a missing value.
export const MISSING = '--';
// The daemon's attribution is empty while it has no city database, and README says how to fetch one.
export const NO_DATABASE_TEXT = 'NO GEO-IP DATABASE · SEE README';

const SEPARATOR = ' · ';

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
