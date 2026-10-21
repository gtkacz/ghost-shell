import test from 'node:test';
import assert from 'node:assert/strict';
import {
  ATTENTION_MS,
  attentionLeftMs,
  heldStripePhase,
  hideAlert,
  showAlert,
  spendAttention,
} from '../kit/GhostKit/lib/attention.mjs';

const T0 = 1_000_000;
const SECOND_MS = 1000;

test('the attention window is 10 seconds', () => assert.equal(ATTENTION_MS, 10_000));

test('an alert never shown has its whole attention window left', () => {
  assert.equal(attentionLeftMs(new Map(), 'a', T0), ATTENTION_MS);
});

test('an alert shown for the first time has its whole attention window left', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  assert.equal(attentionLeftMs(seen, 'a', T0), ATTENTION_MS);
});

test('the window runs out after 10 seconds on screen and not before', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  assert.equal(attentionLeftMs(seen, 'a', T0 + ATTENTION_MS - 1), 1);
  assert.equal(attentionLeftMs(seen, 'a', T0 + ATTENTION_MS), 0);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 5 * ATTENTION_MS), 0);
});

test('3 seconds shown, hidden and shown again leaves 7 seconds', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS);
  const later = T0 + 60 * SECOND_MS;
  showAlert(seen, 'a', later);
  assert.equal(attentionLeftMs(seen, 'a', later), ATTENTION_MS - 3 * SECOND_MS);
});

test('time while hidden is not spent, however long it lasts', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 3 * SECOND_MS), ATTENTION_MS - 3 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 600 * SECOND_MS), ATTENTION_MS - 3 * SECOND_MS);
});

test('every showing adds its own time to what is spent', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS);
  showAlert(seen, 'a', T0 + 20 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 24 * SECOND_MS);
  showAlert(seen, 'a', T0 + 90 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 90 * SECOND_MS), 3 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 93 * SECOND_MS), 0);
});

test('two boxes showing the same alert at once count the time once', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  showAlert(seen, 'a', T0);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 4 * SECOND_MS), ATTENTION_MS - 4 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 4 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 4 * SECOND_MS);
  showAlert(seen, 'a', T0 + 30 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 30 * SECOND_MS), ATTENTION_MS - 4 * SECOND_MS);
});

test('a second box showing the alert later does not restart the count', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  showAlert(seen, 'a', T0 + 4 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 4 * SECOND_MS), ATTENTION_MS - 4 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 7 * SECOND_MS), ATTENTION_MS - 7 * SECOND_MS);
});

test('the time keeps counting for the box left showing when the other one hides', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  showAlert(seen, 'a', T0 + 4 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 6 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 8 * SECOND_MS), ATTENTION_MS - 8 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 8 * SECOND_MS);
  showAlert(seen, 'a', T0 + 50 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 50 * SECOND_MS), 2 * SECOND_MS);
});

test('a fully spent alert stays held when it is shown again', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + ATTENTION_MS + 5 * SECOND_MS);
  const later = T0 + 60 * SECOND_MS;
  showAlert(seen, 'a', later);
  assert.equal(attentionLeftMs(seen, 'a', later), 0);
});

test('an alert spent to the millisecond stays held when it is shown again', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + ATTENTION_MS);
  showAlert(seen, 'a', T0 + 20 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 20 * SECOND_MS), 0);
});

test('a new alert starts with its whole window while an old one stays held', () => {
  const seen = new Map();
  showAlert(seen, 'old', T0);
  hideAlert(seen, 'old', T0 + ATTENTION_MS);
  const later = T0 + 60 * SECOND_MS;
  showAlert(seen, 'old', later);
  showAlert(seen, 'new', later);
  assert.equal(attentionLeftMs(seen, 'old', later), 0);
  assert.equal(attentionLeftMs(seen, 'new', later), ATTENTION_MS);
});

test('a box destroyed while showing commits the time it showed', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  // A cover destroys the box while it shows; its destruction hides it at the clock reading of that moment, and
  // that clock stands still for as long as the cover lasts.
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 3 * SECOND_MS), ATTENTION_MS - 3 * SECOND_MS);
  showAlert(seen, 'a', T0 + 3 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 3 * SECOND_MS), ATTENTION_MS - 3 * SECOND_MS);
});

test('a box destroyed while another still shows the alert leaves the count to the other', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  showAlert(seen, 'a', T0 + SECOND_MS);
  hideAlert(seen, 'a', T0 + 2 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 5 * SECOND_MS), ATTENTION_MS - 5 * SECOND_MS);
});

test('a clock that steps back during a showing leaves nothing to blink', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  assert.equal(attentionLeftMs(seen, 'a', T0 - 5 * SECOND_MS), 0);
});

test('a clock that steps back before a hide does not give the time back', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 - 5 * SECOND_MS);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS), 0);
});

test('a step back does not give back time already spent', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS);
  showAlert(seen, 'a', T0 + 100 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 50 * SECOND_MS), 0);
});

test('spending an alert while it shows holds it at once and after every box hides', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  showAlert(seen, 'a', T0 + SECOND_MS);
  // The clock counts in whole ticks, so it can read a little short of the window when the timer ends it.
  const ticksShort = ATTENTION_MS - 17;
  spendAttention(seen, 'a', T0 + ticksShort);
  assert.equal(attentionLeftMs(seen, 'a', T0 + ticksShort), 0);
  hideAlert(seen, 'a', T0 + ticksShort + 100);
  hideAlert(seen, 'a', T0 + ticksShort + 100);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS), 0);
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
