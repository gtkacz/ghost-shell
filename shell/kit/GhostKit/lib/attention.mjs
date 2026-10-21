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
