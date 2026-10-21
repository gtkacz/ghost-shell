// A lasting condition, such as a dead drive, should stop costing frames once it has drawn attention: an alert
// blinks for this long the first time it is shown and then holds still.
export const ATTENTION_MS = 10_000;

// `seen` is the session's record of when each alert was first shown; a key never shown before is recorded as shown
// at `nowMs`.
export function firstShown(seen, key, nowMs) {
  if (!seen.has(key)) {
    seen.set(key, nowMs);
  }
  return seen.get(key);
}

// A wall clock that stepped back to before the first showing counts as a spent window, so an alert already shown
// never blinks again because of the step.
export function attentionLeftMs(shownMs, nowMs) {
  if (nowMs < shownMs) {
    return 0;
  }
  return Math.max(0, ATTENTION_MS - (nowMs - shownMs));
}

const MS_PER_SECOND = 1000;

// A held alert's stripes advance once per wall-clock second instead of scrolling, which keeps the scroll's average
// speed (one tile per period) without a frame clock of their own. Floored, so the phase is whole pixels and stays
// below the tile.
export function heldStripePhase(epochSecond, tilePx, periodMs) {
  const intoPeriodMs = (epochSecond * MS_PER_SECOND) % periodMs;
  return Math.floor(tilePx * intoPeriodMs / periodMs);
}
