import { LAND_WORDS } from './landbits.mjs';

// The uplink globe's maths, as the reference mockup's drawGlobe() does it
// (docs/superpowers/specs/mockups/2026-10-20-ghost-shell-reference.html). globe.frag draws the picture; this is the
// node-tested half that projects the labels, and the place every constant is named. Lengths are design units, which
// Globe multiplies by Theme.u. The geometry constants are restated in globe.frag under the same names, and
// globe.test.mjs holds the two equal; the alphas reach the shader as uniforms.

// drawGlobe's PTS: y = 1 - 2i/(N - 1), longitude advancing by the golden angle.
export const LATTICE_POINTS = 3200;
// rot = .6 + t * .00009, tilt = .38.
export const ROT_BASE = 0.6;
export const ROT_PER_MS = 0.00009;
export const TILT = 0.38;
// small = min(w, h) < 240; R = min(w, h) / 2 - (small ? 10 : 30); dot = max(1.3, R / 70).
export const SMALL_BELOW = 240;
export const MARGIN_SMALL = 10;
export const MARGIN = 30;
export const DOT_MIN = 1.3;
export const DOT_PER_R = 1 / 70;
// A sea dot is a square of dot / 2.
export const SEA_DOT_SCALE = 0.5;
// Land at rgba(sig, .2 + .75 z), sea at rgba(sig, .07 z).
export const LAND_ALPHA_FLOOR = 0.2;
export const LAND_ALPHA_SPAN = 0.75;
export const SEA_ALPHA_SPAN = 0.07;
// The outline: rgba(sig, .2), lineWidth .8, at R + 1.
export const RING_ALPHA = 0.2;
export const RING_WIDTH = 0.8;
export const RING_INSET = 1;
// Arcs: 40 segments, alt = 1 + (.08 + .22 ω/π) sin(πi/40), rgba(hi, .5), lineWidth small ? .7 : 1.
export const ARC_SEGMENTS = 40;
export const ARC_LIFT_BASE = 0.08;
export const ARC_LIFT_SPAN = 0.22;
export const ARC_ALPHA = 0.5;
export const ARC_WIDTH = 1;
export const ARC_WIDTH_SMALL = 0.7;
// The packet sits at segment ⌊((t * .00025 + k * .37) % 1) * 40⌋.
export const PACKET_PER_MS = 0.00025;
export const PACKET_STAGGER = 0.37;
// Square sides: packet 3, end 4, home 5.
export const PACKET = 3;
export const END = 4;
export const HOME = 5;
// pulse = (t % 1800) / 1800; strokeRect side 6 + 16 pulse, lineWidth 1, at alpha 1 - pulse.
export const PULSE_MS = 1800;
export const PULSE_FROM = 6;
export const PULSE_GROW = 16;
export const PULSE_LINE = 1;
// The uplink feed's cap, which is also the shader's count of endpoint uniforms.
export const MAX_ENDPOINTS = 8;

const D2R = Math.PI / 180;
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));
const SQRT5 = Math.sqrt(5);
const PHI = (1 + SQRT5) / 2;
const GOLDEN = PHI - 1;
const LOG_PHI_SQUARED = Math.log(PHI * PHI);
// globe.frag splits an index as 64a + b, because 32-bit floats lose a third of a pixel in i * GOLDEN by i = 3199.
// Done here too, so the two lookups are the same arithmetic.
const SPLIT = 64;
const GOLDEN_SPLIT_FRAC = fract(SPLIT * GOLDEN);
// Keinert's level k needs 1 - z² above zero at the poles.
const POLE_FLOOR = 1e-12;
const CELL_CORNERS = 4;
const BITS_PER_COMPONENT = 24;
const COMPONENTS_PER_WORD = 4;

function fract(x) {
  return x - Math.floor(x);
}

function fracGolden(i) {
  const a = Math.floor(i / SPLIT);
  const b = i - a * SPLIT;
  return fract(a * GOLDEN_SPLIT_FRAC + b * GOLDEN);
}

function squaredDistance(a, b) {
  return (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2;
}

export function fibonacciPoint(i, n) {
  const y = 1 - (2 * i) / (n - 1);
  const theta = GOLDEN_ANGLE * i;
  return { lat: Math.asin(y) / D2R, lon: Math.atan2(Math.sin(theta), Math.cos(theta)) / D2R };
}

export function unitVector({ lat, lon }) {
  return [Math.cos(lat * D2R) * Math.sin(lon * D2R), Math.sin(lat * D2R), Math.cos(lat * D2R) * Math.cos(lon * D2R)];
}

export function view(v, rot, tilt) {
  const cr = Math.cos(rot);
  const sr = Math.sin(rot);
  const ct = Math.cos(tilt);
  const st = Math.sin(tilt);
  const x = v[0] * cr + v[2] * sr;
  const z = v[2] * cr - v[0] * sr;
  return [x, v[1] * ct - z * st, v[1] * st + z * ct];
}

export function rotation(elapsedMs) {
  return ROT_BASE + elapsedMs * ROT_PER_MS;
}

export function slerp(a, b, t) {
  const d = Math.min(1, Math.max(-1, a[0] * b[0] + a[1] * b[1] + a[2] * b[2]));
  const omega = Math.acos(d);
  // The mockup's guard: coincident ends would divide by zero.
  const s = Math.sin(omega) || 1e-6;
  const ka = Math.sin((1 - t) * omega) / s;
  const kb = Math.sin(t * omega) / s;
  return { point: [a[0] * ka + b[0] * kb, a[1] * ka + b[1] * kb, a[2] * ka + b[2] * kb], omega };
}

export function arcAltitude(omega, i, segments) {
  return 1 + (ARC_LIFT_BASE + (ARC_LIFT_SPAN * omega) / Math.PI) * Math.sin((Math.PI * i) / segments);
}

// Keinert et al. 2015, "Spherical Fibonacci Mapping", adapted to the mockup's lattice: its points include both poles
// and step z by 2 / (N - 1), and its longitude runs the other way. The four corners of the lattice cell around v miss
// the true nearest point within a dot of a pole, where the pole points sit, so the nearer pole is a fifth candidate.
export function nearestIndex(v) {
  const n = LATTICE_POINTS;
  const dz = 2 / (n - 1);
  const phi = Math.atan2(-v[0], v[2]);
  const z = v[1];
  const k = Math.max(2, Math.floor(Math.log(n * Math.PI * SQRT5 * Math.max(1 - z * z, POLE_FLOOR)) / LOG_PHI_SQUARED));
  const fk = PHI ** k / SQRT5;
  const f0 = Math.round(fk);
  const f1 = Math.round(fk * PHI);
  const b00 = 2 * Math.PI * (fracGolden(f0 + 1) - GOLDEN);
  const b01 = 2 * Math.PI * (fracGolden(f1 + 1) - GOLDEN);
  const b10 = -f0 * dz;
  const b11 = -f1 * dz;
  const det = b00 * b11 - b01 * b10;
  const cu = Math.floor((b11 * phi - b01 * (z - 1)) / det);
  const cv = Math.floor((b00 * (z - 1) - b10 * phi) / det);
  const candidates = [z > 0 ? 0 : n - 1];
  for (let corner = 0; corner < CELL_CORNERS; corner += 1) {
    const cornerZ = b10 * (cu + (corner % 2)) + b11 * (cv + Math.floor(corner / 2)) + 1;
    // A corner past a pole reflects back onto the sphere, as in the paper.
    const reflected = Math.min(1, Math.max(-1, cornerZ)) * 2 - cornerZ;
    candidates.push(Math.min(n - 1, Math.max(0, Math.round((1 - reflected) / dz))));
  }
  let nearest = candidates[0];
  let best = Infinity;
  for (const i of candidates) {
    const d = squaredDistance(unitVector(fibonacciPoint(i, n)), v);
    if (d < best) {
      best = d;
      nearest = i;
    }
  }
  return nearest;
}

export function landBit(i) {
  const component = Math.floor(i / BITS_PER_COMPONENT);
  const word = LAND_WORDS[Math.floor(component / COMPONENTS_PER_WORD)];
  return ((word[component % COMPONENTS_PER_WORD] >> (i % BITS_PER_COMPONENT)) & 1) === 1;
}

export function globeLayout({ width, height, unit }) {
  const side = Math.min(width, height);
  const small = side < SMALL_BELOW * unit;
  const radius = side / 2 - (small ? MARGIN_SMALL : MARGIN) * unit;
  return {
    small,
    radius,
    dot: Math.max(DOT_MIN * unit, radius * DOT_PER_R),
    arcWidth: (small ? ARC_WIDTH_SMALL : ARC_WIDTH) * unit
  };
}

// The mockup's rule: a point is drawn while it faces the viewer, or while it is outside the disc, as a raised arc is.
function visible(q) {
  return q[2] > 0 || Math.hypot(q[0], q[1]) > 1;
}

// Where the uplink widget's labels go: one anchor per endpoint, at most MAX_ENDPOINTS, then home, hidden when there
// is none.
// The same rotation, tilt and radius as globe.frag, so a label stays on its square.
export function labelAnchors({ width, height, unit, elapsedMs, hasHome, home, endpoints }) {
  const { radius } = globeLayout({ width, height, unit });
  const rot = rotation(elapsedMs);
  const project = (place) => {
    const q = view(unitVector(place), rot, TILT);
    return { x: width / 2 + radius * q[0], y: height / 2 - radius * q[1], visible: visible(q) };
  };
  const anchors = (endpoints || []).slice(0, MAX_ENDPOINTS).map(project);
  anchors.push(hasHome ? project(home) : { x: width / 2, y: height / 2, visible: false });
  return anchors;
}
