import test from 'node:test';
import assert from 'node:assert/strict';
import { ATTENTION_MS, attentionLeftMs, firstShown, heldStripePhase } from '../kit/GhostKit/lib/attention.mjs';

const T0 = 1_000_000;

test('an alert shown for the first time has its whole attention window left', () => {
  const seen = new Map();
  assert.equal(attentionLeftMs(firstShown(seen, 'a', T0), T0), ATTENTION_MS);
});

test('the attention window is 10 seconds', () => assert.equal(ATTENTION_MS, 10_000));

test('the window runs out after 10 seconds and not before', () => {
  assert.equal(attentionLeftMs(T0, T0 + ATTENTION_MS - 1), 1);
  assert.equal(attentionLeftMs(T0, T0 + ATTENTION_MS), 0);
  assert.equal(attentionLeftMs(T0, T0 + 5 * ATTENTION_MS), 0);
});

test('the same alert shown again later does not get a new window', () => {
  const seen = new Map();
  firstShown(seen, 'a', T0);
  assert.equal(firstShown(seen, 'a', T0 + 60_000), T0);
  assert.equal(attentionLeftMs(firstShown(seen, 'a', T0 + 60_000), T0 + 60_000), 0);
});

test('the same alert shown again inside the window keeps what is left of it', () => {
  const seen = new Map();
  firstShown(seen, 'a', T0);
  assert.equal(attentionLeftMs(firstShown(seen, 'a', T0 + 4_000), T0 + 4_000), ATTENTION_MS - 4_000);
});

test('a new alert gets its own window while an old one stays still', () => {
  const seen = new Map();
  firstShown(seen, 'old', T0);
  const later = T0 + 60_000;
  assert.equal(attentionLeftMs(firstShown(seen, 'old', later), later), 0);
  assert.equal(attentionLeftMs(firstShown(seen, 'new', later), later), ATTENTION_MS);
});

test('a clock that steps back to before the first showing leaves nothing to blink', () => {
  assert.equal(attentionLeftMs(T0, T0 - 5_000), 0);
});

const TILE_PX = 16;
const STRIPE_PERIOD_MS = 1600;
const phaseAt = (epochSecond, tilePx = TILE_PX) => heldStripePhase(epochSecond, tilePx, STRIPE_PERIOD_MS);

test('a held alert steps its stripes 10 px a second on a 16 px tile', () => {
  const phases = [0, 1, 2, 3, 4, 5, 6, 7, 8].map((second) => phaseAt(second));
  assert.deepEqual(phases, [0, 10, 4, 14, 8, 2, 12, 6, 0]);
});

test('the held phase follows the wall clock second, not when the box appeared', () => {
  assert.equal(phaseAt(1_800_000_003), 14);
  assert.equal(phaseAt(1_800_000_004), 8);
});

test('the held phase wraps at the tile and repeats every eight seconds', () => {
  for (let second = 0; second < 64; second += 1) {
    assert.ok(phaseAt(second) >= 0 && phaseAt(second) < TILE_PX);
    assert.equal(phaseAt(second + 8), phaseAt(second));
  }
});

test('the held phase keeps the scroll speed of one tile per 1.6 seconds', () => {
  const advancedPx = Array.from({ length: 8 }, (_, second) => (phaseAt(second + 1) - phaseAt(second) + TILE_PX) % TILE_PX);
  assert.equal(advancedPx.reduce((total, step) => total + step, 0), 5 * TILE_PX);
});

test('the held phase is in whole pixels at any scale', () => {
  for (const tilePx of [1, 13, 16, 21, 24, 32]) {
    for (let second = 0; second < 16; second += 1) {
      const phase = phaseAt(second, tilePx);
      assert.ok(Number.isInteger(phase), `tile ${tilePx}, second ${second}: ${phase}`);
      assert.ok(phase >= 0 && phase < tilePx);
    }
  }
});

test('the held phase scales with the tile', () => {
  assert.equal(phaseAt(1, 32), 20);
});
