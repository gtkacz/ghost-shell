// The process tree's logic, apart from QML so that node can test it. Built from the reference mockup's buildTree and
// drawTree. A tree is the daemon's procs.tree: { kernel_threads, procs: [{ pid, ppid, name, cpu, rss_kib }] }, cpu a
// percentage of the whole machine. Lengths are design pixels.

const ROOT_PID = 1;
const SYNTHETIC_ROOT_KEY = '?';
const KERNEL_THREADS_KEY = 'kthreads';
const KERNEL_THREADS_NAME = 'kthreadd';

const START_ANGLE = -Math.PI / 2;
const FULL_TURN = 2 * Math.PI;
// A panel narrower than this keeps a smaller margin round the rim.
const SMALL_PANEL_PX = 300;
const SMALL_MARGIN_PX = 14;
const MARGIN_PX = 40;
// Below 1 the rings crowd outwards, where the leaves are.
const RADIUS_EXPONENT = 0.75;

const DOT_BASE_PX = 1.6;
const DOT_PER_DECADE_PX = 1.1;
const KIB_PER_MIB = 1024;

const CRIT_CPU = 20;
const WARN_CPU = 5;
const HI_CPU = 1;
// With a radius under SMALL_RADIUS_PX only processes busier than SMALL_LABEL_CPU are named; a larger tree also names
// its top-level branches of LABELLED_BRANCH_LEAVES leaves or more and its second-level ones of LABELLED_TWIG_LEAVES.
const SMALL_RADIUS_PX = 120;
const SMALL_LABEL_CPU = 8;
const LABEL_CPU = 4;
const LABELLED_BRANCH_LEAVES = 3;
const LABELLED_TWIG_LEAVES = 12;
const BRANCH_DEPTH = 1;
const TWIG_DEPTH = 2;
const LABEL_CPU_ABOVE = 1;

// A pid the kernel hands out again starts another process, which usually has another name.
export function key(proc) {
  return proc.pid + ':' + proc.name;
}

function nodeOf(proc) {
  return { key: key(proc), pid: proc.pid, ppid: proc.ppid, name: proc.name, cpu: proc.cpu, rss_kib: proc.rss_kib, kids: [] };
}

function byNameThenPid(a, b) {
  return a.name.localeCompare(b.name) || (a.pid ?? 0) - (b.pid ?? 0);
}

function descendants(node) {
  return node.kids.reduce((count, kid) => count + 1 + descendants(kid), 0);
}

// Walks down from the root, so a process whose parents form a loop, which only a pid reused between two reads of one
// scan can make, is never reached and is left out.
function visit(node, depth, parent, collapsed, nodes) {
  node.depth = depth;
  node.parent = parent;
  nodes.set(node.key, node);
  if (collapsed.has(node.key) && node.kids.length > 0) {
    node.hidden = descendants(node);
    node.kids = [];
  }
  node.kids.sort(byNameThenPid);
  node.kids.forEach((kid) => visit(kid, depth + 1, node, collapsed, nodes));
  node.leaves = node.kids.length > 0 ? node.kids.reduce((sum, kid) => sum + kid.leaves, 0) : 1;
}

// The tree below pid 1, with every kernel thread folded into one child of the root. `nodes` holds the nodes on show, by
// key, root first; `present` holds the key of every process in the table, shown or hidden by a collapse.
export function buildTree(tree, collapsed) {
  const byPid = new Map(tree.procs.map((proc) => [proc.pid, nodeOf(proc)]));
  const root = byPid.get(ROOT_PID) ?? { key: SYNTHETIC_ROOT_KEY, pid: null, ppid: null, name: SYNTHETIC_ROOT_KEY, cpu: 0, rss_kib: 0, kids: [] };
  byPid.forEach((node) => {
    if (node === root) {
      return;
    }
    const parent = byPid.get(node.ppid);
    (parent && parent !== node ? parent : root).kids.push(node);
  });
  root.kids.push({ key: KERNEL_THREADS_KEY, pid: null, ppid: root.pid, name: KERNEL_THREADS_NAME + ' (' + tree.kernel_threads + ')', cpu: 0, rss_kib: 0, kids: [] });
  const nodes = new Map();
  visit(root, 0, null, collapsed, nodes);
  const present = new Set(Array.from(byPid.values(), (node) => node.key));
  present.add(KERNEL_THREADS_KEY);
  return { root, nodes, present };
}

function place(node, from, to) {
  node.angle = (from + to) / 2;
  let start = from;
  node.kids.forEach((kid) => {
    const span = (to - from) * kid.leaves / node.leaves;
    place(kid, start, start + span);
    start += span;
  });
}

// Gives every node its angle, its radius and its x and y in a w × h area: each node's share of the turn is its share of
// the leaves, and the deepest nodes sit on the rim.
export function layoutRadial(built, w, h) {
  const short = Math.min(w, h);
  const R = short / 2 - (short < SMALL_PANEL_PX ? SMALL_MARGIN_PX : MARGIN_PX);
  const cx = w / 2;
  const cy = h / 2;
  place(built.root, START_ANGLE, START_ANGLE + FULL_TURN);
  let maxDepth = 0;
  built.nodes.forEach((node) => {
    maxDepth = Math.max(maxDepth, node.depth);
  });
  built.nodes.forEach((node) => {
    node.radius = node.depth === 0 ? 0 : R * Math.pow(node.depth / maxDepth, RADIUS_EXPONENT);
    node.x = cx + Math.cos(node.angle) * node.radius;
    node.y = cy + Math.sin(node.angle) * node.radius;
  });
  return Object.assign(built, { R, cx, cy });
}

export function dotSize(rssKib) {
  return DOT_BASE_PX + Math.log10(1 + rssKib / KIB_PER_MIB) * DOT_PER_DECADE_PX;
}

export function dotLevel(cpu) {
  if (cpu > CRIT_CPU) {
    return 'crit';
  }
  if (cpu > WARN_CPU) {
    return 'warn';
  }
  if (cpu > HI_CPU) {
    return 'hi';
  }
  return 'ok';
}

export function showsLabel(node, R) {
  if (node.hidden !== undefined) {
    return true;
  }
  if (R < SMALL_RADIUS_PX) {
    return node.cpu > SMALL_LABEL_CPU;
  }
  return (node.depth === BRANCH_DEPTH && node.leaves >= LABELLED_BRANCH_LEAVES) || node.cpu > LABEL_CPU
    || (node.depth === TWIG_DEPTH && node.leaves >= LABELLED_TWIG_LEAVES);
}

// Padded with a space at each end, as the mockup draws it.
export function labelText(node) {
  if (node.hidden !== undefined) {
    return ' ' + node.name + ' +' + node.hidden + ' ';
  }
  return ' ' + node.name + (node.cpu > LABEL_CPU_ABOVE ? ' ' + node.cpu.toFixed(0) + '%' : '') + ' ';
}

export function rootLabel(root) {
  if (root.pid === null) {
    return SYNTHETIC_ROOT_KEY;
  }
  return root.name.toUpperCase() + ' · PID ' + root.pid + (root.hidden !== undefined ? ' +' + root.hidden : '');
}

// The key of the node nearest (x, y), if it is within its own dot or `slopPx` of it.
export function hitTest(nodes, x, y, slopPx) {
  let found = null;
  let nearest = Infinity;
  nodes.forEach((node, nodeKey) => {
    const distance = Math.hypot(node.x - x, node.y - y);
    if (distance <= Math.max(dotSize(node.rss_kib), slopPx) && distance < nearest) {
      found = nodeKey;
      nearest = distance;
    }
  });
  return found;
}

export function pruneCollapsed(collapsed, built) {
  return new Set([...collapsed].filter((nodeKey) => built.present.has(nodeKey)));
}

// A node without children has nothing to collapse.
export function toggled(collapsed, node) {
  const next = new Set(collapsed);
  if (next.has(node.key)) {
    next.delete(node.key);
  } else if (node.kids.length > 0) {
    next.add(node.key);
  }
  return next;
}
