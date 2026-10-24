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

// Stateless over the topic: `shown` holds the ids whose toast has ended, so what is still on screen is simply what
// this returns again. Criticals take the slots first, so one never waits while a normal shows; both go oldest first.
export function toastQueue({ items, shown, shellStartS, maxPopups, suppressed }) {
  const eligible = items.filter((item) => !item.held && item.arrived_s >= shellStartS && !shown.has(item.id)).reverse();
  if (suppressed !== '') {
    return { show: [], waiting: eligible.length };
  }
  const isCritical = (item) => item.urgency === 'critical';
  const ranked = [...eligible.filter(isCritical), ...eligible.filter((item) => !isCritical(item))];
  const visible = new Set(ranked.slice(0, maxPopups));
  return {
    show: eligible
      .filter((item) => visible.has(item))
      .map((item) => item.id)
      .reverse(),
    waiting: eligible.length - visible.size,
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
  return topicValue?.server === SERVER ? topicValue.unread : 0;
}

export function isOpenableLink(url) {
  return typeof url === 'string' && OPENABLE_LINK.test(url);
}
