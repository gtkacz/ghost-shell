import { COMPONENTS_PER_WORD, pack } from './blockgraph.mjs';
import { heatAlphaByte, level } from './segments.mjs';

// A band of HeatGraph's rows as heatband.frag unpacks it, in BlockGraph's word layout (lib/blockgraph.mjs): a byte per
// cell, three to a component. An ok cell's byte is its heatAlphaByte(), which lies in 18-222, so it never collides
// with the three codes. The columns sit in a ring, each column's rows together, and `head` is the slot of the oldest
// column drawn: a new sample overwrites that slot and moves the head on, so only the one word a column spans is sent
// again, rather than every word of the band. A band holds HEAT_BAND_ROWS rows because a program with more words cost
// NVIDIA's driver about 20 MiB more here (128 words against 256 or 512).
const CELL_RANGE = 256;
const CELLS_PER_COMPONENT = 3;
const CELLS_PER_WORD = CELLS_PER_COMPONENT * COMPONENTS_PER_WORD;
export const HEAT_WORDS = 128;
export const HEAT_MAX_COLUMNS = 384;
export const HEAT_BAND_ROWS = (HEAT_WORDS * CELLS_PER_WORD) / HEAT_MAX_COLUMNS;
export const CELL_MISSING = 0;
export const CELL_WARN = 254;
export const CELL_CRIT = 255;

// `graph` holds HeatGraph's lo, hi, warnAt, critAt and live. A missing sample is checked first, because a fraction of
// null reads as 0.
export function heatCellByte(sample, graph) {
  if (!graph.live || !Number.isFinite(sample)) {
    return CELL_MISSING;
  }
  const fraction = (sample - graph.lo) / (graph.hi - graph.lo);
  const cellLevel = level(fraction, graph.warnAt, graph.critAt);
  if (cellLevel === 'crit') {
    return CELL_CRIT;
  }
  if (cellLevel === 'warn') {
    return CELL_WARN;
  }
  return Number.isFinite(fraction) ? heatAlphaByte(fraction) : CELL_MISSING;
}

// The rows share the height, gaps between them (the mockup's heat()).
export function heatRowHeight(height, rows, gapSize) {
  return (height - (rows - 1) * gapSize) / rows;
}

// The newest columns that are drawn: past HEAT_MAX_COLUMNS the oldest stay unlit.
export function heatDrawnColumns(columns) {
  return Math.max(0, Math.min(columns, HEAT_MAX_COLUMNS));
}

// A series shorter than the graph fills it from the right.
function sampleAt(samples, columns, column) {
  const index = (samples || []).length - columns + column;
  return index >= 0 ? samples[index] : undefined;
}

// The cells of one column, top row first.
function columnCells(series, columns, column, graph) {
  return series.map(samples => heatCellByte(sampleAt(samples, columns, column), graph));
}

function packWord(cells, word) {
  return pack(cells.slice(word * CELLS_PER_WORD, (word + 1) * CELLS_PER_WORD), 1, CELLS_PER_COMPONENT, CELL_RANGE)[0];
}

// `series` holds one series per row of the band, top row first, at most HEAT_BAND_ROWS of them, for a graph `columns`
// wide; the ring starts with its head at slot 0. The caller keeps the ring and moves it on with heatRingAdvance().
export function heatRing(series, columns, graph) {
  const drawn = heatDrawnColumns(columns);
  const rows = series.length;
  // Slot s holds column columns - drawn + s, its rows together.
  const cells = Array.from({ length: drawn * rows }, (_, index) => {
    const slot = Math.floor(index / rows);
    return heatCellByte(sampleAt(series[index - slot * rows], columns, columns - drawn + slot), graph);
  });
  return {
    series,
    columns,
    rows,
    drawn,
    head: 0,
    graph: { lo: graph.lo, hi: graph.hi, warnAt: graph.warnAt, critAt: graph.critAt, live: graph.live },
    cells,
    words: pack(cells, HEAT_WORDS, CELLS_PER_COMPONENT, CELL_RANGE),
  };
}

function sameSettings(ring, rows, columns, graph) {
  const kept = ring.graph;
  return ring.rows === rows && ring.columns === columns && kept.lo === graph.lo && kept.hi === graph.hi
    && kept.warnAt === graph.warnAt && kept.critAt === graph.critAt && kept.live === graph.live;
}

// Whether every drawn column but the newest shows what the column to its right showed before. Indexes the arrays
// directly, as it runs over every drawn cell on every sample.
function movedOnByOne(ring, series) {
  for (let row = 0; row < ring.rows; row += 1) {
    const next = series[row] || [];
    const previous = ring.series[row] || [];
    // The column that now reads next[at] read previous[at + shift] before.
    const shift = previous.length - next.length + 1;
    for (let at = next.length - ring.drawn; at < next.length - 1; at += 1) {
      const now = at >= 0 ? next[at] : undefined;
      const before = at + shift >= 0 ? previous[at + shift] : undefined;
      if (now !== before && (Number.isFinite(now) || Number.isFinite(before))) {
        return false;
      }
    }
  }
  return true;
}

// The ring that draws `series` after `ring`, and the indices of its words that differ from `ring`'s: none when nothing
// changed, the words of one column when every series moved on by one sample. Anything else, such as a gap of several
// samples, a series replaced wholesale, a resize or new settings, returns null, and the caller packs a new heatRing().
export function heatRingAdvance(ring, series, columns, graph) {
  if (!sameSettings(ring, series.length, columns, graph)) {
    return null;
  }
  if (series.every((samples, row) => samples === ring.series[row])) {
    return { ring, changed: [] };
  }
  if (ring.drawn === 0 || !movedOnByOne(ring, series)) {
    return null;
  }
  // The newest column takes the oldest one's slot.
  const slot = ring.head;
  const start = slot * ring.rows;
  const newest = columnCells(series, columns, columns - 1, graph);
  const cells = ring.cells.slice(0, start).concat(newest, ring.cells.slice(start + ring.rows));
  const first = Math.floor(start / CELLS_PER_WORD);
  const last = Math.floor((start + ring.rows - 1) / CELLS_PER_WORD);
  const changed = [];
  for (let word = first; word <= last; word += 1) {
    changed.push(word);
  }
  return {
    ring: {
      series,
      columns: ring.columns,
      rows: ring.rows,
      drawn: ring.drawn,
      head: (slot + 1) % ring.drawn,
      graph: ring.graph,
      cells,
      words: ring.words.map((word, index) => (index >= first && index <= last ? packWord(cells, index) : word)),
    },
    changed,
  };
}
