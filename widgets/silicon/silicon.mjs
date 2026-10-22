// The silicon schematic's text and numbers, apart from QML so that node can test it. They follow the reference
// mockup's drawDie, less its AM5, DDR5, PCIE 5 X16 and 6 NM, which no topic carries. A widget's helper reaches the
// kit only through the GhostKit import, so iodBlocks is handed the kit's formatters (Fmt in QML, lib/format.mjs and
// lib/labels.mjs in node) rather than importing them.

// As the kit's formatters print a missing value.
export const MISSING = '--';
// The NVMe meter's full scale, the mockup's 400 MB/s.
export const NVME_FULL_BPS = 400_000_000;
// The packets' speed, from the mockup: a base rate and a share per point of the CCD's load, in cycles per ms.
export const PACKET_BASE_PER_MS = 0.0004;
export const PACKET_PER_LOAD_PER_MS = 0.00003;
// Each link's packet starts half a cycle after the previous link's, so they never move in step.
export const PACKET_LINK_OFFSET = 0.5;

const SEPARATOR = ' · ';
const PERCENT = 100;
const KIB_PER_MIB = 1024;
const KIB_PER_GIB = 1024 * 1024;
const BYTES_PER_MB = 1e6;
// The kernel's model string ends in the core count, which the title prints from the topology instead.
const CPU_CORE_COUNT_SUFFIX = /\s+\d+-Core Processor$/i;
const NVME_KERNEL_PREFIX = 'nvme';
const LIVE_DRIVE = 'live';
const IGPU_OFF = 'OFF';

function finite(value) {
  return Number.isFinite(value) ? value : null;
}

function clampUnit(value) {
  return Math.min(1, Math.max(0, value));
}

// SYS is the package and the graphics card only, so without the package figure it would understate the machine.
export function headerMeta(cpu, gpu) {
  const packageW = cpu ? finite(cpu.package_w) : null;
  if (packageW === null) {
    return '';
  }
  const pkg = 'PKG ' + Math.round(packageW) + ' W';
  const gpuW = gpu && gpu.present ? finite(gpu.power_w) : null;
  if (gpuW === null) {
    return pkg;
  }
  return pkg + SEPARATOR + 'SYS ≈ ' + Math.round(packageW + gpuW) + ' W';
}

// The narrow form keeps the model's last word, its number, as the mockup's narrow title does.
export function titleLine(cpu, narrow = false) {
  if (!cpu || typeof cpu.model !== 'string' || cpu.model.trim() === '') {
    return MISSING;
  }
  const model = cpu.model.trim().replace(CPU_CORE_COUNT_SUFFIX, '').toUpperCase();
  const name = narrow ? model.split(/\s+/).pop() : model;
  return name + SEPARATOR + cpu.cores.length + 'C / ' + cpu.threads.length + 'T';
}

function loadLevel(fraction, warnAt, critAt) {
  if (fraction >= critAt) {
    return 'crit';
  }
  if (fraction >= warnAt) {
    return 'warn';
  }
  return 'ok';
}

// Per CCD, a tile per physical core: the core's threads in cpu order, and the level of the busier one. warnAt and
// critAt are fractions, as Theme.loadWarn and Theme.loadCrit.
export function ccdTiles(cpu, warnAt, critAt) {
  if (!cpu) {
    return [];
  }
  let next = 0;
  return cpu.ccds.map((ccd) => {
    const members = new Set(ccd.cpus);
    return cpu.cores
      .filter((core) => members.has(core.cpus[0]))
      .map((core) => {
        const loads = [...core.cpus].sort((a, b) => a - b).map((thread) => cpu.threads[thread]);
        const hottest = Math.max(...loads.map((load) => finite(load) ?? 0));
        const tile = { name: 'C' + next, loads, level: loadLevel(hottest / PERCENT, warnAt, critAt) };
        next += 1;
        return tile;
      });
  });
}

// A V-Cache CCD's extra L3 is what it has over the smallest plain CCD; with no plain CCD to compare, its whole L3.
export function l3Text(ccds, ccd, narrow) {
  const mb = (kib) => Math.round(kib / KIB_PER_MIB) + (narrow ? 'M' : ' MB');
  const plain = ccds.filter((other) => !other.vcache).map((other) => other.l3_kib);
  if (!ccd.vcache || plain.length === 0) {
    return 'L3 ' + mb(ccd.l3_kib);
  }
  const base = Math.min(...plain);
  return 'L3 ' + mb(base) + ' + ' + (narrow ? 'V-CACHE ' : '3D V-CACHE ') + mb(ccd.l3_kib - base);
}

function memBlock(mem, fmt) {
  if (!mem || !(mem.total_kib > 0)) {
    return { name: 'MEM', value: MISSING, fraction: null, alarms: true };
  }
  return {
    name: 'MEM',
    value: fmt.gib(mem.used_kib, 1) + ' / ' + Math.round(mem.total_kib / KIB_PER_GIB) + 'G',
    fraction: clampUnit(mem.used_kib / mem.total_kib),
    alarms: true,
  };
}

function gpuBlock(gpu, fmt) {
  if (!gpu || !gpu.present || finite(gpu.load) === null) {
    return { name: 'GPU', value: MISSING, fraction: null, alarms: true };
  }
  return { name: 'GPU', value: fmt.gpuNameShort(gpu.name) + ' ' + Math.round(gpu.load) + '%', fraction: clampUnit(gpu.load / PERCENT), alarms: true };
}

function nvmeBlock(disks) {
  const live = disks ? disks.drives.filter((drive) => drive.state === LIVE_DRIVE && drive.kernel_name.startsWith(NVME_KERNEL_PREFIX)) : [];
  if (live.length === 0) {
    return { name: 'NVME', value: MISSING, fraction: null, alarms: false };
  }
  const readBps = live.reduce((sum, drive) => sum + drive.read_bps, 0);
  return { name: 'NVME', value: 'R ' + Math.round(readBps / BYTES_PER_MB) + ' MB/s', fraction: clampUnit(readBps / NVME_FULL_BPS), alarms: false };
}

function igpuBlock(cpu) {
  if (!cpu) {
    return { name: 'IGPU', value: MISSING, fraction: null, alarms: true };
  }
  const busy = finite(cpu.igpu_busy_pct);
  if (busy === null) {
    return { name: 'IGPU', value: IGPU_OFF, fraction: null, alarms: true };
  }
  return { name: 'IGPU', value: Math.round(busy) + '%', fraction: clampUnit(busy / PERCENT), alarms: true };
}

// The I/O die's blocks, each with its text and a 0-1 fraction for its meter, or null where there is no reading.
// A read rate is no alarm however high, so the NVMe meter never takes a warning colour (alarms false).
// fmt: the kit's gib and gpuNameShort.
export function iodBlocks(mem, gpu, disks, cpu, fmt) {
  return [memBlock(mem, fmt), gpuBlock(gpu, fmt), nvmeBlock(disks), igpuBlock(cpu)];
}

// Where along its link, 0 at the I/O die and 1 at the CCD, link k's packet is after elapsedMs; a busier CCD's
// packet moves faster.
export function packetPhase(elapsedMs, loadPct, k) {
  return (elapsedMs * (PACKET_BASE_PER_MS + loadPct * PACKET_PER_LOAD_PER_MS) + k * PACKET_LINK_OFFSET) % 1;
}

// Design pixels, from the reference mockup's drawDie.
export const SMALL_BELOW_PX = 380;
const WIDE = Object.freeze({ fs: 9, pad: 14, gap: 12, titleDrop: 20, titleBaseline: 14 });
const SMALL = Object.freeze({ fs: 7, pad: 6, gap: 6, titleDrop: 14, titleBaseline: 10 });
const INSET = 10;
const CCD_SHARE = 0.6;
const IOD_GAP_SCALE = 1.6;
export const PACKAGE_CHAMFER_PX = 12;
export const BLOCK_CHAMFER_PX = 8;
const HEAD_BASELINE_PX = 6;
const HEAD_RIGHT_PX = 10;
const CONTENT_SIDE_PX = 8;
const CONTENT_TOP_PX = 12;
const CONTENT_TRIM_PX = 18;
const TILE_GAP_PX = 4;
const TILE_TEXT_PX = 4;
const TILE_GRAPH_TRIM_PX = 8;
const TILE_BAR_MIN_PX = 2;
const TILE_BAR_TRIM_PX = 14;
const L3_GAP_PX = 4;
const L3_HEIGHT_PX = 10;
const L3_BASELINE_PX = 8;
const L3_AFTER_PX = L3_HEIGHT_PX + L3_GAP_PX;
// The L3 strip and the gaps above and below it, between the two rows of tiles.
const L3_BAND_PX = L3_GAP_PX + L3_HEIGHT_PX + L3_GAP_PX;
const LINK_LABEL_PX = 6;
const LINK_LABEL_DROP_PX = 3;
const BLOCK_GAP_PX = 8;
const BLOCK_TEXT_PX = 5;
const BLOCK_NAME_DROP_PX = 1;
const BLOCK_VALUE_DROP_PX = 4;
const BLOCK_VALUE_ROOM_PX = 10;
const METER_FROM_BOTTOM_PX = 9;
const METER_HEIGHT_PX = 4;
const STROKE_PX = 1;
const ROWS_OF_TILES = 2;
const IOD_BLOCK_COUNT = 4;
// The hatch's stripes: one every 5 px, each rising 10 px across the strip's 10 px height.
export const HATCH_STEP_PX = 5;
const HATCH_RISE_PX = 10;

// Where drawDie puts everything in a body w × h design pixels, for CCDs of coreCounts cores each. Rects are
// { x, y, w, h }; text positions are baselines.
export function dieLayout(w, h, coreCounts) {
  const small = w < SMALL_BELOW_PX;
  const { fs, pad, gap, titleDrop, titleBaseline } = small ? SMALL : WIDE;
  const top = pad + titleDrop;
  const innerW = w - 2 * pad - 2 * INSET;
  const x0 = pad + INSET;
  const ccdH = (h - top - pad - INSET) * CCD_SHARE;
  const iodY = top + ccdH + gap * IOD_GAP_SCALE;
  const iodH = h - pad - INSET - iodY;
  const count = Math.max(1, coreCounts.length);
  const ccdW = (innerW - (count - 1) * gap) / count;
  const ccds = coreCounts.map((cores, k) => {
    const x = x0 + k * (ccdW + gap);
    const gy = top + fs + CONTENT_TOP_PX;
    const rowH = (ccdH - fs - CONTENT_TRIM_PX - L3_BAND_PX) / ROWS_OF_TILES;
    const l3y = gy + rowH + L3_GAP_PX;
    const perRow = Math.max(1, Math.ceil(cores / ROWS_OF_TILES));
    const tileW = (ccdW - 2 * CONTENT_SIDE_PX - (perRow - 1) * TILE_GAP_PX) / perRow;
    const barW = Math.max(TILE_BAR_MIN_PX, (tileW - TILE_BAR_TRIM_PX) / 2);
    const tiles = Array.from({ length: cores }, (_, core) => {
      const tx = x + CONTENT_SIDE_PX + (core % perRow) * (tileW + TILE_GAP_PX);
      const ty = core < perRow ? gy : l3y + L3_AFTER_PX;
      return {
        x: tx, y: ty, w: tileW, h: rowH,
        labelX: tx + TILE_TEXT_PX,
        labelBaseline: ty + fs,
        graph: { x: tx + TILE_TEXT_PX, y: ty + fs + TILE_TEXT_PX, w: 2 * barW + TILE_TEXT_PX, h: Math.max(0, rowH - fs - TILE_GRAPH_TRIM_PX) },
      };
    });
    return {
      x, y: top, w: ccdW, h: ccdH,
      headX: x + CONTENT_SIDE_PX,
      headRight: x + ccdW - HEAD_RIGHT_PX,
      headBaseline: top + fs + HEAD_BASELINE_PX,
      l3: { x: x + CONTENT_SIDE_PX, y: l3y, w: ccdW - 2 * CONTENT_SIDE_PX, h: L3_HEIGHT_PX },
      l3Baseline: l3y + L3_BASELINE_PX,
      tiles,
      link: { x: x + ccdW / 2, y0: top + ccdH, y1: iodY },
    };
  });
  const blockW = (innerW - 2 * CONTENT_SIDE_PX - (IOD_BLOCK_COUNT - 1) * BLOCK_GAP_PX) / IOD_BLOCK_COUNT;
  const blockY = iodY + fs + CONTENT_TOP_PX;
  const blockH = iodH - fs - CONTENT_TRIM_PX;
  const blocks = Array.from({ length: IOD_BLOCK_COUNT }, (_, i) => {
    const bx = x0 + CONTENT_SIDE_PX + i * (blockW + BLOCK_GAP_PX);
    const meterY = blockY + blockH - METER_FROM_BOTTOM_PX;
    return {
      x: bx, y: blockY, w: blockW, h: blockH,
      textX: bx + BLOCK_TEXT_PX,
      nameBaseline: blockY + fs + BLOCK_NAME_DROP_PX,
      valueBaseline: blockY + 2 * fs + BLOCK_VALUE_DROP_PX,
      showsValue: blockH > 2 * fs + BLOCK_VALUE_ROOM_PX,
      meter: { x: bx + BLOCK_TEXT_PX, y: meterY, w: blockW - 2 * BLOCK_TEXT_PX, h: METER_HEIGHT_PX },
      // A block shorter than its meter's offset would put the meter above the block, over the I/O die's head.
      showsMeter: meterY >= blockY,
    };
  });
  // A block whose name would cross its bottom stroke is too short to draw; its name moves to the I/O die's head row.
  const showsBlocks = blockH >= fs + BLOCK_NAME_DROP_PX + STROKE_PX;
  return {
    small, fs,
    showsBlocks,
    package: { x: pad, y: pad, w: w - 2 * pad, h: h - 2 * pad },
    titleX: x0,
    titleRight: w - pad - INSET,
    titleBaseline: pad + titleBaseline,
    ccds,
    linkLabelOffset: LINK_LABEL_PX,
    linkLabelDrop: LINK_LABEL_DROP_PX,
    iod: { x: x0, y: iodY, w: innerW, h: iodH },
    iodHeadX: x0 + CONTENT_SIDE_PX,
    iodBaseline: iodY + fs + HEAD_BASELINE_PX,
    blocks,
  };
}

function point(x, y, scale) {
  return x * scale + ' ' + y * scale;
}

// An SVG path of a rect with its top-right and bottom-left corners cut by `cut`, as the mockup's chamfer(), its
// edges inset by half the 1 px stroke so that the stroke lies inside the rect; scale turns design into logical px.
export function chamferPath(rect, cut, scale) {
  const half = 0.5;
  const x = rect.x + half;
  const y = rect.y + half;
  const right = rect.x + rect.w - half;
  const bottom = rect.y + rect.h - half;
  return 'M ' + point(x, y, scale) + ' L ' + point(right - cut, y, scale) + ' L ' + point(right, y + cut, scale)
    + ' L ' + point(right, bottom, scale) + ' L ' + point(x + cut, bottom, scale) + ' L ' + point(x, bottom - cut, scale) + ' Z';
}

// The V-Cache strip's diagonal stripes across a strip `width` wide, in its own coordinates; the strip clips them.
export function hatchPath(width, height, scale) {
  const parts = [];
  for (let i = -HATCH_RISE_PX; i < width; i += HATCH_STEP_PX) {
    parts.push('M ' + point(i, height, scale) + ' L ' + point(i + HATCH_RISE_PX, height - HATCH_RISE_PX, scale));
  }
  return parts.join(' ');
}
