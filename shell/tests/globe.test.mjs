import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  ARC_LIFT_BASE,
  ARC_LIFT_SPAN,
  ARC_SEGMENTS,
  END,
  HOME,
  LATTICE_POINTS,
  MAX_ENDPOINTS,
  PACKET,
  PACKET_PER_MS,
  PACKET_STAGGER,
  PULSE_FROM,
  PULSE_GROW,
  PULSE_LINE,
  PULSE_MS,
  RING_INSET,
  RING_WIDTH,
  ROT_BASE,
  ROT_PER_MS,
  SEA_DOT_SCALE,
  TILT,
  arcAltitude,
  fibonacciPoint,
  globeLayout,
  labelAnchors,
  landBit,
  nearestIndex,
  slerp,
  unitVector,
  view
} from '../kit/GhostKit/lib/globe.mjs';
import { LAND_BIT_POINTS, LAND_WORDS } from '../kit/GhostKit/lib/landbits.mjs';

const GLOBE_FRAG = new URL('../kit/GhostKit/shaders/globe.frag', import.meta.url);
const SAO_PAULO = Object.freeze({ lat: -23.55, lon: -46.63 });
// The reference mockup's six PEERS, which the fake daemon publishes.
const SIX_ENDPOINTS = Object.freeze([
  { lat: 39.04, lon: -77.49 },
  { lat: 59.33, lon: 18.07 },
  { lat: 47.61, lon: -122.33 },
  { lat: 50.11, lon: 8.68 },
  { lat: 37.77, lon: -122.42 },
  { lat: -22.9, lon: -43.2 }
]);
const SHADER_CONSTANTS = Object.freeze([
  ['ARC_SEGMENTS', ARC_SEGMENTS],
  ['ARC_LIFT_BASE', ARC_LIFT_BASE],
  ['ARC_LIFT_SPAN', ARC_LIFT_SPAN],
  ['PACKET_PER_MS', PACKET_PER_MS],
  ['PACKET_STAGGER', PACKET_STAGGER],
  ['PACKET', PACKET],
  ['END', END],
  ['HOME', HOME],
  ['PULSE_MS', PULSE_MS],
  ['PULSE_FROM', PULSE_FROM],
  ['PULSE_GROW', PULSE_GROW],
  ['PULSE_LINE', PULSE_LINE],
  ['RING_INSET', RING_INSET],
  ['RING_WIDTH', RING_WIDTH],
  ['SEA_DOT_SCALE', SEA_DOT_SCALE],
  ['MAX_ENDPOINTS', MAX_ENDPOINTS]
]);
const RANDOM_DIRECTIONS = 2000;
const POLE_DIRECTIONS = 200;
const POLE_CAP_DEG = 2;
const NEARNESS_SLACK = 1e-12;
const SEED = 0x6c0be;
const D2R = Math.PI / 180;
const BITS_PER_COMPONENT = 24;

function FRAME(width, height, unit) {
  return { width, height, unit, elapsedMs: 0, hasHome: true, home: SAO_PAULO, endpoints: [] };
}

// -0 and 0 differ to deepEqual.
function round9(x) {
  return Math.round(x * 1e9) / 1e9 || 0;
}

function norm(v) {
  return Math.hypot(v[0], v[1], v[2]);
}

function distance(a, b) {
  return Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
}

// mulberry32: a seeded generator, so a failure names the same directions every run.
function generator(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const LATTICE = Array.from({ length: LATTICE_POINTS }, (_, i) => unitVector(fibonacciPoint(i, LATTICE_POINTS)));

function bruteNearest(v) {
  let best = Infinity;
  for (const point of LATTICE) {
    best = Math.min(best, distance(point, v));
  }
  return best;
}

// Directions whose angle from the y axis has its cosine uniform between `cosFrom` and 1, mirrored to the south pole
// when `south` is set.
function directions(count, random, cosFrom, south) {
  return Array.from({ length: count }, () => {
    const y = cosFrom + (1 - cosFrom) * random();
    const r = Math.sqrt(1 - y * y);
    const phi = 2 * Math.PI * random();
    return [r * Math.sin(phi), south ? -y : y, r * Math.cos(phi)];
  });
}

function disagreementsIn(vs) {
  return vs.filter((v) => distance(LATTICE[nearestIndex(v)], v) > bruteNearest(v) + NEARNESS_SLACK).length;
}

function landNearest(lat, lon) {
  return landBit(nearestIndex(unitVector({ lat, lon })));
}

// The elapsed time at which São Paulo faces directly away, at the rotation's first such turn after ROT_BASE.
function homeBehindMs() {
  const v = unitVector(SAO_PAULO);
  const facing = Math.PI - Math.atan2(v[0], v[2]);
  const turns = Math.ceil((ROT_BASE - facing) / (2 * Math.PI));
  return (facing + turns * 2 * Math.PI - ROT_BASE) / ROT_PER_MS;
}
const HOME_BEHIND_MS = homeBehindMs();

function shaderConstant(source, name) {
  const match = source.match(new RegExp(`const\\s+(?:float|int)\\s+${name}\\s*=\\s*([^;]+);`));
  return match ? Number(match[1]) : undefined;
}

test('lattice ends at the poles', () => {
  assert.equal(fibonacciPoint(0, 3200).lat, 90);
  assert.equal(fibonacciPoint(3199, 3200).lat, -90);
});

test('view faces latitude 0 longitude 0 to the viewer with no rotation', () => {
  assert.deepEqual(view(unitVector({ lat: 0, lon: 0 }), 0, 0).map(round9), [0, 0, 1]);
});

test('slerp São Paulo–London', () => {
  const { point, omega } = slerp(unitVector({ lat: -23.55, lon: -46.63 }), unitVector({ lat: 51.5, lon: -0.12 }), 0.5);
  assert.ok(Math.abs(norm(point) - 1) < 1e-9);
  assert.ok(omega > 1.4 && omega < 1.6);
});

test('arc altitude', () => {
  assert.ok(Math.abs(arcAltitude(Math.PI, 20, 40) - 1.3) < 1e-9);
  assert.equal(arcAltitude(1, 0, 40), 1);
});

test('layout sizes', () => {
  const big = globeLayout({ width: 400, height: 300, unit: 1 });
  assert.deepEqual([big.small, big.radius], [false, 120]);
  const small = globeLayout({ width: 200, height: 200, unit: 1 });
  assert.deepEqual([small.small, small.radius], [true, 90]);
  assert.equal(small.dot, Math.max(1.3, 90 / 70));
});

test('the Keinert lookup agrees with brute force on random directions', () => {
  const disagreements = disagreementsIn(directions(RANDOM_DIRECTIONS, generator(SEED), -1, false));
  assert.equal(disagreements, 0);
});

test('the Keinert lookup agrees with brute force within 2 degrees of both poles', () => {
  const random = generator(SEED + 1);
  const cap = Math.cos(POLE_CAP_DEG * D2R);
  const north = directions(POLE_DIRECTIONS, random, cap, false);
  const south = directions(POLE_DIRECTIONS, random, cap, true);
  const disagreements = disagreementsIn([...north, ...south]);
  assert.equal(disagreements, 0);
});

test('the Keinert lookup names the pole points at the poles', () => {
  assert.equal(nearestIndex([0, 1, 0]), 0);
  assert.equal(nearestIndex([0, -1, 0]), 3199);
});

test('land bits: the dot nearest São Paulo is land and the one south-east of it is sea', () => {
  assert.equal(landNearest(-23.55, -46.63), true);
  assert.equal(landNearest(-25.0, -44.5), false);
});

test('land bits: London and Australia are land', () => {
  assert.equal(landNearest(51.5, -0.12), true);
  assert.equal(landNearest(-25.0, 135.0), true);
});

test('land bits: the mid-Atlantic and the mid-North-Atlantic are sea', () => {
  assert.equal(landNearest(0.0, -30.0), false);
  assert.equal(landNearest(35.0, -40.0), false);
});

test('land words are 34 words of 4 integers below 2^24 and no bit past the last dot', () => {
  assert.equal(LAND_BIT_POINTS, LATTICE_POINTS);
  assert.equal(LAND_WORDS.length, 34);
  assert.ok(LAND_WORDS.flat().every((c) => Number.isInteger(c) && c >= 0 && c < 2 ** 24));
  const c = LAND_WORDS.flat();
  assert.equal(c[133] >> (3200 - 24 * 133), 0);
  assert.deepEqual(c.slice(134), [0, 0]);
});

test('landBit reads bit k of component c as dot 24c + k', () => {
  const components = LAND_WORDS.flat();
  for (let i = 0; i < LATTICE_POINTS; i += 1) {
    const component = components[Math.floor(i / BITS_PER_COMPONENT)];
    assert.equal(landBit(i), Math.floor(component / 2 ** (i % BITS_PER_COMPONENT)) % 2 === 1, `dot ${i}`);
  }
});

test('a home behind the globe has a hidden anchor', () => {
  const rot = ROT_BASE + HOME_BEHIND_MS * ROT_PER_MS;
  assert.ok(view(unitVector(SAO_PAULO), rot, TILT)[2] < 0);
  const anchors = labelAnchors({ ...FRAME(400, 400, 1), elapsedMs: HOME_BEHIND_MS });
  assert.equal(anchors.at(-1).visible, false);
});

test('a home in front of the globe has a visible anchor at its projection', () => {
  const frame = FRAME(400, 400, 1);
  const { radius } = globeLayout(frame);
  const q = view(unitVector(SAO_PAULO), ROT_BASE, TILT);
  const anchors = labelAnchors(frame);
  assert.ok(q[2] > 0);
  assert.deepEqual(anchors.at(-1), { x: 200 + radius * q[0], y: 200 - radius * q[1], visible: true });
});

test('label anchors are one per endpoint, then home', () => {
  const anchors = labelAnchors({ ...FRAME(400, 400, 1), endpoints: SIX_ENDPOINTS });
  assert.equal(anchors.length, 7);
});

test('label anchors stop at MAX_ENDPOINTS endpoints', () => {
  const endpoints = [...SIX_ENDPOINTS, ...SIX_ENDPOINTS];
  const anchors = labelAnchors({ ...FRAME(400, 400, 1), endpoints });
  assert.equal(anchors.length, MAX_ENDPOINTS + 1);
});

test('without a home the home anchor is hidden', () => {
  const anchors = labelAnchors({ ...FRAME(400, 400, 1), hasHome: false, endpoints: SIX_ENDPOINTS });
  assert.equal(anchors.length, 7);
  assert.equal(anchors.at(-1).visible, false);
});

test('globe.frag names the mockup constants as globe.mjs does', () => {
  const source = readFileSync(GLOBE_FRAG, 'utf8');
  for (const [name, value] of SHADER_CONSTANTS) {
    assert.equal(shaderConstant(source, name), value, name);
  }
});
