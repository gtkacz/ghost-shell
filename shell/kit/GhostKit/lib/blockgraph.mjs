import { level, litCells, rampAlphaByte } from './segments.mjs';

// BlockGraph's data as blockgraph.frag unpacks it. ShaderEffect ignores array uniforms, so the data travels in named
// vec4 words, each component an integer below 2^24 so that a 32-bit float holds it exactly, its first field in the
// lowest bits. A column's field is its lit count times LEVELS plus its level code, in 12 bits; a row's is the
// ramp's alpha byte there.
const LEVEL_CODES = Object.freeze({ ok: 0, warn: 1, crit: 2 });
// Four slots for three codes: a power of two keeps the shader's division by it exact.
const LEVELS = 4;
export const COMPONENTS_PER_WORD = 4;
const COLUMN_FIELD_RANGE = 4096;
const COLUMN_FIELDS_PER_COMPONENT = 2;
const ROW_FIELD_RANGE = 256;
const ROW_FIELDS_PER_COMPONENT = 3;
export const COLUMN_WORDS = 48;
export const ROW_WORDS = 22;
export const MAX_COLUMNS = COLUMN_WORDS * COMPONENTS_PER_WORD * COLUMN_FIELDS_PER_COMPONENT;
// The row words bound the rows rather than the column fields, which could count up to 1023 lit cells.
export const MAX_ROWS = ROW_WORDS * COMPONENTS_PER_WORD * ROW_FIELDS_PER_COMPONENT;

export function pack(fields, wordCount, fieldsPerComponent, range) {
  const words = [];
  for (let word = 0; word < wordCount; word += 1) {
    const components = [];
    for (let slot = 0; slot < COMPONENTS_PER_WORD; slot += 1) {
      const first = (word * COMPONENTS_PER_WORD + slot) * fieldsPerComponent;
      let value = 0;
      for (let shift = fieldsPerComponent - 1; shift >= 0; shift -= 1) {
        value = value * range + (fields[first + shift] ?? 0);
      }
      components.push(value);
    }
    words.push(components);
  }
  return words;
}

// Columns past MAX_COLUMNS are the oldest, on the left, and draw unlit.
export function skippedColumns(columns) {
  return Math.max(0, columns - MAX_COLUMNS);
}

// A slot taller than MAX_ROWS rows gets its bottom MAX_ROWS, with the values scaled to them, so a column never stops
// short of the value it shows; the height above stays empty.
export function drawnRows(rows) {
  return Math.min(rows, MAX_ROWS);
}

// The column words of a graph `columns` wide whose slot fits `rows`; `graph` holds BlockGraph's lo, hi, warnAt,
// critAt, minLit and live. A series shorter than the graph fills it from the right, and the columns before it read as
// missing.
export function columnWords(series, columns, rows, graph) {
  const samples = series || [];
  const drawn = drawnRows(rows);
  const fields = [];
  for (let at = skippedColumns(columns); at < columns; at += 1) {
    const index = samples.length - columns + at;
    const sample = index >= 0 ? samples[index] : undefined;
    // A missing sample lights nothing, even below minLit, so it reads as unavailable rather than idle. A minLit past
    // the rows drawn would ask the shader for rows that do not exist and, from 1024, overflow the column's field.
    const lit = graph.live && Number.isFinite(sample) ? Math.min(drawn, litCells(sample, graph.lo, graph.hi, drawn, graph.minLit)) : 0;
    const code = LEVEL_CODES[level((sample - graph.lo) / (graph.hi - graph.lo), graph.warnAt, graph.critAt)];
    fields.push(lit * LEVELS + code);
  }
  return pack(fields, COLUMN_WORDS, COLUMN_FIELDS_PER_COMPONENT, COLUMN_FIELD_RANGE);
}

export function rowWords(rows) {
  const drawn = drawnRows(rows);
  const fields = [];
  for (let row = 0; row < drawn; row += 1) {
    fields.push(rampAlphaByte(row, drawn));
  }
  return pack(fields, ROW_WORDS, ROW_FIELDS_PER_COMPONENT, ROW_FIELD_RANGE);
}
