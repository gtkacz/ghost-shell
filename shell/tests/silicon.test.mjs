import test from 'node:test';
import assert from 'node:assert/strict';
import * as format from '../kit/GhostKit/lib/format.mjs';
import * as labels from '../kit/GhostKit/lib/labels.mjs';
import { headerMeta, titleLine, ccdTiles, l3Text, iodBlocks, packetPhase, dieLayout, chamferPath, hatchPath } from '../../widgets/silicon/silicon.mjs';

const FMT = { ...format, ...labels };
const WARN_AT = 0.75;
const CRIT_AT = 0.9;
const KIB_PER_GIB = 1024 * 1024;

// The fake daemon's machine: two CCDs of eight cores, logical CPU n and n + 16 the threads of one core.
const SIBLING = 16;
const range = (from, to) => Array.from({ length: to - from }, (_, i) => from + i);
const FAKE_CPU = {
  model: 'AMD Ryzen 9 9950X3D 16-Core Processor',
  threads: range(0, 32).map((cpu) => cpu * 3 % 97),
  ccds: [
    { id: 0, cpus: [...range(0, 8), ...range(16, 24)], l3_kib: 98_304, vcache: true, load: 40 },
    { id: 1, cpus: [...range(8, 16), ...range(24, 32)], l3_kib: 32_768, vcache: false, load: 20 },
  ],
  cores: range(0, 16).map((core) => ({ cpus: [core, core + SIBLING], load: 0 })),
  load: 30,
  freq_mhz: 5050,
  package_w: 45.2,
  igpu_busy_pct: 7,
};
const MEM = { total_kib: 96_364_134, used_kib: Math.round(19.8 * KIB_PER_GIB), swap_total_kib: 0, swap_used_kib: 0 };
const GPU = { present: true, name: 'NVIDIA GeForce RTX 4080 SUPER', load: 12.4, power_w: 54.1 };
const drive = (kernelName, state, readBps) => ({ id: kernelName, label: kernelName, model: kernelName, kernel_name: kernelName, state, mounts: [], used_pct: null, temp_c: null, read_bps: readBps, write_bps: 0 });
const DISKS = {
  drives: [
    drive('nvme0n1', 'live', 100_000_000),
    drive('nvme2n1', 'live', 20_000_000),
    drive('nvme1n1', 'dead', 900_000_000),
    drive('sda', 'live', 300_000_000),
  ],
};

test('header meta', () => {
  assert.equal(headerMeta({ package_w: 45.2 }, { present: true, power_w: 54.1 }), 'PKG 45 W · SYS ≈ 99 W');
  assert.equal(headerMeta({ package_w: null }, { present: true, power_w: 54 }), '');
});

test('header meta without a gpu shows the package alone', () => {
  assert.equal(headerMeta({ package_w: 45.2 }, { present: false, power_w: 0 }), 'PKG 45 W');
  assert.equal(headerMeta({ package_w: 45.2 }, null), 'PKG 45 W');
  assert.equal(headerMeta(null, GPU), '');
});

test('title', () => assert.equal(titleLine(FAKE_CPU), 'AMD RYZEN 9 9950X3D · 16C / 32T'));

test('narrow title keeps the model number', () => assert.equal(titleLine(FAKE_CPU, true), '9950X3D · 16C / 32T'));

test('title before the first cpu sample', () => assert.equal(titleLine(null), '--'));

test('tiles pair each core with its threads', () => {
  const [ccd0] = ccdTiles(FAKE_CPU);
  assert.equal(ccd0.length, 8);
  assert.deepEqual(ccd0[0].loads, [FAKE_CPU.threads[0], FAKE_CPU.threads[16]]);
  assert.equal(ccdTiles(FAKE_CPU)[1][0].name, 'C8');
});

test('tiles count cores across ccds in ccd order', () => {
  const tiles = ccdTiles(FAKE_CPU, WARN_AT, CRIT_AT);
  assert.deepEqual(tiles.map((ccd) => ccd.map((tile) => tile.name)), [
    ['C0', 'C1', 'C2', 'C3', 'C4', 'C5', 'C6', 'C7'],
    ['C8', 'C9', 'C10', 'C11', 'C12', 'C13', 'C14', 'C15'],
  ]);
  assert.deepEqual(tiles[1][7].loads, [FAKE_CPU.threads[15], FAKE_CPU.threads[31]]);
});

test('a tile takes the level of its hotter thread', () => {
  const threads = range(0, 32).map(() => 10);
  threads[17] = 80;
  threads[2] = 95;
  const tiles = ccdTiles({ ...FAKE_CPU, threads }, WARN_AT, CRIT_AT)[0];
  assert.equal(tiles[0].level, 'ok');
  assert.equal(tiles[1].level, 'warn');
  assert.equal(tiles[2].level, 'crit');
});

test('l3 text', () => {
  assert.equal(l3Text(FAKE_CPU.ccds, FAKE_CPU.ccds[0], false), 'L3 32 MB + 3D V-CACHE 64 MB');
  assert.equal(l3Text(FAKE_CPU.ccds, FAKE_CPU.ccds[1], true), 'L3 32M');
});

test('l3 text in the other widths', () => {
  assert.equal(l3Text(FAKE_CPU.ccds, FAKE_CPU.ccds[0], true), 'L3 32M + V-CACHE 64M');
  assert.equal(l3Text(FAKE_CPU.ccds, FAKE_CPU.ccds[1], false), 'L3 32 MB');
});

test('a v-cache ccd with no plain ccd to compare shows its whole l3', () => {
  const only = [{ id: 0, cpus: [], l3_kib: 98_304, vcache: true, load: 0 }];
  assert.equal(l3Text(only, only[0], false), 'L3 96 MB');
});

test('iod blocks', () => {
  const b = iodBlocks(MEM, GPU, DISKS, { igpu_busy_pct: null }, FMT);
  assert.deepEqual(b.map((x) => x.name), ['MEM', 'GPU', 'NVME', 'IGPU']);
  assert.equal(b[3].value, 'OFF');
});

test('iod block values and fractions', () => {
  const [mem, gpu, nvme, igpu] = iodBlocks(MEM, GPU, DISKS, FAKE_CPU, FMT);
  assert.equal(mem.value, '19.8 / 92G');
  assert.ok(Math.abs(mem.fraction - MEM.used_kib / MEM.total_kib) < 1e-9);
  assert.equal(gpu.value, 'RTX 4080 SUPER 12%');
  assert.ok(Math.abs(gpu.fraction - 0.124) < 1e-9);
  assert.equal(nvme.value, 'R 120 MB/s');
  assert.ok(Math.abs(nvme.fraction - 0.3) < 1e-9);
  assert.equal(igpu.value, '7%');
  assert.ok(Math.abs(igpu.fraction - 0.07) < 1e-9);
});

test('an igpu without a driver reads off and lights nothing', () => {
  const igpu = iodBlocks(MEM, GPU, DISKS, { ...FAKE_CPU, igpu_busy_pct: null }, FMT)[3];
  assert.equal(igpu.value, 'OFF');
  assert.equal(igpu.fraction, null);
});

test('a busy nvme fills its meter and no further', () => {
  const nvme = iodBlocks(MEM, GPU, { drives: [drive('nvme0n1', 'live', 1_200_000_000)] }, FAKE_CPU, FMT)[2];
  assert.equal(nvme.value, 'R 1200 MB/s');
  assert.equal(nvme.fraction, 1);
});

test('iod blocks before any data are missing, not zero', () => {
  const blocks = iodBlocks(null, { present: false, name: '', load: 0 }, null, null, FMT);
  assert.deepEqual(blocks.map((x) => x.value), ['--', '--', '--', '--']);
  assert.deepEqual(blocks.map((x) => x.fraction), [null, null, null, null]);
});

test('packet phase', () => {
  assert.equal(packetPhase(0, 0, 1), 0.5);
  assert.ok(Math.abs(packetPhase(1000, 10, 0) - 0.7) < 1e-9);
});

test('packet phase wraps into 0..1', () => {
  const phase = packetPhase(123_456, 97, 1);
  assert.ok(phase >= 0 && phase < 1);
});

test('only the nvme meter never alarms', () => {
  assert.deepEqual(iodBlocks(MEM, GPU, DISKS, FAKE_CPU, FMT).map((x) => x.alarms), [true, true, false, true]);
});

test('the die turns small below 380 design px', () => {
  assert.equal(dieLayout(379, 200, [8, 8]).small, true);
  assert.equal(dieLayout(380, 200, [8, 8]).small, false);
});

test('the die lays out a tile per core and a link per ccd', () => {
  const die = dieLayout(900, 400, [8, 8]);
  assert.deepEqual(die.ccds.map((ccd) => ccd.tiles.length), [8, 8]);
  assert.equal(die.ccds[0].tiles[3].y, die.ccds[0].tiles[0].y);
  assert.ok(die.ccds[0].tiles[4].y > die.ccds[0].l3.y + die.ccds[0].l3.h);
  for (const ccd of die.ccds) {
    assert.equal(ccd.link.x, ccd.x + ccd.w / 2);
    assert.equal(ccd.link.y0, ccd.y + ccd.h);
    assert.equal(ccd.link.y1, die.iod.y);
  }
  assert.equal(die.blocks.length, 4);
  const last = die.blocks[3];
  assert.ok(Math.abs(last.x + last.w - (die.iod.x + die.iod.w - 8)) < 1e-9);
});

test('a tile too short for its bars gets an empty graph, not a negative one', () => {
  const die = dieLayout(276, 137, [8, 8]);
  assert.ok(die.ccds[0].tiles.every((tile) => tile.graph.h >= 0));
});

test('chamfer path cuts the top-right and bottom-left corners', () => {
  assert.equal(chamferPath({ x: 0, y: 0, w: 101, h: 51 }, 10, 2), 'M 1 1 L 181 1 L 201 21 L 201 101 L 21 101 L 1 81 Z');
});

test('hatch stripes start left of the strip so its left edge is covered', () => {
  const path = hatchPath(20, 10, 1);
  assert.ok(path.startsWith('M -10 10 L 0 0'));
  assert.equal(path.split('M').length - 1, 6);
});

test('a block too short for its meter leaves it out rather than drawing it above the block', () => {
  assert.ok(dieLayout(900, 400, [8, 8]).blocks.every((block) => block.showsMeter));
  assert.ok(dieLayout(276, 137, [8, 8]).blocks.every((block) => !block.showsMeter));
});

const within = (baseline, top, height) => baseline >= top && baseline <= top + height;

test('no text baseline falls outside its block or row at the v1 or v2 size', () => {
  for (const [w, h] of [[276, 137], [276, 158], [1250, 560]]) {
    const die = dieLayout(w, h, [8, 8]);
    assert.ok(within(die.titleBaseline, die.package.y, die.package.h), `title at ${w}×${h}`);
    assert.ok(within(die.iodBaseline, die.iod.y, die.iod.h), `iod head at ${w}×${h}`);
    for (const ccd of die.ccds) {
      assert.ok(within(ccd.headBaseline, ccd.y, ccd.h), `ccd head at ${w}×${h}`);
      assert.ok(within(ccd.l3Baseline, ccd.l3.y, ccd.l3.h), `l3 at ${w}×${h}`);
      for (const tile of ccd.tiles) {
        assert.ok(within(tile.labelBaseline, tile.y, tile.h), `tile label at ${w}×${h}`);
      }
    }
    if (die.showsBlocks !== false) {
      for (const block of die.blocks) {
        assert.ok(within(block.nameBaseline, block.y, block.h - 1), `block name at ${w}×${h}`);
        assert.ok(!block.showsValue || within(block.valueBaseline, block.y, block.h - 1), `block value at ${w}×${h}`);
      }
    }
  }
});

test('blocks too short for their names give way to names on the iod head row', () => {
  assert.equal(dieLayout(276, 137, [8, 8]).showsBlocks, false);
  assert.equal(dieLayout(1250, 560, [8, 8]).showsBlocks, true);
});
