import test from 'node:test';
import assert from 'node:assert/strict';
import {
  CELL_CRIT, CELL_MISSING, CELL_WARN, HEAT_BAND_ROWS, HEAT_MAX_COLUMNS, HEAT_WORDS, heatCellByte, heatDrawnColumns, heatRing, heatRingAdvance, heatRowHeight,
} from '../kit/GhostKit/lib/heatgraph.mjs';
import { heatAlphaByte } from '../kit/GhostKit/lib/segments.mjs';

const G = Object.freeze({ lo: 0, hi: 100, warnAt: 0.75, critAt: 0.9, live: true });
const FLOAT_EXACT_LIMIT = 2 ** 24;

// heatgraph.frag's lookup of the cell at a column (from the left) and a row (from the top), in 32-bit float
// arithmetic as the shader does it.
const f32 = Math.fround;
function cellAt(ring, column, row) {
  const skipped = ring.columns - ring.drawn;
  if (column < skipped) {
    return CELL_MISSING;
  }
  const ringAt = ring.head + column - skipped;
  const slot = ringAt - ring.drawn * Math.floor(f32((ringAt + 0.5) / ring.drawn));
  const index = slot * ring.rows + row;
  const word = Math.floor(f32((index + 0.5) / 12));
  const inWord = index - word * 12;
  const component = Math.floor(f32((inWord + 0.5) / 3));
  const shift = inWord - component * 3;
  const divisor = shift === 0 ? 1 : shift === 1 ? 256 : 65536;
  const shifted = Math.floor(f32(f32(ring.words[word][component]) / divisor));
  return f32(shifted - 256 * Math.floor(f32(shifted / 256)));
}
function picture(ring) {
  return Array.from({ length: ring.rows }, (_, row) => Array.from({ length: ring.columns }, (__, column) => cellAt(ring, column, row)));
}

// Every level, gaps and empty and full cells, moving on by one sample per tick as a history does.
const SERIES_PERIOD = 37 * 101;
function history(tick, rows, length) {
  return Array.from({ length: rows }, (_, row) => Array.from({ length }, (__, i) => {
    const at = (i + tick + row * 19) % SERIES_PERIOD;
    return at % 37 === 0 ? null : (at * 7919) % 101;
  }));
}

test('cell bytes', () => {
  assert.equal(heatCellByte(50, G), heatAlphaByte(0.5));
  assert.equal(heatCellByte(80, G), CELL_WARN);
  assert.equal(heatCellByte(95, G), CELL_CRIT);
  assert.equal(heatCellByte(null, G), CELL_MISSING);
  assert.equal(heatCellByte(50, { ...G, live: false }), CELL_MISSING);
});

test('ok bytes never collide with the codes', () => {
  for (let f = 0; f < 0.75; f += 0.001) {
    const b = heatAlphaByte(f);
    assert.ok(b > CELL_MISSING && b < CELL_WARN, String(f));
  }
});

test('a row fills from the right and skips past 384', () => {
  assert.equal(heatDrawnColumns(251), 251);
  assert.equal(heatDrawnColumns(500), HEAT_MAX_COLUMNS);
  const ring = heatRing([[10, 20]], 3, G);
  assert.deepEqual([0, 1, 2].map(c => cellAt(ring, c, 0)), [CELL_MISSING, heatAlphaByte(0.1), heatAlphaByte(0.2)]);
  const wide = heatRing([Array(400).fill(50)], 400, G);
  assert.equal(cellAt(wide, 15, 0), CELL_MISSING);
  assert.equal(cellAt(wide, 16, 0), heatAlphaByte(0.5));
});

test('a band of 4 rows of 384 crit cells fills its 128 words with exact components', () => {
  assert.equal(HEAT_BAND_ROWS, 4);
  const ring = heatRing(Array.from({ length: HEAT_BAND_ROWS }, () => Array(HEAT_MAX_COLUMNS).fill(100)), HEAT_MAX_COLUMNS, G);
  assert.equal(ring.words.length, HEAT_WORDS);
  for (const word of ring.words) {
    for (const value of word) {
      assert.ok(Number.isInteger(value) && value >= 0 && value < FLOAT_EXACT_LIMIT, `component ${value}`);
      assert.equal(f32(value), value);
    }
  }
  assert.equal(ring.words.flat().every(value => value === FLOAT_EXACT_LIMIT - 1), true);
});

test('moving on one sample at a time re-sends one word and draws what a full repack draws', () => {
  for (const [rows, columns, length] of [[4, 251, 300], [3, 7, 7], [2, 400, 420], [1, 251, 300]]) {
    const ring = heatRing(history(0, rows, length), columns, G);
    for (let tick = 1; tick <= 2 * columns + 5; tick += 1) {
      const series = history(tick, rows, length);
      const changed = heatRingAdvance(ring, series, columns, G);
      assert.equal(changed?.length, 1, `${rows}x${columns} tick ${tick}: ${changed}`);
      assert.deepEqual(picture(ring), picture(heatRing(series, columns, G)), `${rows}x${columns} tick ${tick}`);
    }
  }
});

test('a history that grows from empty past the width moves the ring on one sample at a time', () => {
  const full = history(0, 4, 300);
  const upTo = length => full.map(samples => samples.slice(0, length));
  const ring = heatRing(upTo(0), 251, G);
  for (let length = 1; length <= 300; length += 1) {
    const series = upTo(length).map(samples => samples.slice(-300));
    assert.notEqual(heatRingAdvance(ring, series, 251, G), null, `length ${length}`);
    assert.deepEqual(picture(ring), picture(heatRing(series, 251, G)), `length ${length}`);
  }
});

test('a gap, a replaced series, a resize or new settings need a full repack', () => {
  const series = history(0, 4, 300);
  const withGap = series.map(samples => samples.slice(3).concat([null, null, 50]));
  assert.equal(heatRingAdvance(heatRing(series, 251, G), withGap, 251, G), null);
  // A sample changed in the oldest drawn column, then in the column left of the newest.
  for (const changedAt of [300 - 251, 300 - 2]) {
    const moved = history(1, 4, 300);
    moved[0] = moved[0].map((sample, at) => (at === changedAt ? 1 + (sample ?? 0) : sample));
    assert.equal(heatRingAdvance(heatRing(series, 251, G), moved, 251, G), null, `changed at ${changedAt}`);
  }
  assert.equal(heatRingAdvance(heatRing(series, 251, G), history(100, 4, 300), 251, G), null);
  assert.equal(heatRingAdvance(heatRing(series, 251, G), history(1, 4, 300), 250, G), null);
  assert.equal(heatRingAdvance(heatRing(series, 251, G), history(1, 3, 300), 251, G), null);
  assert.equal(heatRingAdvance(heatRing(series, 251, G), history(1, 4, 300), 251, { ...G, live: false }), null);
  assert.equal(heatRingAdvance(heatRing(series, 251, G), history(1, 4, 300), 251, { ...G, warnAt: 0.8 }), null);
});

test('drawing as HeatBand does matches a full repack after ticks, a gap and a resize', () => {
  // HeatBand.draw(): move the ring on, or pack a new one when it cannot.
  let ring = heatRing(history(0, 4, 300), 251, G);
  const draw = (series, columns) => {
    if (heatRingAdvance(ring, series, columns, G) === null) {
      ring = heatRing(series, columns, G);
    }
    assert.deepEqual(picture(ring), picture(heatRing(series, columns, G)));
  };
  let series = history(0, 4, 300);
  for (let tick = 1; tick <= 40; tick += 1) {
    series = history(tick, 4, 300);
    draw(series, 251);
  }
  series = series.map(samples => samples.slice(3).concat([null, null, 50]));
  draw(series, 251);
  for (let tick = 1; tick <= 40; tick += 1) {
    series = series.map((samples, row) => samples.slice(1).concat([(tick * 13 + row * 7) % 101]));
    draw(series, 251);
  }
  draw(series, 200);
  for (let tick = 1; tick <= 40; tick += 1) {
    series = series.map((samples, row) => samples.slice(1).concat([(tick * 17 + row) % 101]));
    draw(series, 200);
  }
});

test('the same series again changes nothing', () => {
  const series = history(0, 4, 300);
  const ring = heatRing(series, 251, G);
  assert.deepEqual(heatRingAdvance(ring, series.slice(), 251, G), []);
  assert.equal(ring.head, 0);
});

test('row height', () => assert.equal(heatRowHeight(70, 16, 1), 55 / 16));
