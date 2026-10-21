import test from 'node:test';
import assert from 'node:assert/strict';
import {
  DEFAULT_ATTENTION_MS,
  attentionLeftMs,
  heldStripePhase,
  hideAlert,
  showAlert,
  spendAttention,
  spendExhausted,
  stepsOnWholeSeconds,
} from '../kit/GhostKit/lib/attention.mjs';

const T0 = 1_000_000;
const SECOND_MS = 1000;
const WINDOW_MS = DEFAULT_ATTENTION_MS;
const LONG_WINDOW_MS = 600 * SECOND_MS;

test('the default attention window is 10 seconds', () => assert.equal(DEFAULT_ATTENTION_MS, 10_000));

test('an alert never shown has its whole attention window left', () => {
  assert.equal(attentionLeftMs(new Map(), 'a', T0, WINDOW_MS), WINDOW_MS);
});

test('an alert shown for the first time has its whole attention window left', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  assert.equal(attentionLeftMs(seen, 'a', T0, WINDOW_MS), WINDOW_MS);
});

test('the window runs out after 10 seconds on screen and not before', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  assert.equal(attentionLeftMs(seen, 'a', T0 + WINDOW_MS - 1, WINDOW_MS), 1);
  assert.equal(attentionLeftMs(seen, 'a', T0 + WINDOW_MS, WINDOW_MS), 0);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 5 * WINDOW_MS, WINDOW_MS), 0);
});

test('3 seconds shown, hidden and shown again leaves 7 seconds', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS, WINDOW_MS);
  const later = T0 + 60 * SECOND_MS;
  showAlert(seen, 'a', later);
  assert.equal(attentionLeftMs(seen, 'a', later, WINDOW_MS), WINDOW_MS - 3 * SECOND_MS);
});

test('time while hidden is not spent, however long it lasts', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS, WINDOW_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 3 * SECOND_MS, WINDOW_MS), WINDOW_MS - 3 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 600 * SECOND_MS, WINDOW_MS), WINDOW_MS - 3 * SECOND_MS);
});

test('every showing adds its own time to what is spent', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS, WINDOW_MS);
  showAlert(seen, 'a', T0 + 20 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 24 * SECOND_MS, WINDOW_MS);
  showAlert(seen, 'a', T0 + 90 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 90 * SECOND_MS, WINDOW_MS), 3 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 93 * SECOND_MS, WINDOW_MS), 0);
});

test('two boxes showing the same alert at once count the time once', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  showAlert(seen, 'a', T0);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 4 * SECOND_MS, WINDOW_MS), WINDOW_MS - 4 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 4 * SECOND_MS, WINDOW_MS);
  hideAlert(seen, 'a', T0 + 4 * SECOND_MS, WINDOW_MS);
  showAlert(seen, 'a', T0 + 30 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 30 * SECOND_MS, WINDOW_MS), WINDOW_MS - 4 * SECOND_MS);
});

test('a second box showing the alert later does not restart the count', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  showAlert(seen, 'a', T0 + 4 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 4 * SECOND_MS, WINDOW_MS), WINDOW_MS - 4 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 7 * SECOND_MS, WINDOW_MS), WINDOW_MS - 7 * SECOND_MS);
});

test('the time keeps counting for the box left showing when the other one hides', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  showAlert(seen, 'a', T0 + 4 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 6 * SECOND_MS, WINDOW_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 8 * SECOND_MS, WINDOW_MS), WINDOW_MS - 8 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 8 * SECOND_MS, WINDOW_MS);
  showAlert(seen, 'a', T0 + 50 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 50 * SECOND_MS, WINDOW_MS), 2 * SECOND_MS);
});

test('a fully spent alert stays held when it is shown again', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + WINDOW_MS + 5 * SECOND_MS, WINDOW_MS);
  const later = T0 + 60 * SECOND_MS;
  showAlert(seen, 'a', later);
  assert.equal(attentionLeftMs(seen, 'a', later, WINDOW_MS), 0);
});

test('an alert spent to the millisecond stays held when it is shown again', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + WINDOW_MS, WINDOW_MS);
  showAlert(seen, 'a', T0 + 20 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 20 * SECOND_MS, WINDOW_MS), 0);
});

test('a new alert starts with its whole window while an old one stays held', () => {
  const seen = new Map();
  showAlert(seen, 'old', T0);
  hideAlert(seen, 'old', T0 + WINDOW_MS, WINDOW_MS);
  const later = T0 + 60 * SECOND_MS;
  showAlert(seen, 'old', later);
  showAlert(seen, 'new', later);
  assert.equal(attentionLeftMs(seen, 'old', later, WINDOW_MS), 0);
  assert.equal(attentionLeftMs(seen, 'new', later, WINDOW_MS), WINDOW_MS);
});

test('a key that changes while shown keeps the old key partly spent and starts the new one whole', () => {
  const seen = new Map();
  showAlert(seen, 'id:warn', T0);
  const escalation = T0 + 4 * SECOND_MS;
  hideAlert(seen, 'id:warn', escalation, WINDOW_MS);
  showAlert(seen, 'id:crit', escalation);
  assert.equal(attentionLeftMs(seen, 'id:warn', escalation, WINDOW_MS), 6 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'id:crit', escalation, WINDOW_MS), WINDOW_MS);
  const back = escalation + 2 * SECOND_MS;
  hideAlert(seen, 'id:crit', back, WINDOW_MS);
  showAlert(seen, 'id:warn', back);
  assert.equal(attentionLeftMs(seen, 'id:warn', back, WINDOW_MS), 6 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'id:crit', back, WINDOW_MS), 8 * SECOND_MS);
});

test('a box destroyed while showing commits the time it showed', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  // A cover destroys the box while it shows; its destruction hides it at the clock reading of that moment, and
  // that clock stands still for as long as the cover lasts.
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS, WINDOW_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 3 * SECOND_MS, WINDOW_MS), WINDOW_MS - 3 * SECOND_MS);
  showAlert(seen, 'a', T0 + 3 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 3 * SECOND_MS, WINDOW_MS), WINDOW_MS - 3 * SECOND_MS);
});

test('a box destroyed while another still shows the alert leaves the count to the other', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  showAlert(seen, 'a', T0 + SECOND_MS);
  hideAlert(seen, 'a', T0 + 2 * SECOND_MS, WINDOW_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 5 * SECOND_MS, WINDOW_MS), WINDOW_MS - 5 * SECOND_MS);
});

test('a clock that steps back during a showing leaves nothing to blink', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  assert.equal(attentionLeftMs(seen, 'a', T0 - 5 * SECOND_MS, WINDOW_MS), 0);
});

test('a clock that steps back before a hide does not give the time back', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 - 5 * SECOND_MS, WINDOW_MS);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS, WINDOW_MS), 0);
});

test('a step back does not give back time already spent', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS, WINDOW_MS);
  showAlert(seen, 'a', T0 + 100 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 50 * SECOND_MS, WINDOW_MS), 0);
});

test('spending an alert while it shows holds it at once and after every box hides', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  showAlert(seen, 'a', T0 + SECOND_MS);
  // The clock counts in whole ticks, so it can read a little short of the window when the timer ends it.
  const ticksShort = WINDOW_MS - 17;
  spendAttention(seen, 'a', T0 + ticksShort);
  assert.equal(attentionLeftMs(seen, 'a', T0 + ticksShort, WINDOW_MS), 0);
  hideAlert(seen, 'a', T0 + ticksShort + 100, WINDOW_MS);
  hideAlert(seen, 'a', T0 + ticksShort + 100, WINDOW_MS);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS, WINDOW_MS), 0);
});

test('the window is as long as it is given, to the millisecond', () => {
  for (const windowMs of [3 * SECOND_MS, 600 * SECOND_MS]) {
    const seen = new Map();
    showAlert(seen, 'a', T0);
    assert.equal(attentionLeftMs(seen, 'a', T0, windowMs), windowMs);
    assert.equal(attentionLeftMs(seen, 'a', T0 + windowMs - 1, windowMs), 1);
    assert.equal(attentionLeftMs(seen, 'a', T0 + windowMs, windowMs), 0);
  }
});

test('a 600 second window is spent across showings like a short one', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 200 * SECOND_MS, LONG_WINDOW_MS);
  showAlert(seen, 'a', T0 + 900 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 900 * SECOND_MS, LONG_WINDOW_MS), 400 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 1300 * SECOND_MS, LONG_WINDOW_MS);
  showAlert(seen, 'a', T0 + 2000 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 2000 * SECOND_MS, LONG_WINDOW_MS), 0);
});

test('a window that grows while an alert shows counts what is spent against the new length', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  const now = T0 + 2 * SECOND_MS;
  assert.equal(attentionLeftMs(seen, 'a', now, 3 * SECOND_MS), SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', now, 6 * SECOND_MS), 4 * SECOND_MS);
});

test('a window that shrinks below what an alert has spent holds it at once', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  const now = T0 + 5 * SECOND_MS;
  assert.equal(attentionLeftMs(seen, 'a', now, LONG_WINDOW_MS), LONG_WINDOW_MS - 5 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', now, 3 * SECOND_MS), 0);
});

test('a window that shrinks below what a hidden alert has spent holds it too', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 5 * SECOND_MS, LONG_WINDOW_MS);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS, 3 * SECOND_MS), 0);
});

test('an alert hidden with time left gets the rest of a longer window', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS, WINDOW_MS);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS, LONG_WINDOW_MS), LONG_WINDOW_MS - 3 * SECOND_MS);
});

test('an alert hidden after its window ended stays held under a longer window', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 3 * SECOND_MS, 3 * SECOND_MS);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS, LONG_WINDOW_MS), 0);
});

test('an alert whose window ended while it showed stays held under a longer window', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  spendAttention(seen, 'a', T0 + 3 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 3 * SECOND_MS, LONG_WINDOW_MS), 0);
  hideAlert(seen, 'a', T0 + 4 * SECOND_MS, LONG_WINDOW_MS);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS, LONG_WINDOW_MS), 0);
});

test('a clock that steps back leaves nothing to blink however long the window', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  assert.equal(attentionLeftMs(seen, 'a', T0 - 5 * SECOND_MS, LONG_WINDOW_MS), 0);
  hideAlert(seen, 'a', T0 - 5 * SECOND_MS, LONG_WINDOW_MS);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS, LONG_WINDOW_MS), 0);
});

test('spendExhausted holds what a shrunken window no longer covers, showing or not, and nothing else', () => {
  const seen = new Map();
  showAlert(seen, 'hidden', T0);
  hideAlert(seen, 'hidden', T0 + 5 * SECOND_MS, LONG_WINDOW_MS);
  showAlert(seen, 'showing', T0);
  showAlert(seen, 'brief', T0);
  hideAlert(seen, 'brief', T0 + SECOND_MS, LONG_WINDOW_MS);
  showAlert(seen, 'never-ended', T0 + 5 * SECOND_MS);

  const now = T0 + 5 * SECOND_MS;
  spendExhausted(seen, now, 3 * SECOND_MS);

  const later = now + SECOND_MS;
  assert.equal(attentionLeftMs(seen, 'hidden', later, LONG_WINDOW_MS), 0);
  assert.equal(attentionLeftMs(seen, 'showing', later, LONG_WINDOW_MS), 0);
  assert.equal(attentionLeftMs(seen, 'brief', later, LONG_WINDOW_MS), LONG_WINDOW_MS - SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'never-ended', later, LONG_WINDOW_MS), LONG_WINDOW_MS - SECOND_MS);
});

test('spendExhausted holds a showing alert for good, so it stays held after every box hides', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  spendExhausted(seen, T0 + 5 * SECOND_MS, 3 * SECOND_MS);
  hideAlert(seen, 'a', T0 + 6 * SECOND_MS, LONG_WINDOW_MS);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS, LONG_WINDOW_MS), 0);
});

test('spendExhausted at a window that still covers every alert changes nothing', () => {
  const seen = new Map();
  showAlert(seen, 'a', T0);
  hideAlert(seen, 'a', T0 + 5 * SECOND_MS, LONG_WINDOW_MS);
  spendExhausted(seen, T0 + 60 * SECOND_MS, LONG_WINDOW_MS);
  showAlert(seen, 'a', T0 + 60 * SECOND_MS);
  assert.equal(attentionLeftMs(seen, 'a', T0 + 60 * SECOND_MS, LONG_WINDOW_MS), LONG_WINDOW_MS - 5 * SECOND_MS);
});

const TILE_PX = 16;
const STRIPE_PERIOD_MS = 1600;
const STRIPE_STEP_MS = 1000;
const heldPhase = (wallMs, { tilePx = TILE_PX, periodMs = STRIPE_PERIOD_MS, stepMs = STRIPE_STEP_MS } = {}) =>
  heldStripePhase(wallMs, tilePx, periodMs, stepMs);
const phaseAt = (epochSecond, tilePx = TILE_PX) => heldPhase(epochSecond * SECOND_MS, { tilePx });

// What the held phase was before the step became a setting: whole epoch seconds in, a 1 s step implied.
const phaseBeforeTheStepSetting = (epochSecond, tilePx, periodMs) =>
  Math.floor((tilePx * ((epochSecond * 1000) % periodMs)) / periodMs);

test('with the default period and step the held phase is what it was before they were settings', () => {
  for (const tilePx of [1, 13, 16, 21, 24, 32]) {
    for (const epochSecond of [0, 1, 2, 7, 8, 59, 1_800_000_003, 1_800_000_004]) {
      assert.equal(
        phaseAt(epochSecond, tilePx),
        phaseBeforeTheStepSetting(epochSecond, tilePx, STRIPE_PERIOD_MS),
        `tile ${tilePx}, second ${epochSecond}`,
      );
    }
  }
});

test('a held alert steps its stripes 10 px a second on a 16 px tile', () => {
  const phases = [0, 1, 2, 3, 4, 5, 6, 7, 8].map((second) => phaseAt(second));
  assert.deepEqual(phases, [0, 10, 4, 14, 8, 2, 12, 6, 0]);
});

test('the held phase follows the wall clock second, not when the box appeared', () => {
  assert.equal(phaseAt(1_800_000_003), 14);
  assert.equal(phaseAt(1_800_000_004), 8);
});

test('the held phase holds for the whole of a step and moves on its boundary', () => {
  const wallMs = 1_800_000_003 * SECOND_MS;
  assert.equal(heldPhase(wallMs), 14);
  assert.equal(heldPhase(wallMs + 500), 14);
  assert.equal(heldPhase(wallMs + 999), 14);
  assert.equal(heldPhase(wallMs + 1000), 8);
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

test('a 2 second step moves every 2 seconds and holds in between', () => {
  const phases = [0, 2000, 4000, 6000, 8000].map((wallMs) => heldPhase(wallMs, { stepMs: 2000 }));
  assert.deepEqual(phases, [0, 4, 8, 12, 0]);
  for (const wallMs of [1, 1000, 1999]) {
    assert.equal(heldPhase(wallMs, { stepMs: 2000 }), 0);
  }
  for (const wallMs of [2001, 2999, 3999]) {
    assert.equal(heldPhase(wallMs, { stepMs: 2000 }), 4);
  }
});

test('a 0.5 second step moves twice a second and holds in between', () => {
  const phases = [0, 500, 1000, 1500, 2000, 2500].map((wallMs) => heldPhase(wallMs, { stepMs: 500 }));
  assert.deepEqual(phases, [0, 5, 10, 15, 4, 9]);
  for (const wallMs of [1, 250, 499]) {
    assert.equal(heldPhase(wallMs, { stepMs: 500 }), 0);
  }
  for (const wallMs of [501, 750, 999]) {
    assert.equal(heldPhase(wallMs, { stepMs: 500 }), 5);
  }
});

test('a step that does not divide the second still lands on whole steps of the wall clock', () => {
  assert.equal(heldPhase(2 * SECOND_MS - 1, { stepMs: 700 }), heldPhase(1400, { stepMs: 700 }));
  assert.equal(heldPhase(1400, { stepMs: 700 }), Math.floor((TILE_PX * 1400) / STRIPE_PERIOD_MS));
  assert.equal(heldPhase(2100, { stepMs: 700 }), Math.floor((TILE_PX * 500) / STRIPE_PERIOD_MS));
});

test('a slower period moves the stripes less on each step and repeats later', () => {
  const slow = { periodMs: 3200 };
  const phases = [0, 1, 2, 3, 4, 5, 6, 7].map((second) => heldPhase(second * SECOND_MS, slow));
  assert.deepEqual(phases, [0, 5, 10, 15, 4, 9, 14, 3]);
  assert.equal(heldPhase(16 * SECOND_MS, slow), heldPhase(0, slow));
  assert.equal(heldPhase(17 * SECOND_MS, slow), heldPhase(SECOND_MS, slow));
});

test('a faster period moves the stripes further on each step', () => {
  assert.equal(heldPhase(SECOND_MS, { periodMs: 800 }), 4);
});

test('the held phase wraps at the tile for any period and step', () => {
  for (const periodMs of [100, 800, 1600, 3200, 10_000]) {
    for (const stepMs of [50, 500, 1000, 2000, 3000, 7000]) {
      for (let wallMs = 0; wallMs < 40_000; wallMs += 130) {
        const phase = heldPhase(wallMs + 1_800_000_000_000, { periodMs, stepMs });
        assert.ok(phase >= 0 && phase < TILE_PX, `period ${periodMs}, step ${stepMs}, ${wallMs}: ${phase}`);
      }
    }
  }
});

test('the phase is the last pixel just before the period and the first one on it', () => {
  assert.equal(heldPhase(1500, { stepMs: 100 }), 15);
  assert.equal(heldPhase(1599, { stepMs: 100 }), 15);
  assert.equal(heldPhase(1600, { stepMs: 100 }), 0);
});

test('the held phase is in whole pixels for any period, step and scale', () => {
  for (const tilePx of [1, 13, 16, 21, 32]) {
    for (const periodMs of [800, 1600, 3200]) {
      for (const stepMs of [500, 1000, 2000]) {
        for (let wallMs = 0; wallMs < 20_000; wallMs += 250) {
          const phase = heldPhase(wallMs, { tilePx, periodMs, stepMs });
          assert.ok(Number.isInteger(phase), `tile ${tilePx}, period ${periodMs}, step ${stepMs}, ${wallMs}: ${phase}`);
          assert.ok(phase >= 0 && phase < tilePx);
        }
      }
    }
  }
});

test('only a step of whole seconds can ride the seconds clock', () => {
  for (const stepMs of [1000, 2000, 3000, 60_000]) {
    assert.equal(stepsOnWholeSeconds(stepMs), true, `${stepMs} ms`);
  }
  for (const stepMs of [1, 250, 500, 1500, 1600, 2500]) {
    assert.equal(stepsOnWholeSeconds(stepMs), false, `${stepMs} ms`);
  }
});
