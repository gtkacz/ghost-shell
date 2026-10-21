import test from 'node:test';
import assert from 'node:assert/strict';
import { cellCount, litCells, level, levelLow, levelAbove, cellColor, rampAlphaByte } from '../kit/GhostKit/lib/segments.mjs';

test('cells and levels', () => {
  assert.equal(cellCount(266), 44);
  assert.equal(level(.75, .75, .9), 'warn');
  assert.equal(level(.9, .75, .9), 'crit');
  assert.equal(levelLow(20, 20, 10), 'warn');
  assert.equal(levelLow(10, 20, 10), 'crit');
});

test('level is ok below the warn threshold and levelLow above it', () => {
  assert.equal(level(.74, .75, .9), 'ok');
  assert.equal(levelLow(21, 20, 10), 'ok');
});

test('an unavailable reading is never an alarm', () => {
  for (const missing of [null, undefined, NaN]) {
    assert.equal(level(missing, .75, .9), 'ok');
    assert.equal(levelLow(missing, 20, 10), 'ok');
  }
});

test('cellCount never goes negative for a width narrower than one cell', () => {
  assert.equal(cellCount(0), 0);
  assert.equal(cellCount(3), 0);
  assert.equal(cellCount(4), 1);
});

test('litCells scales the value across the cells and clamps to the range', () => {
  assert.equal(litCells(50, 0, 100, 20), 10);
  assert.equal(litCells(-5, 0, 100, 20), 0);
  assert.equal(litCells(150, 0, 100, 20), 20);
});

test('litCells keeps at least the minimum lit', () => {
  assert.equal(litCells(0, 0, 100, 20, 1), 1);
  assert.equal(litCells(50, 0, 100, 20, 1), 10);
});

test('litCells returns the minimum instead of NaN for an empty range', () => {
  assert.equal(litCells(5, 10, 10, 20), 0);
  assert.equal(litCells(5, 10, 10, 20, 1), 1);
});

test('litCells treats a missing value as nothing lit', () => {
  assert.equal(litCells(NaN, 0, 100, 20), 0);
  assert.equal(litCells(undefined, 0, 100, 20), 0);
});

const THEME = { signal: '#a3d4cc', hi: '#e4f1ef', warn: '#e9a84c', crit: '#ff3b45' };

test('unlit cells are signal at six percent', () => {
  assert.equal(cellColor('ok', 5, 5, 10, THEME), '#0FA3D4CC');
  assert.equal(cellColor('crit', 7, 5, 10, THEME), '#0FA3D4CC');
});

test('warn and crit levels colour every lit cell', () => {
  assert.equal(cellColor('crit', 0, 5, 10, THEME), THEME.crit);
  assert.equal(cellColor('crit', 4, 5, 10, THEME), THEME.crit);
  assert.equal(cellColor('warn', 2, 5, 10, THEME), THEME.warn);
});

test('the top lit cell of an ok bar is the highlight colour', () => {
  assert.equal(cellColor('ok', 4, 5, 10, THEME), THEME.hi);
});

test('lower lit cells ramp from dim to bright signal', () => {
  assert.equal(cellColor('ok', 0, 5, 10, THEME), '#59A3D4CC');
  assert.equal(cellColor('ok', 2, 5, 10, THEME), '#73A3D4CC');
});

test('the ramp alpha byte is the one cellColor gives a lit ok cell below the top', () => {
  assert.equal(rampAlphaByte(0, 10), 0x59);
  assert.equal(rampAlphaByte(2, 10), 0x73);
  const alphaOf = color => Number.parseInt(color.slice(1, 3), 16);
  for (let rows = 1; rows <= 300; rows += 1) {
    for (let row = 0; row < rows; row += 1) {
      assert.equal(rampAlphaByte(row, rows), alphaOf(cellColor('ok', row, row + 2, rows, THEME)), `row ${row} of ${rows}`);
    }
  }
});

test('aqi levels are strictly above', () => {
  assert.equal(levelAbove(100, 100, 150), 'ok');
  assert.equal(levelAbove(101, 100, 150), 'warn');
  assert.equal(levelAbove(150, 100, 150), 'warn');
  assert.equal(levelAbove(151, 100, 150), 'crit');
  assert.equal(levelAbove(null, 100, 150), 'ok');
});

test('levelAbove never alarms on an unavailable reading', () => {
  for (const missing of [null, undefined, NaN]) {
    assert.equal(levelAbove(missing, 100, 150), 'ok');
  }
});
