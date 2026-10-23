export const PIN_COLUMNS = 6;
export const ARM_WINDOW_MS = 3000;
// Shorter than a key repeat or a double-click takes to come round again, so neither confirms an armed action.
export const CONFIRM_DELAY_MS = 250;

const FLATPAK_EXPORTS = '/flatpak/exports/';
const ORIGIN_USER = 0;
const ORIGIN_SYSTEM = 1;
const ORIGIN_FLATPAK = 2;
const NAME_PREFIX = 0;
const WORD_PREFIX = 1;
const EXTRA_PREFIX = 2;
const SUBSTRING = 3;
const WORD_BREAK = /[\s-]+/;
const BLANKS = /\s+/;
const DEFAULT_CHIP = 'UTILITY';

// The first match wins, so a game that is also a utility reads GAME.
const CHIPS = [
  [['Game'], 'GAME'],
  [['Development'], 'DEV'],
  [['Network'], 'NETWORK'],
  [['AudioVideo', 'Audio', 'Video'], 'MEDIA'],
  [['Graphics'], 'GRAPHICS'],
  [['Office'], 'OFFICE'],
  [['Settings'], 'SETTINGS'],
  [['System'], 'SYSTEM'],
];

export const initialPower = Object.freeze({ armed: null, armedAtMs: 0 });

export function category(categories) {
  const found = CHIPS.find(([names]) => names.some((name) => categories.includes(name)));
  return found ? found[1] : DEFAULT_CHIP;
}

const byName = (a, b) => a.name.localeCompare(b.name);

// DesktopEntry exposes no path, so the caller derives one from where the entry came from.
function origin(entry, userDir) {
  if (entry.path.includes(FLATPAK_EXPORTS)) {
    return ORIGIN_FLATPAK;
  }
  return userDir !== '' && entry.path.startsWith(userDir) ? ORIGIN_USER : ORIGIN_SYSTEM;
}

export function dedupe(entries, userDir = '') {
  const kept = new Map();
  for (const entry of entries) {
    if (entry.noDisplay) {
      continue;
    }
    const key = entry.name.toLowerCase() + '\n' + entry.command;
    const held = kept.get(key);
    if (held === undefined || origin(entry, userDir) < origin(held, userDir)) {
      kept.set(key, entry);
    }
  }
  return [...kept.values()];
}

function score(entry, query) {
  const name = entry.name.toLowerCase();
  if (name.startsWith(query)) {
    return NAME_PREFIX;
  }
  if (name.split(WORD_BREAK).some((word) => word.startsWith(query))) {
    return WORD_PREFIX;
  }
  const extra = (entry.genericName + ' ' + entry.keywords.join(' ')).toLowerCase().split(BLANKS);
  if (extra.some((word) => word.startsWith(query))) {
    return EXTRA_PREFIX;
  }
  return name.includes(query) ? SUBSTRING : -1;
}

export function rank(entries, query) {
  const lowered = query.toLowerCase();
  return entries
    .map((entry) => ({ entry, score: score(entry, lowered) }))
    .filter((scored) => scored.score >= 0)
    .sort((a, b) => a.score - b.score || byName(a.entry, b.entry))
    .map((scored) => scored.entry);
}

// The matched part of a name split as [before, match, after], for drawing the match in a brighter colour.
export function highlight(name, query) {
  const at = query === '' ? -1 : name.toLowerCase().indexOf(query.toLowerCase());
  if (at < 0) {
    return [name, '', ''];
  }
  return [name.slice(0, at), name.slice(at, at + query.length), name.slice(at + query.length)];
}

export function sections({ query, pinned, recent, all }) {
  if (query !== '') {
    return rank(all, query).map((entry) => ({ kind: 'app', entry }));
  }
  return [
    ...pinned.map((entry) => ({ kind: 'pin', entry })),
    ...recent.map((file) => ({ kind: 'file', file })),
    ...[...all].sort(byName).map((entry) => ({ kind: 'app', entry })),
  ];
}

// The index the key lands on. The pins are the first pinCount items, in a grid of PIN_COLUMNS; everything after is a list.
export function move({ index, pinCount, total }, key) {
  const last = Math.max(0, total - 1);
  if (index >= pinCount) {
    if (key === 'Down') {
      return Math.min(last, index + 1);
    }
    if (key === 'Up') {
      return index === pinCount && pinCount > 0 ? pinCount - 1 : Math.max(0, index - 1);
    }
    return index;
  }
  switch (key) {
    case 'Down':
      if (index + PIN_COLUMNS < pinCount) {
        return index + PIN_COLUMNS;
      }
      return pinCount < total ? pinCount : index;
    case 'Up':
      return index - PIN_COLUMNS >= 0 ? index - PIN_COLUMNS : index;
    case 'Right':
      return Math.min(index + 1, pinCount - 1);
    case 'Left':
      return Math.max(index - 1, 0);
    default:
      return index;
  }
}

export function power(state, action, nowMs) {
  if (action === 'lock') {
    return { state: initialPower, fire: action };
  }
  if (state.armed === action) {
    const sinceMs = nowMs - state.armedAtMs;
    if (sinceMs < CONFIRM_DELAY_MS) {
      return { state, fire: null };
    }
    if (sinceMs <= ARM_WINDOW_MS) {
      return { state: initialPower, fire: action };
    }
  }
  return { state: { armed: action, armedAtMs: nowMs }, fire: null };
}

export function disarm() {
  return initialPower;
}

// Like the view actions, the first desktop value on each connection only sets the baseline: it may carry a
// toggle from before this shell connected, and every ghostd counts from 0.
export function acceptLauncherSeq(seen, desktop, connection) {
  const isBaseline = seen === null || seen.connection !== connection;
  const toggle = !isBaseline && desktop.launcher_seq > seen.seq;
  return { seen: { connection, seq: desktop.launcher_seq }, toggle };
}

// The list's rows top to bottom: a heading for each group, one row for the whole pin grid, and a row for every file
// and app, each of which names the item it stands for.
export function rows(items, query) {
  const out = [];
  const start = (kind) => items.findIndex((item) => item.kind === kind);
  const count = (kind) => items.filter((item) => item.kind === kind).length;
  const group = (kind, label, tag) => {
    if (count(kind) === 0) {
      return;
    }
    out.push({ type: 'head', label, tag, count: tag === 'results' ? count(kind) : null });
    if (kind === 'pin') {
      out.push({ type: 'pins', from: start(kind), count: count(kind) });
      return;
    }
    const from = start(kind);
    for (let at = from; at < from + count(kind); at += 1) {
      out.push({ type: kind, at });
    }
  };
  if (query === '') {
    group('pin', 'PINNED', 'pin');
    group('file', 'RECENT', 'recent');
    group('app', 'ALL APPS', 'all');
  } else {
    group('app', 'RESULTS', 'results');
  }
  return out;
}

// The row that shows an item, so the list can keep the selected one in view.
export function rowOf(rowList, itemIndex) {
  return rowList.findIndex((row) => (row.type === 'pins' ? itemIndex >= row.from && itemIndex < row.from + row.count : row.at === itemIndex));
}
