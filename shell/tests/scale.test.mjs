import test from 'node:test';
import assert from 'node:assert/strict';
import { designUnit, pickScreen } from '../kit/GhostKit/lib/scale.mjs';

test('design unit at 1.15', () => assert.ok(Math.abs(designUnit(2560 / 1.15) - 1.739) < 0.001));

test('design unit is one at the mockup width', () => assert.equal(designUnit(1280), 1));

test('screen pick', () => {
  assert.equal(pickScreen([{ model: 'A' }, { model: 'Gigabyte M27Q' }], 'M27Q').model, 'Gigabyte M27Q');
  assert.equal(pickScreen([{ model: 'A' }], '').model, 'A');
  assert.equal(pickScreen([], ''), null);
});

test('screen pick matches case-insensitively', () => {
  assert.equal(pickScreen([{ model: 'A' }, { model: 'Gigabyte M27Q' }], 'gigabyte m27q').model, 'Gigabyte M27Q');
});

test('a model matching nothing falls back to the first screen', () => {
  assert.equal(pickScreen([{ model: 'A' }, { model: 'B' }], 'M27Q').model, 'A');
});

test('no screens gives null even when a model is requested', () => assert.equal(pickScreen([], 'M27Q'), null));
