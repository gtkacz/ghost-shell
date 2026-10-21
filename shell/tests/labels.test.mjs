import test from 'node:test';
import assert from 'node:assert/strict';
import { cpuModelShort, gpuNameShort, ccdTitle, readingByLabel } from '../kit/GhostKit/lib/labels.mjs';

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
  temps: [{ id: 'k10temp/Tctl', label: 'CPU TCTL', value: 57.9 }],
  fans: [{ id: 'nct6799/fan1', label: 'PUMP', value: 1670 }, { id: 'nct6799/fan2', label: 'CPU FAN', value: null }],
};

test('readingByLabel finds a temperature or a fan by the label the daemon reports', () => {
  assert.deepEqual(readingByLabel(SNAPSHOT, 'CPU TCTL'), { id: 'k10temp/Tctl', label: 'CPU TCTL', value: 57.9 });
  assert.equal(readingByLabel(SNAPSHOT, 'PUMP').value, 1670);
});

test('readingByLabel follows a label the user gave a sensor, not the chip it sits on', () => {
  const renamed = { temps: [{ id: 'coretemp/Package id 0', label: 'CPU TCTL', value: 44 }], fans: [] };
  assert.equal(readingByLabel(renamed, 'CPU TCTL').value, 44);
});

test('readingByLabel keeps a reading whose value is unavailable', () => {
  assert.equal(readingByLabel(SNAPSHOT, 'CPU FAN').value, null);
});

test('readingByLabel does not match on the sensor id', () => {
  assert.equal(readingByLabel(SNAPSHOT, 'k10temp/Tctl'), null);
});

test('readingByLabel is null for an unknown label or a missing snapshot', () => {
  assert.equal(readingByLabel(SNAPSHOT, 'nope'), null);
  assert.equal(readingByLabel(null, 'CPU FAN'), null);
  assert.equal(readingByLabel({}, 'CPU FAN'), null);
});
