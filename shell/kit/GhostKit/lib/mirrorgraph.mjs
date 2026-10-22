import { COMPONENTS_PER_WORD, pack } from './blockgraph.mjs';
import { cellCount, mirrorAlphaByte } from './segments.mjs';

// MirrorGraph's data as mirrorgraph.frag unpacks it, in BlockGraph's word layout (lib/blockgraph.mjs). A column's
// 12-bit field is its upper lit count times HALF_FIELD_RANGE plus its lower lit count; the row fields are the upper
// ramp's alpha bytes for the first MAX_HALF_ROWS fields and the lower ramp's for the next MAX_HALF_ROWS.
const COLUMN_FIELD_RANGE = 4096;
const COLUMN_FIELDS_PER_COMPONENT = 2;
const HALF_FIELD_RANGE = 64;
const ROW_FIELD_RANGE = 256;
const ROW_FIELDS_PER_COMPONENT = 3;
// The mockup's nmax and imax: the largest sample sits below the top row, so a burst still reads as a burst.
export const MIRROR_HEADROOM = 1.1;
export const MAX_HALF_ROWS = HALF_FIELD_RANGE - 1;
export const COLUMN_WORDS = 48;
export const ROW_WORDS = 11;
export const MAX_COLUMNS = COLUMN_WORDS * COMPONENTS_PER_WORD * COLUMN_FIELDS_PER_COMPONENT;

function largestFinite(series, start) {
  let largest = start;
  for (const sample of series || []) {
    if (Number.isFinite(sample) && sample > largest) {
      largest = sample;
    }
  }
  return largest;
}

export function mirrorMax(up, down, floor) {
  return largestFinite(down, largestFinite(up, floor)) * MIRROR_HEADROOM;
}

// The square root keeps idle traffic visible next to bursts (the mockup's mirror()).
export function mirrorLit(value, max, rows) {
  const fraction = value / max;
  if (!Number.isFinite(value) || !Number.isFinite(fraction)) {
    return 0;
  }
  return Math.round(Math.sqrt(Math.min(1, Math.max(0, fraction))) * rows);
}

// The mockup's (half + g) can place the top row one gap above the item; this keeps every row inside it.
export function fittedHalfRows(height, cellHeight, gapSize) {
  return cellCount(Math.floor(height / 2) - gapSize, cellHeight, gapSize);
}

// A half taller than MAX_HALF_ROWS rows gets the MAX_HALF_ROWS nearest the axis, with the values scaled to them, so a
// column never stops short of the value it shows; the height beyond stays empty.
export function drawnHalfRows(rows) {
  return Math.min(rows, MAX_HALF_ROWS);
}

function sampleAt(series, columns, at) {
  const samples = series || [];
  const index = samples.length - columns + at;
  return index >= 0 ? samples[index] : undefined;
}

// The column words of a graph `columns` wide whose halves fit `rows`; `graph` holds the max and whether the source is
// live. Each series fills the graph from the right, and the columns before it read as missing on that side.
export function mirrorColumnWords(up, down, columns, rows, graph) {
  const drawn = drawnHalfRows(rows);
  const fields = [];
  for (let at = Math.max(0, columns - MAX_COLUMNS); at < columns; at += 1) {
    const upLit = graph.live ? mirrorLit(sampleAt(up, columns, at), graph.max, drawn) : 0;
    const downLit = graph.live ? mirrorLit(sampleAt(down, columns, at), graph.max, drawn) : 0;
    fields.push(upLit * HALF_FIELD_RANGE + downLit);
  }
  return pack(fields, COLUMN_WORDS, COLUMN_FIELDS_PER_COMPONENT, COLUMN_FIELD_RANGE);
}

export function mirrorRowWords(rows) {
  const drawn = drawnHalfRows(rows);
  const fields = Array(2 * MAX_HALF_ROWS).fill(0);
  for (let row = 0; row < drawn; row += 1) {
    fields[row] = mirrorAlphaByte(true, row, drawn);
    fields[MAX_HALF_ROWS + row] = mirrorAlphaByte(false, row, drawn);
  }
  return pack(fields, ROW_WORDS, ROW_FIELDS_PER_COMPONENT, ROW_FIELD_RANGE);
}
