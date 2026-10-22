import test from 'node:test';
import assert from 'node:assert/strict';
import { COLUMN_WORDS, MAX_COLUMNS, MAX_ROWS, ROW_WORDS, columnWords, drawnRows, rowWords, skippedColumns } from '../kit/GhostKit/lib/blockgraph.mjs';
import { litCells, rampAlphaByte } from '../kit/GhostKit/lib/segments.mjs';

const GRAPH = Object.freeze({ lo: 0, hi: 100, warnAt: 0.75, critAt: 0.9, minLit: 1, live: true });
const LEVEL_OK = 0;
const LEVEL_WARN = 1;
const LEVEL_CRIT = 2;
const FLOAT_EXACT_LIMIT = 2 ** 24;

// blockgraph.frag's unpacking, in 32-bit float arithmetic as the shader does it.
const f32 = Math.fround;
function fieldIn(word, index, perComponent, range) {
  const slot = Math.floor(f32((index + 0.5) / perComponent));
  const shift = index - slot * perComponent;
  const divisor = shift === 0 ? 1 : shift === 1 ? range : range * range;
  const shifted = Math.floor(f32(f32(word[slot]) / divisor));
  return f32(shifted - range * Math.floor(f32(shifted / range)));
}
function column(words, at) {
  const word = Math.floor(f32((at + 0.5) / 8));
  const field = fieldIn(words[word], at - word * 8, 2, 4096);
  const lit = Math.floor(field / 4);
  return { lit, level: field - lit * 4 };
}
function rowAlphaByte(words, row) {
  const word = Math.floor(f32((row + 0.5) / 12));
  return fieldIn(words[word], row - word * 12, 3, 256);
}

// Every level, missing samples, an empty and a full column.
function series(length) {
  return Array.from({ length }, (_, i) => (i % 37 === 0 ? null : (i * 7919) % 101));
}
function expectedLevel(value) {
  const fraction = value / 100;
  if (!Number.isFinite(value) || fraction < 0.75) {
    return LEVEL_OK;
  }
  return fraction >= 0.9 ? LEVEL_CRIT : LEVEL_WARN;
}

test('the limits are 384 columns and 264 rows in 48 and 22 words', () => {
  assert.equal(MAX_COLUMNS, 384);
  assert.equal(MAX_ROWS, 264);
  assert.equal(columnWords([], 0, 0, GRAPH).length, COLUMN_WORDS);
  assert.equal(rowWords(0).length, ROW_WORDS);
  assert.equal(COLUMN_WORDS + ROW_WORDS, 70);
});

test('every component is an integer that a 32-bit float holds exactly', () => {
  const words = columnWords(Array(MAX_COLUMNS).fill(100), MAX_COLUMNS, MAX_ROWS, { ...GRAPH, warnAt: Infinity, critAt: 1 }).concat(rowWords(MAX_ROWS));
  for (const word of words) {
    assert.equal(word.length, 4);
    for (const value of word) {
      assert.ok(Number.isInteger(value) && value >= 0 && value < FLOAT_EXACT_LIMIT, `component ${value}`);
      assert.equal(f32(value), value);
    }
  }
});

test('384 columns at 264 rows unpack to each column\'s lit count and level', () => {
  const values = series(MAX_COLUMNS);
  const words = columnWords(values, MAX_COLUMNS, MAX_ROWS, GRAPH);
  const levels = new Set();
  for (let at = 0; at < MAX_COLUMNS; at += 1) {
    const value = values[at];
    const expectedLit = Number.isFinite(value) ? litCells(value, 0, 100, MAX_ROWS, 1) : 0;
    assert.deepEqual(column(words, at), { lit: expectedLit, level: expectedLevel(value) }, `column ${at}`);
    levels.add(column(words, at).level);
  }
  assert.deepEqual([...levels].sort(), [LEVEL_OK, LEVEL_WARN, LEVEL_CRIT]);
  assert.equal(column(words, values.indexOf(100)).lit, MAX_ROWS);
});

test('past 384 columns only the newest 384 are packed', () => {
  const columns = MAX_COLUMNS + 16;
  const values = series(columns);
  assert.equal(skippedColumns(columns), 16);
  assert.equal(skippedColumns(MAX_COLUMNS), 0);
  const words = columnWords(values, columns, 20, GRAPH);
  for (let at = 0; at < MAX_COLUMNS; at += 1) {
    const value = values[at + 16];
    assert.equal(column(words, at).lit, Number.isFinite(value) ? litCells(value, 0, 100, 20, 1) : 0, `column ${at}`);
  }
});

test('past 264 rows the graph draws its bottom 264 and scales its values to them', () => {
  const fitted = MAX_ROWS + 36;
  assert.equal(drawnRows(fitted), MAX_ROWS);
  assert.equal(drawnRows(20), 20);
  assert.equal(column(columnWords([100], 1, fitted, GRAPH), 0).lit, MAX_ROWS);
  assert.equal(column(columnWords([50], 1, fitted, GRAPH), 0).lit, MAX_ROWS / 2);
  assert.ok(column(columnWords([88], 1, fitted, GRAPH), 0).lit < MAX_ROWS);
});

test('the row words hold the ramp\'s alpha byte for each row drawn', () => {
  for (const rows of [1, 17, MAX_ROWS, MAX_ROWS + 36]) {
    const words = rowWords(rows);
    const drawn = drawnRows(rows);
    for (let row = 0; row < drawn; row += 1) {
      assert.equal(rowAlphaByte(words, row), rampAlphaByte(row, drawn), `row ${row} of ${rows}`);
    }
  }
});

test('a series longer than the graph shows its newest samples, the newest on the right', () => {
  const history = series(300);
  const words = columnWords(history, 69, 13, GRAPH);
  for (let at = 0; at < 69; at += 1) {
    const value = history[300 - 69 + at];
    assert.equal(column(words, at).lit, Number.isFinite(value) ? litCells(value, 0, 100, 13, 1) : 0, `column ${at}`);
  }
});

test('a series shorter than the graph fills it from the right', () => {
  const words = columnWords([50, 100], 4, 10, GRAPH);
  assert.deepEqual([0, 1, 2, 3].map(at => column(words, at).lit), [0, 0, 5, 10]);
});

test('a missing sample lights nothing, even below minLit, and a stale graph lights nothing at all', () => {
  assert.equal(column(columnWords([null], 1, 10, GRAPH), 0).lit, 0);
  assert.equal(column(columnWords([0], 1, 10, GRAPH), 0).lit, 1);
  assert.equal(column(columnWords([0], 1, 10, { ...GRAPH, minLit: 0 }), 0).lit, 0);
  assert.equal(column(columnWords([100], 1, 10, { ...GRAPH, live: false }), 0).lit, 0);
});

test('a minLit above the rows drawn lights only those rows and keeps the level intact', () => {
  assert.deepEqual(column(columnWords([0], 1, 3, { ...GRAPH, minLit: 5 }), 0), { lit: 3, level: LEVEL_OK });
  assert.deepEqual(column(columnWords([95], 1, 10, { ...GRAPH, minLit: 1024 }), 0), { lit: 10, level: LEVEL_CRIT });
  assert.equal(column(columnWords([0], 1, MAX_ROWS + 1, { ...GRAPH, minLit: 2 * MAX_ROWS }), 0).lit, MAX_ROWS);
});
