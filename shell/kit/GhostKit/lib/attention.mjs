// A lasting condition, such as a dead drive, should stop costing frames once it has drawn attention: an alert
// blinks for the attention window, counted only while it is on screen, and then holds still. The window is
// appearance.alert_attention_s; this is the length the shell uses until a config says otherwise.
export const DEFAULT_ATTENTION_MS = 10_000;

const NEVER_SHOWN = { spentMs: 0, showing: 0, sinceMs: 0 };

// What a spent alert has spent: more than any window, so one set later cannot give the alert its attention back.
const SPENT = Infinity;

// A clock that stepped back to before the showing began counts as the whole window spent, so an alert already
// shown never blinks again because of the step.
function showedMs(sinceMs, clockMs) {
  if (clockMs < sinceMs) {
    return SPENT;
  }
  return clockMs - sinceMs;
}

function spentMs(record, clockMs) {
  if (record.showing === 0) {
    return record.spentMs;
  }
  return record.spentMs + showedMs(record.sinceMs, clockMs);
}

// A record that has used up its window is spent for good, so a longer window set afterwards does not re-arm it.
function committedMs(record, clockMs, windowMs) {
  const total = spentMs(record, clockMs);
  return total >= windowMs ? SPENT : total;
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
export function hideAlert(seen, key, clockMs, windowMs) {
  const record = seen.get(key);
  const showing = record.showing - 1;
  seen.set(key, {
    spentMs: showing === 0 ? committedMs(record, clockMs, windowMs) : record.spentMs,
    showing,
    sinceMs: record.sinceMs,
  });
}

// What is left of the window for a key, which is the whole window for one never shown. The window is read at each
// call rather than kept, so a change of the setting applies to alerts already part-way through it.
export function attentionLeftMs(seen, key, clockMs, windowMs) {
  return Math.max(0, windowMs - spentMs(seen.get(key) ?? NEVER_SHOWN, clockMs));
}

// Marks the window spent for a key that is showing. The box's timer ends its blink in wall time while the clock
// counts in whole ticks, so without this the clock could read a few milliseconds short of the window, and the next
// showing would blink for them.
export function spendAttention(seen, key, clockMs) {
  seen.set(key, { spentMs: SPENT, showing: seen.get(key).showing, sinceMs: clockMs });
}

// A window shortened below what an alert has spent ends it, shown or not. It has to be marked then, because a
// window lengthened again later would otherwise give back time the shorter one had already used up.
export function spendExhausted(seen, clockMs, windowMs) {
  for (const [key, record] of seen) {
    if (spentMs(record, clockMs) >= windowMs) {
      // Written out because QML's JavaScript engine cannot parse object spread.
      seen.set(key, { spentMs: SPENT, showing: record.showing, sinceMs: record.sinceMs });
    }
  }
}

// The greeting's reveal is a new showing for every alert, so each blinks for its window again. Records of boxes that
// are showing stay, because their hide has yet to commit the time they spent.
export function rearmAttention(seen) {
  for (const [key, record] of seen) {
    if (record.showing === 0) {
      seen.delete(key);
    }
  }
}

const MS_PER_SECOND = 1000;

// A held alert's stripes jump once per step instead of scrolling, which keeps the scroll's average speed (one tile
// per period) without a frame source of its own. `wallMs` is wall-clock milliseconds; the phase depends only on the
// step it falls in, so it changes on step boundaries and nowhere else. Floored, so the phase is whole pixels and
// stays below the tile.
export function heldStripePhase(wallMs, tilePx, periodMs, stepMs) {
  const stepStartMs = Math.floor(wallMs / stepMs) * stepMs;
  return Math.floor((tilePx * (stepStartMs % periodMs)) / periodMs);
}

// A step of whole seconds lands on the wall clock's second boundaries, which FrameClock's seconds clock already
// delivers; any other step needs the frame clock's ticks to be noticed.
export function stepsOnWholeSeconds(stepMs) {
  return stepMs % MS_PER_SECOND === 0;
}

// A new alert's text flickers in over this long: each opacity starts at its percentage of ARRIVAL_MS and holds until
// the next one starts.
export const ARRIVAL_MS = 1600;
export const ARRIVAL_START_PERCENTS = [0, 6, 10, 14, 40, 70];
export const ARRIVAL_OPACITIES = [0.3, 1, 0.46, 1, 0.82, 0.64];
