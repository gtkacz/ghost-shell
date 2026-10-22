import test from 'node:test';
import assert from 'node:assert/strict';
import {
  buildTree, layoutRadial, dotSize, dotLevel, showsLabel, labelText, rootLabel, hitTest, pruneCollapsed, toggled, key,
} from '../../widgets/process-tree/tree.mjs';

const P = (pid, ppid, name, cpu = 0, rssKib = 0) => ({ pid, ppid, name, cpu, rss_kib: rssKib });
const node = (built, wanted) => built.nodes.get(wanted);
const deepest = (built) => [...built.nodes.values()].reduce((a, b) => (b.depth > a.depth ? b : a));

// systemd → user systemd → bash → pactl, bash; user systemd → pipewire: four below the user manager.
const TREE_3_DEEP = {
  kernel_threads: 212,
  procs: [
    P(1, 0, 'systemd'), P(2081, 1, 'systemd'), P(2118, 2081, 'bash'), P(2124, 2118, 'pactl'), P(2125, 2118, 'bash'),
    P(2129, 2081, 'pipewire'),
  ],
};
const NODES = layoutRadial(buildTree(TREE_3_DEEP, new Set()), 400, 400).nodes;

const SYNTHETIC_NAMES = ['thorium', 'bash', 'zsh', 'kworker', 'Discord', 'spotify', 'python', 'node', 'slack', 'konsole'];
// Each process hangs from one a third of the way down the list, so the tree is a few levels deep and bushy.
const SYNTHETIC_1000 = {
  kernel_threads: 436,
  procs: Array.from({ length: 1000 }, (_, i) => P(i + 1, i === 0 ? 0 : Math.floor(i / 3) + 1, SYNTHETIC_NAMES[i % SYNTHETIC_NAMES.length], i % 37, i * 997)),
};

test('kernel threads collapse into one child of the root', () => { const b = buildTree({ kernel_threads: 212, procs: [P(1, 0, 'systemd')] }, new Set()); assert.equal(b.root.kids.find((k) => k.key === 'kthreads').name, 'kthreadd (212)'); });
test('an orphan attaches to the root', () => { const b = buildTree({ kernel_threads: 0, procs: [P(1, 0, 'systemd'), P(9, 4242, 'lost')] }, new Set()); assert.ok(b.root.kids.some((k) => k.pid === 9)); });
test('a collapsed node hides and counts its descendants', () => { const b = buildTree(TREE_3_DEEP, new Set(['2081:systemd'])); assert.equal(node(b, '2081:systemd').hidden, 4); assert.equal(b.nodes.has('2118:bash'), false); });
test('collapse state drops a vanished pid and survives a rebuild', () => { const s = pruneCollapsed(new Set(['2081:systemd', '777:gone']), buildTree(TREE_3_DEEP, new Set())); assert.deepEqual([...s], ['2081:systemd']); });
test('a recycled pid is a different node', () => assert.notEqual(key(P(7, 1, 'a')), key(P(7, 1, 'b'))));
test('radial layout puts the root at the centre and leaves on the rim', () => { const b = layoutRadial(buildTree(TREE_3_DEEP, new Set()), 400, 400); assert.deepEqual([b.root.x, b.root.y], [200, 200]); assert.equal(Math.round(deepest(b).radius), 160); });
test('dot size and level', () => { assert.equal(dotSize(0), 1.6); assert.equal(dotLevel(21), 'crit'); assert.equal(dotLevel(5), 'hi'); assert.equal(dotLevel(0.5), 'ok'); });
test('a small tree labels only busy nodes', () => { assert.equal(showsLabel({ cpu: 9, depth: 3, leaves: 1 }, 100), true); assert.equal(showsLabel({ cpu: 5, depth: 1, leaves: 9 }, 100), false); });
test('a thousand processes lay out once in under 20 ms (user-confirmed 2026-10-21)', () => { const t0 = performance.now(); layoutRadial(buildTree(SYNTHETIC_1000, new Set()), 600, 500); assert.ok(performance.now() - t0 < 20); });
test('hit test picks the nearest node within the slop', () => { assert.equal(hitTest(NODES, 203, 201, 6), '1:systemd'); assert.equal(hitTest(NODES, 50, 50, 6), null); });

test('a thousand processes all reach the tree once, below one root', () => {
  const b = buildTree(SYNTHETIC_1000, new Set());
  // Every process plus the kernel threads' node.
  assert.equal(b.nodes.size, 1001);
  assert.equal(b.root.key, '1:thorium');
  assert.equal(b.root.leaves, [...b.nodes.values()].filter((n) => n.kids.length === 0).length);
});

test('a missing pid 1 hangs everything from a synthetic root', () => {
  const b = buildTree({ kernel_threads: 3, procs: [P(40, 1, 'a'), P(41, 40, 'b'), P(42, 0, 'c')] }, new Set());
  assert.equal(b.root.key, '?');
  assert.deepEqual(b.root.kids.map((k) => k.key), ['40:a', '42:c', 'kthreads']);
  assert.equal(node(b, '41:b').depth, 2);
});

test('a parent cycle from pid reuse between reads never hangs the build and stays off the tree', () => {
  const b = buildTree({ kernel_threads: 0, procs: [P(1, 0, 'systemd'), P(50, 60, 'x'), P(60, 50, 'y'), P(70, 1, 'z')] }, new Set());
  assert.deepEqual([...b.nodes.keys()].sort(), ['1:systemd', '70:z', 'kthreads']);
});

test('a pid reused under a new name loses the old name\'s collapse', () => {
  const before = buildTree({ kernel_threads: 0, procs: [P(1, 0, 'systemd'), P(7, 1, 'a'), P(8, 7, 'c')] }, new Set(['7:a']));
  assert.equal(node(before, '7:a').hidden, 1);
  const after = buildTree({ kernel_threads: 0, procs: [P(1, 0, 'systemd'), P(7, 1, 'b'), P(8, 7, 'c')] }, new Set(['7:a']));
  assert.equal(node(after, '7:b').hidden, undefined);
  assert.ok(after.nodes.has('8:c'));
  assert.deepEqual([...pruneCollapsed(new Set(['7:a']), after)], []);
});

test('a collapse inside a collapsed node survives the prune', () => {
  const b = buildTree(TREE_3_DEEP, new Set(['2081:systemd', '2118:bash']));
  assert.deepEqual([...pruneCollapsed(new Set(['2081:systemd', '2118:bash']), b)], ['2081:systemd', '2118:bash']);
});

test('children are sorted by name, ties by pid', () => {
  const b = buildTree({ kernel_threads: 0, procs: [P(1, 0, 'systemd'), P(9, 1, 'zsh'), P(5, 1, 'bash'), P(3, 1, 'bash')] }, new Set());
  assert.deepEqual(b.root.kids.map((k) => k.key), ['3:bash', '5:bash', 'kthreads', '9:zsh']);
});

test('leaf-weighted spans: a node sits mid-span and the root spans the full turn from the top', () => {
  const b = layoutRadial(buildTree(TREE_3_DEEP, new Set()), 400, 400);
  // Leaves: kthreads, pactl, bash(2125), pipewire. The root's span starts at -π/2.
  assert.ok(Math.abs(node(b, 'kthreads').angle - (-Math.PI / 2 + Math.PI / 4)) < 1e-9);
  assert.ok(Math.abs(node(b, '2081:systemd').angle - (-Math.PI / 2 + 2 * Math.PI * (1 / 4 + 3 / 8))) < 1e-9);
  const n = node(b, '2124:pactl');
  assert.ok(Math.abs(n.x - (200 + Math.cos(n.angle) * n.radius)) < 1e-9 && Math.abs(n.y - (200 + Math.sin(n.angle) * n.radius)) < 1e-9);
});

test('a small panel keeps a narrower margin', () => {
  const b = layoutRadial(buildTree(TREE_3_DEEP, new Set()), 280, 400);
  assert.equal(b.R, 280 / 2 - 14);
  assert.equal(Math.round(deepest(b).radius), 126);
});

test('the levels follow the mockup\'s cpu steps', () => { assert.equal(dotLevel(20), 'warn'); assert.equal(dotLevel(5.1), 'warn'); assert.equal(dotLevel(1), 'ok'); assert.equal(dotLevel(1.1), 'hi'); });
test('dot size grows with the log of resident memory', () => assert.ok(Math.abs(dotSize(999 * 1024) - (1.6 + 3 * 1.1)) < 1e-9));

test('a large tree labels top-level branches, busy nodes and big second-level branches', () => {
  assert.equal(showsLabel({ cpu: 0, depth: 1, leaves: 3 }, 160), true);
  assert.equal(showsLabel({ cpu: 0, depth: 1, leaves: 2 }, 160), false);
  assert.equal(showsLabel({ cpu: 4.1, depth: 4, leaves: 1 }, 160), true);
  assert.equal(showsLabel({ cpu: 0, depth: 2, leaves: 12 }, 160), true);
  assert.equal(showsLabel({ cpu: 0, depth: 2, leaves: 11 }, 160), false);
});

test('a collapsed node always shows its label, with its hidden count', () => {
  const collapsed = { name: 'systemd', cpu: 0, depth: 1, leaves: 1, hidden: 4 };
  assert.equal(showsLabel(collapsed, 100), true);
  assert.equal(labelText(collapsed), ' systemd +4 ');
});

test('a label carries the cpu above one percent', () => { assert.equal(labelText({ name: 'thorium', cpu: 85.1 }), ' thorium 85% '); assert.equal(labelText({ name: 'bash', cpu: 1 }), ' bash '); });

test('the root label names the root and its pid, and its hidden count once collapsed', () => {
  assert.equal(rootLabel({ key: '1:systemd', pid: 1, name: 'systemd' }), 'SYSTEMD · PID 1');
  assert.equal(rootLabel({ key: '1:systemd', pid: 1, name: 'systemd', hidden: 12 }), 'SYSTEMD · PID 1 +12');
  assert.equal(rootLabel({ key: '?', pid: null, name: '?' }), '?');
});

test('a click collapses an expandable node, a second expands it, and a leaf stays as it is', () => {
  const open = buildTree(TREE_3_DEEP, new Set());
  const once = toggled(new Set(), node(open, '2081:systemd'));
  assert.deepEqual([...once], ['2081:systemd']);
  const shut = buildTree(TREE_3_DEEP, once);
  assert.deepEqual([...toggled(once, node(shut, '2081:systemd'))], []);
  assert.deepEqual([...toggled(once, node(open, '2124:pactl'))], ['2081:systemd']);
  assert.notEqual(toggled(once, node(open, '2124:pactl')), once, 'a new set every time');
});

test('the hit test prefers the nearer of two nodes and counts a big dot\'s own size', () => {
  const nodes = new Map([['a', { x: 0, y: 0, rss_kib: 0 }], ['b', { x: 4, y: 0, rss_kib: 0 }], ['big', { x: 100, y: 0, rss_kib: 999999 * 1024 }]]);
  assert.equal(hitTest(nodes, 3, 0, 6), 'b');
  // log10(1 + 999999) is 6, so the big dot is 1.6 + 6 · 1.1 = 8.2 px.
  assert.equal(hitTest(nodes, 108, 0, 6), 'big');
  assert.equal(hitTest(nodes, 109, 0, 6), null);
});
