// The CRT print of spec §6.1: 24 discrete steps, 640 ms to or from the monitor view and 380 ms
// between main views.
export const PRINT_STEPS = 24;
export const MONITOR_MS = 640;
export const MAIN_MS = 380;

// As the daemon's desktop topic names them.
export const MONITOR_ACTION = 'monitor';
export const SWAP_ACTION = 'swap';

export function initialViews(dashboard) {
  return { current: dashboard.default_view, lastMain: dashboard.default_view };
}

function change(state, current, lastMain, durationMs) {
  return current === state.current ? null : { current, lastMain, durationMs };
}

function toggleMonitor(state, dashboard) {
  const monitor = dashboard.monitor_view;
  if (monitor === '') {
    return null;
  }
  if (state.current === monitor) {
    return change(state, state.lastMain, state.lastMain, MONITOR_MS);
  }
  return change(state, monitor, state.current, MONITOR_MS);
}

// From lastMain rather than current, so a swap from the monitor view prints the main view after the
// one the monitor view was opened from.
function swapMain(state, dashboard) {
  const views = dashboard.main_views;
  const next = views[(views.indexOf(state.lastMain) + 1) % views.length];
  const durationMs = state.current === dashboard.monitor_view ? MONITOR_MS : MAIN_MS;
  return change(state, next, next, durationMs);
}

// Null means the action changes nothing. One that arrives while a print runs is dropped, not queued.
export function applyAction(state, action, dashboard, busy = false) {
  if (busy) {
    return null;
  }
  if (action === MONITOR_ACTION) {
    return toggleMonitor(state, dashboard);
  }
  if (action === SWAP_ACTION) {
    return swapMain(state, dashboard);
  }
  return null;
}

// After a config or layout change. A view that has gone, or that no shortcut could leave or come
// back to, is replaced at once, without a print, by the last main view, or the default one if that
// has gone too. The config and the layout arrive separately, so main_views may still list a view the
// new layout has dropped, which has gone even so.
export function reconcileViews(state, dashboard, viewNames) {
  const lastMain = dashboard.main_views.includes(state.lastMain) && viewNames.includes(state.lastMain) ? state.lastMain : dashboard.default_view;
  const isReachable = dashboard.main_views.includes(state.current) || state.current === dashboard.monitor_view;
  return {
    current: isReachable && viewNames.includes(state.current) ? state.current : lastMain,
    lastMain,
  };
}

export function revealRows(step, steps, height) {
  return Math.round(height * step / steps);
}

// The wait, from elapsedMs into the print, before step + 1, which is due at a whole millisecond
// rounded from its exact time. Measured from the start rather than from the last tick, because timers
// fire on the animation driver's ticks, a little late, and 24 late ticks would add up. elapsedMs is
// wall-clock time, and below 0 the clock has stepped back past the print's start: waiting for it to
// catch up would keep the views busy for the size of the step.
export function stepDelay(step, steps, durationMs, elapsedMs) {
  return Math.max(1, Math.round((step + 1) * durationMs / steps) - Math.max(0, elapsedMs));
}

// `seen` is the connection the last desktop value arrived on and its action_seq, or null before the
// first. The first value on each connection only sets the baseline: it may carry an action the
// daemon took before this shell connected, which must not replay, and every ghostd counts from 0,
// so a baseline kept across a daemon restart would swallow the new daemon's first actions.
export function acceptAction(seen, desktop, connection) {
  const isBaseline = seen === null || seen.connection !== connection;
  const isNew = !isBaseline && desktop.action_seq > seen.seq;
  return { seen: { connection, seq: desktop.action_seq }, action: isNew ? desktop.action : null };
}
