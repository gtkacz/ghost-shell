const TOPIC_NAME = /^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$/;
const MAX_TOPIC_LENGTH = 64;
// Kept even while covered: desktop says whether the dashboard is covered, and the daemon takes the
// client subscribed to perf as the shell whose CPU it reports. session keeps the greeting armed while covered.
const ALWAYS_SUBSCRIBED = Object.freeze(['desktop', 'perf', 'session']);

export function isValidTopic(name) {
  return typeof name === 'string' && name.length <= MAX_TOPIC_LENGTH && TOPIC_NAME.test(name);
}

function collectRequires(node, topics) {
  if (Array.isArray(node.children)) {
    node.children.forEach((child) => collectRequires(child, topics));
    return;
  }
  const slots = Array.isArray(node.tabs) ? node.tabs : [node];
  slots.forEach((slot) => slot.requires.forEach((topic) => topics.add(topic)));
}

// Every view, not just the shown one, so a graph's history keeps growing while another view is up.
export function requiresOf(layout) {
  const topics = new Set();
  Object.keys(layout.views).forEach((name) => collectRequires(layout.views[name], topics));
  return Array.from(topics).sort();
}

const BAR_AREAS = Object.freeze(['left', 'centre', 'right', 'dock']);

// The slots of the bar and the dock, which stay subscribed while the dashboard is covered.
export function barRequiresOf(layout) {
  const topics = new Set();
  BAR_AREAS.forEach((area) => (layout?.bar?.[area] ?? []).forEach((slot) => slot.requires.forEach((topic) => topics.add(topic))));
  return Array.from(topics).sort();
}

// Wants outlive a cover because the dashboard's widgets, whose wants they were, are destroyed by it; what remains is
// the bar's.
export function subscriptionFor({ requires, wanted, covered, barRequires }) {
  const dashboard = covered ? [] : requires;
  return Array.from(new Set(dashboard.concat(wanted, barRequires, ALWAYS_SUBSCRIBED))).sort();
}
