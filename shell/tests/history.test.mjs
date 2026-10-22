import test from 'node:test';
import assert from 'node:assert/strict';
import { fromHistory, gapLength, topicOfKey, withData } from '../kit/GhostKit/lib/history.mjs';

const LENGTH = 300;

const cpu = (load, ccdLoads = []) => ({ load, ccds: ccdLoads.map((ccdLoad, id) => ({ id, load: ccdLoad })), cores: [] });
const gpu = (present, load, temp_c = 0) => ({ present, load, temp_c });
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
  assert.equal(withData(track, data('host', 3, { hostname: 'h' }), LENGTH), track);
});

test('every key maps to one topic', () => {
  assert.equal(topicOfKey('core3'), 'cpu');
  assert.equal(topicOfKey('ccd1'), 'cpu');
  assert.equal(topicOfKey('temp_gpu'), 'gpu');
  assert.equal(topicOfKey('mem_used'), 'mem');
  assert.equal(topicOfKey('temp_cpu'), 'sensors');
  assert.equal(topicOfKey('temp_nvme'), 'disks');
  assert.equal(topicOfKey('net_up'), 'net');
  assert.equal(topicOfKey('nope'), null);
  assert.equal(topicOfKey('corex'), null);
  assert.equal(topicOfKey('core'), null);
});

test('net data records down and up', () => {
  const t = withData(fromHistory({}), { topic: 'net', seq: 1, data: { rx_bps: 10, tx_bps: 2, interfaces: [] } }, 300);
  assert.deepEqual([t.series.net_down, t.series.net_up], [[10], [2]]);
});

test('cpu data appends one sample per core', () => {
  const t = withData(fromHistory({}), { topic: 'cpu', seq: 1, data: { load: 10, ccds: [], cores: [{ cpus: [0, 16], load: 20 }] } }, 300);
  assert.deepEqual(t.series.core0, [20]);
});

test('disks data sums live drives and keeps the hottest nvme', () => {
  const drive = (name, state, read_bps, temp_c) => ({ kernel_name: name, state, read_bps, write_bps: 0, temp_c });
  const t = withData(
    fromHistory({}),
    {
      topic: 'disks',
      seq: 1,
      data: { drives: [drive('nvme0n1', 'live', 1e6, 38), drive('nvme1n1', 'live', 2e6, 41), drive('nvme2n1', 'dead', 9e6, null), drive('sda', 'live', 0, 50)] },
    },
    300,
  );
  assert.deepEqual([t.series.disk_read, t.series.disk_write, t.series.temp_nvme], [[3e6], [0], [41]]);
});

test('disks without a live nvme reading record a null temperature', () => {
  const drive = (name, state, temp_c) => ({ kernel_name: name, state, read_bps: 0, write_bps: 0, temp_c });
  const t = withData(fromHistory({}), data('disks', 1, { drives: [drive('nvme1n1', 'dead', null), drive('nvme0n1', 'unmounted', 70), drive('sda', 'live', 30)] }), LENGTH);
  assert.deepEqual(t.series.temp_nvme, [null]);
});

test('a gap before mem data is filled with nulls', () => {
  const at = (seq, used_kib) => ({ topic: 'mem', seq, data: { used_kib } });
  const t = withData(withData(fromHistory({}), at(1, 1048576), 300), at(4, 2097152), 300);
  assert.deepEqual(t.series.mem_used, [1, null, null, 2]);
});

test('mem_used is rounded to the f32 the daemon keeps', () => {
  const track = recorded(fromHistory({}), [data('mem', 1, { used_kib: 96364133 })]);
  // (96364133 / 1048576) as f32 in the daemon; the unrounded quotient is 91.89999866485596.
  assert.deepEqual(track.series.mem_used, [91.9000015258789]);
});

test('sensors data records the first CPU package reading', () => {
  const reading = (id, value) => ({ id, label: id, value });
  const package_ = recorded(fromHistory({}), [data('sensors', 1, { temps: [reading('nct6799/SYSTIN', 31), reading('k10temp/Tctl', 58)], fans: [] })]);
  const none = recorded(fromHistory({}), [data('sensors', 1, { temps: [reading('nct6799/SYSTIN', 31)], fans: [] })]);
  assert.deepEqual([package_.series.temp_cpu, none.series.temp_cpu], [[58], [null]]);
});

test('gpu data records its temperature, null while absent', () => {
  const track = recorded(fromHistory({}), [data('gpu', 1, gpu(false, 0, 40)), data('gpu', 2, gpu(true, 55, 61))]);
  assert.deepEqual(track.series.temp_gpu, [null, 61]);
});

test('a history with mem samples waits for the replayed mem value', () => {
  const history = fromHistory({ mem_used: [1, 2] });
  const replayed = withData(history, data('mem', 9, { used_kib: 2097152 }), LENGTH);
  const resumed = withData(replayed, data('mem', 10, { used_kib: 3145728 }), LENGTH);
  assert.deepEqual([replayed.series.mem_used, resumed.series.mem_used], [[1, 2], [1, 2, 3]]);
});

test('recording returns new series and leaves published ones unchanged', () => {
  const before = recorded(fromHistory({}), [data('cpu', 1, cpu(10))]);
  const published = before.series;
  const after = withData(before, data('cpu', 2, cpu(20)), LENGTH);
  assert.notEqual(after.series, published);
  assert.deepEqual(published, { load: [10] });
});
