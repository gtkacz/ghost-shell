import test from 'node:test';
import assert from 'node:assert/strict';
import { fromHistory, gapLength, withData } from '../kit/GhostKit/lib/history.mjs';

const LENGTH = 300;

const cpu = (load, ccdLoads = []) => ({ load, ccds: ccdLoads.map((ccdLoad, id) => ({ id, load: ccdLoad })) });
const gpu = (present, load) => ({ present, load });
const data = (topic, seq, value) => ({ t: 'data', topic, seq, data: value });

const recorded = (track, messages, length = LENGTH) => messages.reduce((current, message) => withData(current, message, length), track);

test('a message one tick after the last adds no nulls', () => {
  const track = recorded(fromHistory({}), [data('cpu', 5, cpu(10)), data('cpu', 6, cpu(20))]);
  assert.deepEqual(track.series.load, [10, 20]);
});

test('a jump of 4 ticks adds 3 nulls before the value', () => {
  const track = recorded(fromHistory({}), [data('cpu', 5, cpu(10)), data('cpu', 9, cpu(20))]);
  assert.deepEqual(track.series.load, [10, null, null, null, 20]);
});

test('a jump longer than the series adds at most its length of nulls', () => {
  assert.equal(gapLength(5, 1000, 3), 3);
  const track = recorded(fromHistory({}), [data('cpu', 5, cpu(10)), data('cpu', 6, cpu(20)), data('cpu', 1000, cpu(30))], 3);
  assert.deepEqual(track.series.load, [null, null, 30]);
});

test('the value replayed after a history is not appended again, and its seq is the baseline', () => {
  const history = fromHistory({ load: [1, 2], ccd0: [1, 2], ccd1: [1, 2] });
  const replayed = withData(history, data('cpu', 10, cpu(2, [2, 2])), LENGTH);
  assert.deepEqual(replayed.series, { load: [1, 2], ccd0: [1, 2], ccd1: [1, 2] });

  const resumed = withData(replayed, data('cpu', 12, cpu(3, [4, 5])), LENGTH);
  assert.deepEqual(resumed.series, { load: [1, 2, null, 3], ccd0: [1, 2, null, 4], ccd1: [1, 2, null, 5] });
});

test('only a topic with samples in the history waits for a replay', () => {
  const track = recorded(fromHistory({ gpu_load: [7] }), [data('gpu', 4, gpu(true, 7)), data('cpu', 4, cpu(10))]);
  assert.deepEqual(track.series, { gpu_load: [7], load: [10] });
});

test('an absent GPU records null, as the daemon does', () => {
  const track = recorded(fromHistory({}), [data('gpu', 1, gpu(false, 0)), data('gpu', 2, gpu(true, 55))]);
  assert.deepEqual(track.series.gpu_load, [null, 55]);
});

test('a topic without history leaves the track as it was', () => {
  const track = fromHistory({ load: [1] });
  assert.equal(withData(track, data('mem', 3, { used_kib: 1 }), LENGTH), track);
});

test('recording returns new series and leaves published ones unchanged', () => {
  const before = recorded(fromHistory({}), [data('cpu', 1, cpu(10))]);
  const published = before.series;
  const after = withData(before, data('cpu', 2, cpu(20)), LENGTH);
  assert.notEqual(after.series, published);
  assert.deepEqual(published, { load: [10] });
});
