import { level, litCells, rampAlphaByte } from './segments.mjs';

// BlockGraph's data as blockgraph.frag unpacks it. ShaderEffect ignores array uniforms, so the data travels in named
// vec4 words, each component an integer below 2^24 so that a 32-bit float holds it exactly, its first field in the
// lowest bits. A column's field is its lit count times LEVELS plus its level code, in 12 bits; a row's is the
// ramp's alpha byte there.
const LEVEL_CODES = Object.freeze({ ok: 0, warn: 1, crit: 2 });
const LEVELS = 4;
const COMPONENTS_PER_WORD = 4;
const COLUMN_FIELD_RANGE = 4096;
const COLUMN_FIELDS_PER_COMPONENT = 2;
const ROW_FIELD_RANGE = 256;
const ROW_FIELDS_PER_COMPONENT = 3;
export const COLUMN_WORDS = 48;
export const ROW_WORDS = 22;
export const MAX_COLUMNS = COLUMN_WORDS * COMPONENTS_PER_WORD * COLUMN_FIELDS_PER_COMPONENT;
// The row words bound the rows rather than the column fields, which could count up to 1023 lit cells.
export const MAX_ROWS = ROW_WORDS * COMPONENTS_PER_WORD * ROW_FIELDS_PER_COMPONENT;

function pack(fields, wordCount, fieldsPerComponent, range) {
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

// The column words of a graph `columns` wide and `rows` high; `graph` holds BlockGraph's lo, hi, warnAt, critAt,
// minLit and live. A series shorter than the graph fills it from the right, and the columns before it read as missing.
export function columnWords(series, columns, rows, graph) {
  const samples = series || [];
  const fields = [];
  for (let at = skippedColumns(columns); at < columns; at += 1) {
    const index = samples.length - columns + at;
    const sample = index >= 0 ? samples[index] : undefined;
    // A missing sample lights nothing, even below minLit, so it reads as unavailable rather than idle.
    const lit = graph.live && Number.isFinite(sample) ? litCells(sample, graph.lo, graph.hi, rows, graph.minLit) : 0;
    const code = LEVEL_CODES[level((sample - graph.lo) / (graph.hi - graph.lo), graph.warnAt, graph.critAt)];
    fields.push(Math.min(lit, MAX_ROWS) * LEVELS + code);
  }
  return pack(fields, COLUMN_WORDS, COLUMN_FIELDS_PER_COMPONENT, COLUMN_FIELD_RANGE);
}

// Rows past MAX_ROWS are the top ones, and never light.
export function rowWords(rows) {
  const fields = [];
  for (let row = 0; row < Math.min(rows, MAX_ROWS); row += 1) {
    fields.push(rampAlphaByte(row, rows));
  }
  return pack(fields, ROW_WORDS, ROW_FIELDS_PER_COMPONENT, ROW_FIELD_RANGE);
}
