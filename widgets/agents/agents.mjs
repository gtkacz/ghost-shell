const MISSING = '--';
const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 3600;
const WEEK_S = 604800;
const FIVE_HOUR_S = 18000;
const CLOCK_WIDTH = 2;
const ZERO_PAD = '0';
const TAG_LENGTH = 2;
const NOW_TEXT = 'NOW';
const WEEKDAYS = Object.freeze(['SUN', 'MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT']);
const STATE_RANK = Object.freeze({ waiting: 0, busy: 1, idle: 2 });

export const PROVIDER_TAGS = Object.freeze({ claude: 'CC', codex: 'CX' });
export const IDLE_SHOWN = 5;
export const DAY_S = 86400;

function padded(number) {
  return String(number).padStart(CLOCK_WIDTH, ZERO_PAD);
}

export function providerTag(id) {
  return PROVIDER_TAGS[id] ?? id.slice(0, TAG_LENGTH).toUpperCase();
}

export function windowLabel(lengthS) {
  if (lengthS === FIVE_HOUR_S) {
    return '5H';
  }
  if (lengthS === WEEK_S) {
    return '7D';
  }
  return Math.round(lengthS / SECONDS_PER_HOUR) + 'H';
}

export function weeklyOf(provider) {
  return provider?.windows?.find(w => w.length_s === WEEK_S) ?? null;
}

export function sortSessions(sessions) {
  const rank = session => STATE_RANK[session.state] ?? Object.keys(STATE_RANK).length;
  return [...sessions].sort((a, b) => rank(a) - rank(b) || b.since_s - a.since_s);
}

export function waitingCount(sessions) {
  return sessions.filter(session => session.state === 'waiting').length;
}

export function stateLabel(session) {
  if (session.state === 'waiting') {
    return (session.waiting_for || 'waiting').toUpperCase();
  }
  return session.state.toUpperCase();
}

export function projectOf(cwd) {
  return cwd.split('/').pop();
}

export function resetText(resetsAtS, nowS) {
  if (resetsAtS === null || !Number.isFinite(resetsAtS)) {
    return MISSING;
  }
  const delta = resetsAtS - nowS;
  if (delta < SECONDS_PER_MINUTE) {
    return NOW_TEXT;
  }
  const minutes = Math.floor((delta % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
  if (delta < SECONDS_PER_HOUR) {
    return minutes + 'M';
  }
  if (delta < DAY_S) {
    return Math.floor(delta / SECONDS_PER_HOUR) + 'H ' + minutes + 'M';
  }
  const when = new Date(resetsAtS * 1000);
  return WEEKDAYS[when.getDay()] + ' ' + padded(when.getHours()) + ':' + padded(when.getMinutes());
}

export function collapseIdle(sorted, shown = IDLE_SHOWN) {
  let idleSeen = 0;
  const rows = sorted.filter(session => session.state !== 'idle' || ++idleSeen <= shown);
  return { rows, hiddenIdle: sorted.length - rows.length };
}
