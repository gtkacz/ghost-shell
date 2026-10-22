import { alphaByte, withAlpha } from './theme.mjs';

const DEFAULT_CELL_PX = 4;
const DEFAULT_GAP_PX = 2;
const UNLIT_ALPHA_PCT = 6;
// Exported for SegBars, whose shader colours its lit cells the way cellColor() does.
export const LIT_ALPHA_FLOOR_PCT = 35;
export const LIT_ALPHA_SPAN_PCT = 50;
// The reference mockup's mirror() and heat().
export const MIRROR_UNLIT_ALPHA_PCT = 5;
export const MIRROR_UP_FLOOR_PCT = 45;
export const MIRROR_UP_SPAN_PCT = 40;
export const MIRROR_DOWN_FLOOR_PCT = 45;
export const MIRROR_DOWN_SPAN_PCT = 50;
export const HEAT_FLOOR_PCT = 7;
export const HEAT_SPAN_PCT = 80;

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

// Strictly greater: AQI is warn above 100 and crit above 150, so a reading on a threshold keeps the lower level
// (spec §5.2).
export function levelAbove(value, warnAt, critAt) {
  if (!Number.isFinite(value)) {
    return 'ok';
  }
  if (value > critAt) {
    return 'crit';
  }
  if (value > warnAt) {
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
  return withAlpha(theme.signal, rampAlphaPct(row, rows));
}

function rampAlphaPct(row, rows) {
  return LIT_ALPHA_FLOOR_PCT + (LIT_ALPHA_SPAN_PCT * row) / rows;
}

// The alpha byte of cellColor()'s ramp at a row of an ok column below its top cell, for BlockGraph's shader. From
// withAlpha()'s own rounding, so it is the colour's alpha exactly.
export function rampAlphaByte(row, rows) {
  return alphaByte(rampAlphaPct(row, rows));
}

function mirrorAlphaPct(upper, row, rows) {
  if (upper) {
    return MIRROR_UP_FLOOR_PCT + (MIRROR_UP_SPAN_PCT * row) / rows;
  }
  return MIRROR_DOWN_FLOOR_PCT + (MIRROR_DOWN_SPAN_PCT * row) / rows;
}

// A cell of a mirror graph: `upper` picks the half, and rows count out from the axis.
export function mirrorCellColor(upper, row, lit, rows, theme) {
  if (row >= lit) {
    return withAlpha(theme.signal, MIRROR_UNLIT_ALPHA_PCT);
  }
  return withAlpha(upper ? theme.hi : theme.signal, mirrorAlphaPct(upper, row, rows));
}

// The alpha byte of a lit mirror cell, for MirrorGraph's shader.
export function mirrorAlphaByte(upper, row, rows) {
  return alphaByte(mirrorAlphaPct(upper, row, rows));
}

function heatAlphaPct(fraction) {
  return HEAT_FLOOR_PCT + HEAT_SPAN_PCT * Math.min(1, Math.max(0, fraction));
}

// A cell of a heat graph, whose brightness rather than height shows the value.
export function heatCellColor(fraction, cellLevel, theme) {
  if (cellLevel === 'crit') {
    return theme.crit;
  }
  if (cellLevel === 'warn') {
    return theme.warn;
  }
  if (!Number.isFinite(fraction)) {
    return withAlpha(theme.signal, UNLIT_ALPHA_PCT);
  }
  return withAlpha(theme.signal, heatAlphaPct(fraction));
}

// The alpha byte of an ok heat cell, for HeatGraph's shader.
export function heatAlphaByte(fraction) {
  return alphaByte(heatAlphaPct(fraction));
}
