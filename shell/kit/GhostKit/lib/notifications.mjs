import { ago } from './format.mjs';

export const SERVER = 'ghost-shell';
export const BADGE_CAP = 99;

const COUNTDOWN = /^\d\d:\d\d:\d\d$/;
const OPENABLE_LINK = /^(https?|mailto):/i;
const TWO_DIGITS = 2;

export function bellState({ server, unread, unreadCrit, dnd }) {
  if (server !== SERVER) {
    return { glyph: 'bell', colour: 'dim', badge: '', badgeColour: 'dim', tip: 'Notifications · Plasma serving' };
  }
  const badge = unread > 0 ? String(Math.min(unread, BADGE_CAP)) : '';
  const tip = `Notifications · ${unread} unread`;
  if (dnd) {
    return { glyph: 'bell-off', colour: 'dim', badge, badgeColour: 'dim', tip };
  }
  if (unread === 0) {
    return { glyph: 'bell', colour: 'dim', badge, badgeColour: 'dim', tip };
  }
  const colour = unreadCrit > 0 ? 'crit' : 'signal';
  return { glyph: 'bell', colour, badge, badgeColour: colour, tip };
}

// Stateless over the topic: `shown` holds the ids whose toast has ended and `visible` the ids on screen now. A visible
// record keeps its slot, free slots go to the waiting ones oldest first with criticals ahead, and when a critical still
// waits with no slot free the oldest visible normal gives way, so a critical never waits while a normal shows.
export function toastQueue({ items, shown, visible = [], shellStartS, maxPopups, suppressed }) {
  const eligible = items.filter((item) => !item.held && item.arrived_s >= shellStartS && !shown.has(item.id)).reverse();
  const slots = Number.isInteger(maxPopups) && maxPopups > 0 ? maxPopups : 0;
  if (suppressed !== '' || slots === 0) {
    return { show: [], waiting: eligible.length };
  }
  const isCritical = (item) => item.urgency === 'critical';
  const onScreen = new Set(visible);
  const chosen = new Set(eligible.filter((item) => onScreen.has(item.id)).slice(0, slots));
  const waiting = eligible.filter((item) => !chosen.has(item) && !onScreen.has(item.id));
  const waitingFirst = [...waiting.filter(isCritical), ...waiting.filter((item) => !isCritical(item))];
  for (const item of waitingFirst) {
    if (chosen.size < slots) {
      chosen.add(item);
      continue;
    }
    const yielding = isCritical(item) ? eligible.find((candidate) => chosen.has(candidate) && !isCritical(candidate)) : undefined;
    if (yielding === undefined) {
      break;
    }
    chosen.delete(yielding);
    chosen.add(item);
  }
  return {
    show: eligible
      .filter((item) => chosen.has(item))
      .map((item) => item.id)
      .reverse(),
    waiting: eligible.length - chosen.size,
  };
}

export function filterRecords(items, query) {
  const needle = query.toLowerCase();
  if (needle === '') {
    return items;
  }
  return items.filter((item) => [item.app, item.summary, item.body_text].some((field) => field.toLowerCase().includes(needle)));
}

export function rowText(record, nowS) {
  return {
    app: record.app.toUpperCase(),
    summary: record.summary,
    body: record.body_text,
    age: ago(Math.max(0, nowS - record.arrived_s)),
  };
}

export function countsLine(items) {
  const count = (level) => items.filter((item) => item.level === level).length;
  const crit = count('crit');
  const warn = count('warn');
  return [`${items.length} TOTAL`, crit > 0 ? `${crit} CRIT` : '', warn > 0 ? `${warn} WARN` : ''].filter((part) => part !== '').join(' · ');
}

export function footerLeft(items, unread) {
  return `${items.length} SHOWN · ${unread} NEW`;
}

// Local time, as the rest of the shell shows clocks.
export function quietSince(items, startS) {
  const since = new Date((items.length > 0 ? items[0].arrived_s : startS) * 1000);
  const pad = (value) => String(value).padStart(TWO_DIGITS, '0');
  return `QUIET SINCE ${pad(since.getHours())}:${pad(since.getMinutes())}`;
}

// The app's own window by desktop entry, else by the sender's pid; the active one wins among several.
export function windowToRaise(windows, record) {
  const pick = (matches) => (matches.find((window) => window.active) ?? matches[0])?.id ?? null;
  const byApp = record.desktop_entry === '' ? [] : windows.filter((window) => window.app_id === record.desktop_entry);
  if (byApp.length > 0) {
    return pick(byApp);
  }
  return record.pid === 0 ? null : pick(windows.filter((window) => window.pid === record.pid));
}

// Like the launcher's, the first desktop value on each connection only sets the baseline.
export function acceptNotificationsSeq(seen, desktop, connection) {
  const isBaseline = seen === null || seen.connection !== connection;
  const toggle = !isBaseline && desktop.notifications_seq > seen.seq;
  return { seen: { connection, seq: desktop.notifications_seq }, toggle };
}

export function moveSelection(index, key, length) {
  const last = Math.max(0, length - 1);
  if (key === 'Down') {
    return Math.min(last, index + 1);
  }
  if (key === 'Up') {
    return Math.max(0, index - 1);
  }
  return index;
}

export function isCountdown(bodyText) {
  return COUNTDOWN.test(bodyText);
}

export function greetingSignals(topicValue) {
  return topicValue?.server === SERVER ? (topicValue.unread ?? 0) : 0;
}

export function isOpenableLink(url) {
  return typeof url === 'string' && OPENABLE_LINK.test(url);
}

// Each publish hands over new arrays, so a list a Repeater draws buttons from keeps its identity while its keys and
// labels do not change; a new identity rebuilds every button, and a press in flight is lost.
export function stableActions(previous, next) {
  const same = previous.length === next.length && previous.every((action, i) => action.key === next[i].key && action.label === next[i].label);
  return same ? previous : next;
}
