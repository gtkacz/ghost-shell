import test from 'node:test';
import assert from 'node:assert/strict';
import { cpuModelShort, gpuNameShort, ccdTitle, reading } from '../kit/GhostKit/lib/labels.mjs';

test('short names', () => {
  assert.equal(cpuModelShort('AMD Ryzen 9 9950X3D 16-Core Processor'), 'RYZEN 9 9950X3D');
  assert.equal(gpuNameShort('NVIDIA GeForce RTX 4080 SUPER'), 'RTX 4080 SUPER');
  assert.equal(ccdTitle({ id: 0, vcache: true }), 'CCD0 · V-CACHE');
});

test('intel cpu names lose the vendor prefix', () => {
  assert.equal(cpuModelShort('Intel(R) Core(TM) i9-14900K'), 'I9-14900K');
});

test('amd gpu names lose the vendor prefix', () => {
  assert.equal(gpuNameShort('AMD Radeon RX 7900 XTX'), 'RX 7900 XTX');
});

test('short names are empty before the daemon has reported a model', () => {
  for (const missing of [null, undefined]) {
    assert.equal(cpuModelShort(missing), '');
    assert.equal(gpuNameShort(missing), '');
  }
});

test('the non-vcache die is titled FREQ', () => {
  assert.equal(ccdTitle({ id: 1, vcache: false }), 'CCD1 · FREQ');
});

const SNAPSHOT = {
  temps: [{ id: 'cpu_tctl', label: 'CPU', value: 57.9 }],
  fans: [{ id: 'fan1', label: 'Fan 1', value: 1670 }, { id: 'fan2', label: 'Fan 2', value: null }],
};

test('reading finds a temperature or a fan by id', () => {
  assert.deepEqual(reading(SNAPSHOT, 'cpu_tctl'), { id: 'cpu_tctl', label: 'CPU', value: 57.9 });
  assert.equal(reading(SNAPSHOT, 'fan1').value, 1670);
});

test('reading keeps a reading whose value is unavailable', () => {
  assert.equal(reading(SNAPSHOT, 'fan2').value, null);
});

test('reading is null for an unknown id or a missing snapshot', () => {
  assert.equal(reading(SNAPSHOT, 'nope'), null);
  assert.equal(reading(null, 'fan1'), null);
  assert.equal(reading({}, 'fan1'), null);
});
