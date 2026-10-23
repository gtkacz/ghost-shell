import { ARRIVAL_MS, ARRIVAL_OPACITIES, ARRIVAL_START_PERCENTS } from './attention.mjs';

export const CELLS = 28;

export const BLINK_STEP_MS = 250;
export const BLINK_STEPS = [1, 0.5, 0, 0.5];
// The blink runs a whole number of cycles past the arrival and then holds at full opacity, as a boxed alert does.
const BLINK_END_MS = 9600;

const PERCENT = 100;
const SCAN_STEPS = 30;
const SCAN_SWEEP_MS = 800;
const DEFAULT_STYLE = 'prompt';
const JAPANESE_MODES = ['kanji', 'katakana', 'mixed', 'off'];
const HEX_COLOR = /^#[0-9a-fA-F]{6}$/;
const NUMBER = /^\s*-?\d+(\.\d+)?\s*$/;

// Mirrors ghostd's defaults, which apply to every key the snapshot lacks or garbles.
const DEFAULTS = {
  gate: { host: '', display_model: '' },
  appearance: {
    font: 'Kode Mono',
    font_cjk: 'Noto Sans CJK JP',
    japanese: 'mixed',
    panel_fill: 55,
    alert_stripe_period_s: 1.6,
    alert_stripe_step_s: 1,
  },
  colors: {
    ground: '#0a0f11',
    panel: '#0a0f11',
    hair: '#2a3a3d',
    hair2: '#1a2527',
    text: '#b7d1cd',
    hi: '#e4f1ef',
    dim: '#587170',
    signal: '#a3d4cc',
    warn: '#e9a84c',
    crit: '#ff3b45',
    dot: '#a3d4cc',
    burn: '#ffffff',
  },
  dots: { spacing: 24, opacity: 16 },
  scanlines: { opacity: 15 },
};

function pick(section, key, valid, parse = (raw) => raw) {
  const raw = section?.[key];
  return valid(raw) ? parse(raw) : undefined;
}

function isText(raw) {
  return typeof raw === 'string' && raw !== '';
}

function isNumber(raw) {
  return typeof raw === 'string' && NUMBER.test(raw);
}

function fill(defaults, section) {
  const out = {};
  for (const key of Object.keys(defaults)) {
    const fallback = defaults[key];
    let value;
    if (typeof fallback === 'number') {
      value = pick(section, key, isNumber, Number);
    } else if (HEX_COLOR.test(fallback)) {
      value = pick(section, key, (raw) => HEX_COLOR.test(raw));
    } else {
      value = pick(section, key, isText);
    }
    out[key] = value ?? fallback;
  }
  return out;
}

export function resolveStyle() {
  return DEFAULT_STYLE;
}

export function parseSnapshot(sections) {
  const gate = sections.gate ?? {};
  const appearance = fill(DEFAULTS.appearance, sections.appearance);
  if (!JAPANESE_MODES.includes(appearance.japanese)) {
    appearance.japanese = DEFAULTS.appearance.japanese;
  }
  return {
    style: resolveStyle(gate.style),
    host: gate.host ?? DEFAULTS.gate.host,
    displayModel: gate.display_model ?? DEFAULTS.gate.display_model,
    appearance: {
      ...appearance,
      colors: fill(DEFAULTS.colors, sections.colors),
      dots: fill(DEFAULTS.dots, sections.dots),
      scanlines: fill(DEFAULTS.scanlines, sections.scanlines),
    },
  };
}

export function cells(length, count = CELLS) {
  return { lit: Math.min(length, count), cursor: Math.min(length, count - 1) };
}

function next(items, current) {
  if (items.length === 0) {
    return current;
  }
  return items[(items.indexOf(current) + 1) % items.length];
}

export function nextLayout(layouts, current) {
  return next(layouts, current);
}

export function nextUser(users, current) {
  return next(users, current);
}

// Agrees with AlertBox: the same arrival tables for the first ARRIVAL_MS, then the blink.
export function alertOpacity(msSinceShown) {
  if (msSinceShown >= BLINK_END_MS) {
    return 1;
  }
  if (msSinceShown >= ARRIVAL_MS) {
    const step = Math.floor((msSinceShown - ARRIVAL_MS) / BLINK_STEP_MS);
    return BLINK_STEPS[step % BLINK_STEPS.length];
  }
  const reachedPercent = (PERCENT * msSinceShown) / ARRIVAL_MS;
  let opacity = ARRIVAL_OPACITIES[0];
  ARRIVAL_START_PERCENTS.forEach((start, i) => {
    if (reachedPercent >= start) {
      opacity = ARRIVAL_OPACITIES[i];
    }
  });
  return opacity;
}

export function failureText(attempt, message) {
  return message === '' ? `ACCESS DENIED · ATTEMPT ${attempt}` : message.toUpperCase();
}

export function actionsFor(mode, allowed) {
  const wanted =
    mode === 'lock'
      ? [['switch-user', allowed.switchUser], ['sleep', allowed.sleep]]
      : [['sleep', allowed.sleep], ['restart', allowed.restart], ['power', allowed.shutdown]];
  return wanted.filter(([, permitted]) => permitted).map(([name]) => name);
}

export function scanStep(ms) {
  return Math.min(SCAN_STEPS, Math.max(0, Math.round((SCAN_STEPS * ms) / SCAN_SWEEP_MS)));
}
