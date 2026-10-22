import test from 'node:test';
import assert from 'node:assert/strict';
import {
  COLUMN_WORDS, MAX_COLUMNS, MAX_HALF_ROWS, ROW_WORDS, drawnHalfRows, fittedHalfRows, mirrorColumnWords, mirrorLit, mirrorMax, mirrorRowWords,
} from '../kit/GhostKit/lib/mirrorgraph.mjs';
import { mirrorAlphaByte } from '../kit/GhostKit/lib/segments.mjs';

const FLOAT_EXACT_LIMIT = 2 ** 24;
const LIVE = Object.freeze({ max: 100, live: true });

// mirrorgraph.frag's unpacking, in 32-bit float arithmetic as the shader does it.
const f32 = Math.fround;
function fieldIn(word, index, perComponent, range) {
  const slot = Math.floor(f32((index + 0.5) / perComponent));
  const shift = index - slot * perComponent;
  const divisor = shift === 0 ? 1 : shift === 1 ? range : range * range;
  const shifted = Math.floor(f32(f32(word[slot]) / divisor));
  return f32(shifted - range * Math.floor(f32(shifted / range)));
}
function unpackColumn(words, at) {
  const word = Math.floor(f32((at + 0.5) / 8));
  const field = fieldIn(words[word], at - word * 8, 2, 4096);
  const up = Math.floor(field / 64);
  return { up, down: field - up * 64 };
}
function rowByte(words, field) {
  const word = Math.floor(f32((field + 0.5) / 12));
  return fieldIn(words[word], field - word * 12, 3, 256);
}

test('max is the floor or the largest sample, with headroom', () => {
  assert.ok(Math.abs(mirrorMax([10, null], [40], 50) - 55) < 1e-9);
  assert.ok(Math.abs(mirrorMax([100], [0], 50) - 110) < 1e-9);
});

// A tenth of the max lights three rows of ten rather than one: the square root keeps idle traffic visible.
test('square-root scale', () => {
  assert.equal(mirrorLit(25, 100, 10), 5);
  assert.equal(mirrorLit(1, 100, 10), 1);
  assert.equal(mirrorLit(10, 100, 10), 3);
  assert.equal(mirrorLit(0, 100, 10), 0);
  assert.equal(mirrorLit(null, 100, 10), 0);
  assert.equal(mirrorLit(200, 100, 10), 10);
});

test('half rows fit above the axis gap', () => {
  assert.equal(fittedHalfRows(40, 2, 1), 6);
  assert.equal(drawnHalfRows(200), 63);
});

test('a column packs up and down lit counts', () => {
  const w = mirrorColumnWords([0, 100], [100, 25], 2, 10, LIVE);
  assert.deepEqual(unpackColumn(w, 0), { up: 0, down: 10 });
  assert.deepEqual(unpackColumn(w, 1), { up: 10, down: 5 });
});

test('a short series fills from the right; not live lights nothing', () => {
  const w = mirrorColumnWords([50], [50], 4, 10, LIVE);
  assert.deepEqual([0, 1, 2].map(c => unpackColumn(w, c)), Array(3).fill({ up: 0, down: 0 }));
  assert.equal(mirrorColumnWords([50], [50], 1, 10, { max: 100, live: false }).flat().every(v => v === 0), true);
});

test('a missing sample lights nothing on its own side only', () => {
  assert.deepEqual(unpackColumn(mirrorColumnWords([null], [100], 1, 10, LIVE), 0), { up: 0, down: 10 });
});

test('row words carry both ramps', () => {
  const w = mirrorRowWords(10);
  assert.equal(rowByte(w, 0), mirrorAlphaByte(true, 0, 10));
  assert.equal(rowByte(w, 63 + 9), mirrorAlphaByte(false, 9, 10));
  assert.equal(rowByte(w, 10), 0);
});

test('limits and exact components', () => {
  assert.equal(MAX_COLUMNS, 384);
  assert.equal(COLUMN_WORDS + ROW_WORDS, 59);
  const full = Array(MAX_COLUMNS).fill(100);
  const words = mirrorColumnWords(full, full, MAX_COLUMNS, MAX_HALF_ROWS, LIVE).concat(mirrorRowWords(MAX_HALF_ROWS));
  assert.equal(words.length, COLUMN_WORDS + ROW_WORDS);
  for (const word of words) {
    assert.equal(word.length, 4);
    for (const value of word) {
      assert.ok(Number.isInteger(value) && value >= 0 && value < FLOAT_EXACT_LIMIT, `component ${value}`);
      assert.equal(f32(value), value);
    }
  }
  assert.deepEqual(unpackColumn(words, MAX_COLUMNS - 1), { up: MAX_HALF_ROWS, down: MAX_HALF_ROWS });
  for (let row = 0; row < MAX_HALF_ROWS; row += 1) {
    assert.equal(rowByte(words.slice(COLUMN_WORDS), row), mirrorAlphaByte(true, row, MAX_HALF_ROWS), `upper row ${row}`);
    assert.equal(rowByte(words.slice(COLUMN_WORDS), MAX_HALF_ROWS + row), mirrorAlphaByte(false, row, MAX_HALF_ROWS), `lower row ${row}`);
  }
});

test('past 384 columns only the newest are packed, and past 63 half rows the values scale to 63', () => {
  const up = Array.from({ length: MAX_COLUMNS + 16 }, (_, i) => (i < 16 ? 100 : 0));
  const words = mirrorColumnWords(up, [], MAX_COLUMNS + 16, 10, LIVE);
  assert.equal(Array.from({ length: MAX_COLUMNS }, (_, at) => unpackColumn(words, at).up).every(lit => lit === 0), true);
  assert.equal(unpackColumn(mirrorColumnWords([100], [25], 1, 200, LIVE), 0).up, MAX_HALF_ROWS);
  assert.equal(unpackColumn(mirrorColumnWords([100], [25], 1, 200, LIVE), 0).down, mirrorLit(25, 100, MAX_HALF_ROWS));
});
