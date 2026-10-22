// The rules of the greeting: its timed sequence (spec §7, timed after the reference mockup's boot), its subtitle,
// its easings and the phase machine that decides when it plays. Nothing here reads a clock or keeps state between
// calls, so a skip, a freeze and a late tick all read the same frame. Plans are never mutated; QML's JavaScript
// engine cannot parse object spread, so copies are made with Object.assign.

const BISECTION_STEPS = 40;

// CSS cubic-bezier(x1, y1, x2, y2) as a function of progress. The curve is parametric in t, so x(t) is inverted by
// bisection, which is exact to far below a pixel at this many steps.
export function cubicBezier(x1, y1, x2, y2) {
  const coordinate = (t, a, b) => 3 * (1 - t) * (1 - t) * t * a + 3 * (1 - t) * t * t * b + t * t * t;
  return (x) => {
    if (x <= 0) {
      return 0;
    }
    if (x >= 1) {
      return 1;
    }
    let low = 0;
    let high = 1;
    for (let i = 0; i < BISECTION_STEPS; i += 1) {
      const mid = (low + high) / 2;
      if (coordinate(mid, x1, x2) < x) {
        low = mid;
      } else {
        high = mid;
      }
    }
    return coordinate((low + high) / 2, y1, y2);
  };
}

export const EASE_OUT = cubicBezier(0, 0, 0.58, 1);
export const TUBE_OPEN_EASING = cubicBezier(0.2, 0.8, 0.3, 1);

const LINE_GROW_MS = 120;
const LINE_SCALE_X_FROM = 0.05;
const TUBE_OPEN_AT_MS = 130;
const TUBE_OPEN_MS = 300;
const LINE_SCALE_Y_TO = 220;
const GLOW_MS = 900;
const GLOW_PEAK_AT = 0.35;
const GLOW_PEAK_OPACITY = 0.9;
const GLOW_FULL_INSET = 0.5;
const TUBE_END_MS = TUBE_OPEN_AT_MS + GLOW_MS;

const DOT_STEPS = 24;
const DOT_STEP_MS = 36;
const DOT_PRINT_MS = (DOT_STEPS + 1) * DOT_STEP_MS;

const ROLL_LEAD_MS = 350;
const ROLL_HEIGHT = 140;
const ROLL_PERIOD_MS = 3400;
// The mockup's stage; a screen with another aspect ratio passes its own height so the band's speed stays the same.
const MOCKUP_DESIGN_HEIGHT = 720;

const TITLE_CHAR_MS = 62;
const SUBTITLE_CHAR_MS = 26;
const JITTER_MIN = 0.6;
const JITTER_SPAN = 0.8;
const SPACE_EXTRA = 0.6;
const LINE_PAUSE_MS = 420;
const BURN_MS = 550;
const HOLD_MS = 1500;

const SCAN_STEPS = 30;
const SCAN_STEP_MS = 26;
const SCAN_PASS_MS = (SCAN_STEPS + 1) * SCAN_STEP_MS;

const SEPARATOR = '  ·  ';
const WELCOME_JAPANESE = 'おかえりなさい';
const SESSION_DIGITS = 4;

function clamp01(x) {
  return Math.min(1, Math.max(0, x));
}

// The timed head. Ms are counted from the greeting's first frame. From the dots, the dot print is skipped because
// the overlay has been resting on the printed dots under the lock.
export function sequence({ fromDots, designHeight = MOCKUP_DESIGN_HEIGHT }) {
  const dotsMs = fromDots ? 0 : DOT_PRINT_MS;
  const typingStartMs = TUBE_END_MS + dotsMs + ROLL_LEAD_MS;
  return {
    fromDots,
    designHeight,
    typingStartMs,
    rollStartMs: typingStartMs - ROLL_LEAD_MS,
    lines: null,
    scanStartMs: null,
    doneMs: null,
  };
}

// Each character's appear time is its predecessor's plus a jittered wait, as the mockup's typeOut does it, so a
// space lingers. The wait after the last character still counts: it is what the line pause is measured from.
function typeLine(text, startMs, baseMs, random) {
  const appearMs = [];
  let nowMs = startMs;
  for (let i = 0; i < text.length; i += 1) {
    appearMs.push(nowMs);
    const extra = text[i] === ' ' ? SPACE_EXTRA * baseMs : 0;
    nowMs += baseMs * (JITTER_MIN + JITTER_SPAN * random()) + extra;
  }
  return { text, appearMs, endMs: nowMs };
}

// The text is fixed here, once: frames only read it, so a title that changes later cannot change a line that is
// already typing. `nameStart` stays on the plan for the view to colour the name.
export function typeText(plan, { title, nameStart, subtitle }, random) {
  const first = typeLine(title, plan.typingStartMs, TITLE_CHAR_MS, random);
  const cursorMoveMs = first.endMs + LINE_PAUSE_MS;
  const second = typeLine(subtitle, cursorMoveMs, SUBTITLE_CHAR_MS, random);
  const scanStartMs = second.endMs + HOLD_MS;
  return Object.assign({}, plan, {
    lines: [first, second],
    nameStart,
    cursorMoveMs,
    scanStartMs,
    doneMs: scanStartMs + SCAN_PASS_MS,
  });
}

function tubeAt(ms) {
  if (ms >= TUBE_END_MS) {
    return null;
  }
  const grown = ms >= LINE_GROW_MS ? 1 : EASE_OUT(ms / LINE_GROW_MS);
  const opened = TUBE_OPEN_EASING(clamp01((ms - TUBE_OPEN_AT_MS) / TUBE_OPEN_MS));
  // The glow's easing covers its whole run and the keyframe offsets are on the eased progress.
  const glow = EASE_OUT(clamp01((ms - TUBE_OPEN_AT_MS) / GLOW_MS));
  // The mockup's glow layer is fully open with its tint from power-on; only its clip animation waits for 130 ms.
  const glowStarted = ms >= TUBE_OPEN_AT_MS;
  const peaked = glow >= GLOW_PEAK_AT;
  const toPeak = glow / GLOW_PEAK_AT;
  const afterPeak = (glow - GLOW_PEAK_AT) / (1 - GLOW_PEAK_AT);
  return {
    lineScaleX: LINE_SCALE_X_FROM + (1 - LINE_SCALE_X_FROM) * grown,
    lineScaleY: 1 + (LINE_SCALE_Y_TO - 1) * opened,
    lineOpacity: 1 - opened,
    glowInset: peaked || !glowStarted ? 0 : GLOW_FULL_INSET * (1 - toPeak),
    glowOpacity: peaked ? GLOW_PEAK_OPACITY * (1 - afterPeak) : 1 - (1 - GLOW_PEAK_OPACITY) * toPeak,
  };
}

function dotStepAt(plan, ms) {
  if (plan.fromDots) {
    return DOT_STEPS;
  }
  if (ms < TUBE_END_MS) {
    return 0;
  }
  return Math.min(DOT_STEPS, Math.floor((ms - TUBE_END_MS) / DOT_STEP_MS));
}

function rollTopAt(plan, ms) {
  if (ms < plan.rollStartMs || (plan.doneMs !== null && ms >= plan.doneMs)) {
    return null;
  }
  const travel = plan.designHeight + ROLL_HEIGHT;
  return -ROLL_HEIGHT + (travel * ((ms - plan.rollStartMs) % ROLL_PERIOD_MS)) / ROLL_PERIOD_MS;
}

function lineAt(line, ms) {
  const burn = [];
  for (let i = 0; i < line.appearMs.length && line.appearMs[i] <= ms; i += 1) {
    burn.push(EASE_OUT(clamp01((ms - line.appearMs[i]) / BURN_MS)));
  }
  return { shown: burn.length, burn };
}

function cursorLineAt(plan, ms) {
  if (plan.lines === null || ms < plan.typingStartMs) {
    return null;
  }
  // With no subtitle there is no second line for the cursor to move to.
  const hasSubtitle = plan.lines[1].text.length > 0;
  return hasSubtitle && ms >= plan.cursorMoveMs ? 1 : 0;
}

function scanStepAt(plan, ms) {
  if (plan.scanStartMs === null || ms < plan.scanStartMs) {
    return null;
  }
  return Math.min(SCAN_STEPS, Math.floor((ms - plan.scanStartMs) / SCAN_STEP_MS));
}

const NO_TEXT = Object.freeze({ shown: 0, burn: Object.freeze([]) });

export function frameAt(plan, atMs) {
  const ms = Math.max(0, atMs);
  const typed = plan.lines !== null && ms >= plan.typingStartMs;
  return {
    tube: tubeAt(ms),
    dotStep: dotStepAt(plan, ms),
    rollTop: rollTopAt(plan, ms),
    lines: typed ? [lineAt(plan.lines[0], ms), lineAt(plan.lines[1], ms)] : [NO_TEXT, NO_TEXT],
    cursorLine: cursorLineAt(plan, ms),
    cursorPhaseMs: Math.max(0, ms - plan.typingStartMs),
    scanStep: scanStepAt(plan, ms),
    done: plan.doneMs !== null && ms >= plan.doneMs,
  };
}

// The armed overlay under the lock screen.
export function restingFrame() {
  return {
    tube: null,
    dotStep: DOT_STEPS,
    rollTop: null,
    lines: [NO_TEXT, NO_TEXT],
    cursorLine: null,
    cursorPhaseMs: 0,
    scanStep: null,
    done: false,
  };
}

export function subtitleText({ japanese, sessionId }) {
  const parts = [];
  if (japanese !== 'off') {
    parts.push(WELCOME_JAPANESE);
  }
  if (sessionId !== '') {
    parts.push('SESSION ' + sessionId.padStart(SESSION_DIGITS, '0'));
  }
  return parts.join(SEPARATOR);
}

// The phase machine. `enabled` is false for mode 'off', which never arms or plays. `seen` is the connection the
// last session value arrived on and its greeting_seq, or null before the first.
function greetingState(enabled, phase, kind, fromDots, seen) {
  return { enabled, phase, kind, fromDots, seen };
}

function withPhase(state, phase, kind, fromDots, seen) {
  return greetingState(state.enabled, phase, kind, fromDots, seen);
}

export function initialGreeting({ mode, greeted }) {
  if (mode === 'off') {
    return greetingState(false, 'idle', null, false, null);
  }
  if (mode === 'auto' && greeted) {
    return greetingState(true, 'idle', null, false, null);
  }
  return greetingState(true, 'playing', 'login', false, null);
}

// Lock and sleep are state, greeting_seq is an event. The first value on a connection only sets the seq baseline,
// because it may carry a greeting that was already played, but a baseline that is locked or asleep still arms.
export function withSession(state, session, connection) {
  if (!state.enabled) {
    return state;
  }
  const seen = { connection, seq: session.greeting_seq };
  const isBaseline = state.seen === null || state.seen.connection !== connection;
  const isNew = !isBaseline && session.greeting_seq > state.seen.seq;
  if (session.locked || session.sleeping) {
    // A greeting that is cut short by a lock must wait for the next unlock and start over.
    return withPhase(state, 'armed', null, true, seen);
  }
  // A new seq while playing is absorbed on purpose: the daemon bumps it only at an unlock or a wake, which a play
  // cannot overlap unless a lock re-armed it first.
  if (state.phase === 'playing') {
    return withPhase(state, 'playing', state.kind, state.fromDots, seen);
  }
  if (isNew) {
    return withPhase(state, 'playing', session.greeting ?? 'unlock', state.phase === 'armed', seen);
  }
  return withPhase(state, 'idle', null, false, seen);
}

// Nothing would ever tell an armed overlay about the unlock once the daemon is gone.
export function disconnected(state) {
  return state.phase === 'armed' ? withPhase(state, 'idle', null, false, state.seen) : state;
}

export function finished(state) {
  return state.phase === 'playing' ? withPhase(state, 'idle', null, false, state.seen) : state;
}

export function skipped(state) {
  return finished(state);
}
