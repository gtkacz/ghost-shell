import { withAlpha } from './theme.mjs';

const DEFAULT_CELL_PX = 4;
const DEFAULT_GAP_PX = 2;
const UNLIT_ALPHA_PCT = 6;
const LIT_ALPHA_FLOOR_PCT = 35;
const LIT_ALPHA_SPAN_PCT = 50;

export function cellCount(width, cell = DEFAULT_CELL_PX, gap = DEFAULT_GAP_PX) {
  return Math.max(0, Math.floor((width + gap) / (cell + gap)));
}

export function litCells(value, lo, hi, cells, min = 0) {
  const fraction = (value - lo) / (hi - lo);
  // A degenerate range or a missing value yields NaN or Infinity, which must render as nothing lit rather than propagate.
  const clamped = Number.isFinite(fraction) ? Math.min(1, Math.max(0, fraction)) : 0;
  return Math.max(min, Math.round(clamped * cells));
}

// Comparisons coerce null to 0, so an unavailable reading must be screened out or it would read as a low-battery alarm.
export function level(fraction, warnAt, critAt) {
  if (!Number.isFinite(fraction)) {
    return 'ok';
  }
  if (fraction >= critAt) {
    return 'crit';
  }
  if (fraction >= warnAt) {
    return 'warn';
  }
  return 'ok';
}

export function levelLow(value, warnAt, critAt) {
  if (!Number.isFinite(value)) {
    return 'ok';
  }
  if (value <= critAt) {
    return 'crit';
  }
  if (value <= warnAt) {
    return 'warn';
  }
  return 'ok';
}

// Rows count from the bottom; `lit` is how many rows from the bottom are on.
export function cellColor(cellLevel, row, lit, rows, theme) {
  if (row >= lit) {
    return withAlpha(theme.signal, UNLIT_ALPHA_PCT);
  }
  if (cellLevel === 'crit') {
    return theme.crit;
  }
  if (cellLevel === 'warn') {
    return theme.warn;
  }
  if (row === lit - 1) {
    return theme.hi;
  }
  return withAlpha(theme.signal, LIT_ALPHA_FLOOR_PCT + (LIT_ALPHA_SPAN_PCT * row) / rows);
}
