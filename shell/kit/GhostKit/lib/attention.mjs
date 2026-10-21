// A lasting condition, such as a dead drive, should stop costing frames once it has drawn attention: an alert
// blinks for this long, counted only while it is on screen, and then holds still.
export const ATTENTION_MS = 10_000;

const NEVER_SHOWN = { spentMs: 0, showing: 0, sinceMs: 0 };

// A clock that stepped back to before the showing began counts as the whole window spent, so an alert already
// shown never blinks again because of the step.
function showedMs(sinceMs, clockMs) {
  if (clockMs < sinceMs) {
    return ATTENTION_MS;
  }
  return clockMs - sinceMs;
}

function spentMs(record, clockMs) {
  if (record.showing === 0) {
    return record.spentMs;
  }
  return record.spentMs + showedMs(record.sinceMs, clockMs);
}

// `seen` is the session's record of each alert key: the attention it has spent, how many boxes show it right now
// and the clock reading since which any of them has. Two boxes can show one key at once (across styles), so the
// time is counted from the first to appear to the last to hide. `clockMs` must come from a clock that pauses
// while the dashboard cannot be seen, so that a covered desktop spends nothing.
export function showAlert(seen, key, clockMs) {
  const record = seen.get(key) ?? NEVER_SHOWN;
  seen.set(key, {
    spentMs: record.spentMs,
    showing: record.showing + 1,
    sinceMs: record.showing === 0 ? clockMs : record.sinceMs,
  });
}

// The last box to hide commits the time the alert was on screen; until then the record keeps counting.
export function hideAlert(seen, key, clockMs) {
  const record = seen.get(key);
  const showing = record.showing - 1;
  seen.set(key, {
    spentMs: showing === 0 ? spentMs(record, clockMs) : record.spentMs,
    showing,
    sinceMs: record.sinceMs,
  });
}

// What is left of the window for a key, which is the whole window for one never shown.
export function attentionLeftMs(seen, key, clockMs) {
  return Math.max(0, ATTENTION_MS - spentMs(seen.get(key) ?? NEVER_SHOWN, clockMs));
}

// Marks the window spent for a key that is showing. The box's timer ends its blink in wall time while the clock
// counts in whole ticks, so without this the clock could read a few milliseconds short of the window, and the next
// showing would blink for them.
export function spendAttention(seen, key, clockMs) {
  seen.set(key, { spentMs: ATTENTION_MS, showing: seen.get(key).showing, sinceMs: clockMs });
}

const MS_PER_SECOND = 1000;

// A held alert's stripes advance once per wall-clock second instead of scrolling, which keeps the scroll's average
// speed (one tile per period) without a frame clock of their own. Floored, so the phase is whole pixels and stays
// below the tile.
export function heldStripePhase(epochSecond, tilePx, periodMs) {
  const intoPeriodMs = (epochSecond * MS_PER_SECOND) % periodMs;
  return Math.floor(tilePx * intoPeriodMs / periodMs);
}
