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

function fillSlot(ring, slot, column, graph) {
  for (let row = 0; row < ring.rows; row += 1) {
    ring.cells[slot * ring.rows + row] = heatCellByte(sampleAt(ring.series[row], ring.columns, column), graph);
  }
}

// `series` holds one series per row of the band, top row first, at most HEAT_BAND_ROWS of them, for a graph `columns`
// wide; the ring starts with its head at slot 0. The caller keeps the ring and moves it on with heatRingAdvance().
export function heatRing(series, columns, graph) {
  const rows = series.length;
  const drawn = heatDrawnColumns(columns);
  const ring = {
    series,
    columns,
    rows,
    drawn,
    head: 0,
    graph: { lo: graph.lo, hi: graph.hi, warnAt: graph.warnAt, critAt: graph.critAt, live: graph.live },
    cells: Array(drawn * rows).fill(CELL_MISSING),
    words: [],
  };
  for (let slot = 0; slot < drawn; slot += 1) {
    fillSlot(ring, slot, columns - drawn + slot, graph);
  }
  ring.words = pack(ring.cells, HEAT_WORDS, CELLS_PER_COMPONENT, CELL_RANGE);
  return ring;
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

// Moves `ring` on to draw `series` and returns the indices of the words that changed: none when nothing did, the
// words of one column when every series moved on by one sample. Anything else, such as a gap of several samples, a
// series replaced wholesale, a resize or new settings, returns null, and the caller packs a new heatRing(). The ring
// changes in place: a new one per sample would cost the copying that the ring exists to avoid.
export function heatRingAdvance(ring, series, columns, graph) {
  if (!sameSettings(ring, series.length, columns, graph)) {
    return null;
  }
  if (series.every((samples, row) => samples === ring.series[row])) {
    return [];
  }
  if (ring.drawn === 0 || !movedOnByOne(ring, series)) {
    return null;
  }
  const slot = ring.head;
  ring.series = series;
  fillSlot(ring, slot, columns - 1, graph);
  ring.head = (slot + 1) % ring.drawn;
  const first = Math.floor((slot * ring.rows) / CELLS_PER_WORD);
  const last = Math.floor(((slot + 1) * ring.rows - 1) / CELLS_PER_WORD);
  const changed = [];
  for (let word = first; word <= last; word += 1) {
    ring.words[word] = pack(ring.cells.slice(word * CELLS_PER_WORD, (word + 1) * CELLS_PER_WORD), 1, CELLS_PER_COMPONENT, CELL_RANGE)[0];
    changed.push(word);
  }
  return changed;
}
