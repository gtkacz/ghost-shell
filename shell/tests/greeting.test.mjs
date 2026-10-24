import test from 'node:test';
import assert from 'node:assert/strict';
import {
  EASE_OUT,
  disconnected,
  finished,
  frameAt,
  initialGreeting,
  restingFrame,
  sequence,
  skipped,
  subtitleText,
  typeText,
  withSession,
} from '../kit/GhostKit/lib/greeting.mjs';

const TUBE_END_MS = 1030;
const GLOW_START_MS = 130;
const GLOW_RUN_MS = 900;
const DOT_PRINT_MS = 900;
const TYPING_LEAD_MS = 350;
const TITLE_CHAR_MS = 62;
const BURN_MS = 550;
const LINE_PAUSE_MS = 420;
const HOLD_MS = 1500;
const SCAN_STEP_MS = 26;
const MIDDLE = () => 0.5;
const JUST_BELOW_ONE = 0.999999;

const CONNECTION = 1;
const ENABLED = initialGreeting({ mode: 'auto', greeted: true });

function session(fields) {
  return Object.assign({ id: '2', locked: false, sleeping: false, greeting: null, greeting_seq: 0 }, fields);
}

function typed(title, subtitle, random = MIDDLE) {
  return typeText(sequence({ fromDots: true }), { title, nameStart: 0, subtitle }, random);
}

test('ease-out matches CSS at its midpoint', () => {
  assert.ok(Math.abs(EASE_OUT(0.5) - 0.6846) < 0.001);
  assert.equal(EASE_OUT(0), 0);
  assert.equal(EASE_OUT(1), 1);
});

test('tube grows, opens and ends at 1030 ms', () => {
  const p = sequence({ fromDots: false });
  assert.ok(Math.abs(frameAt(p, 0).tube.lineScaleX - 0.05) < 1e-9);
  assert.equal(frameAt(p, 120).tube.lineScaleX, 1);
  assert.equal(frameAt(p, 430).tube.lineOpacity, 0);
  assert.equal(frameAt(p, 130 + 315).tube.glowInset, 0);
  assert.equal(frameAt(p, TUBE_END_MS - 1).tube !== null, true);
  assert.equal(frameAt(p, TUBE_END_MS).tube, null);
});

test('the tube line opens to 220 times its height and the glow fades out', () => {
  const p = sequence({ fromDots: false });
  assert.equal(frameAt(p, 129).tube.lineScaleY, 1);
  assert.equal(frameAt(p, 430).tube.lineScaleY, 220);
  assert.equal(frameAt(p, 130).tube.glowInset, 0.5);
  assert.equal(frameAt(p, 130).tube.glowOpacity, 1);
  assert.ok(Math.abs(frameAt(p, TUBE_END_MS - 1e-6).tube.glowOpacity) < 1e-3);
});

test('the glow tints the whole screen until its clip animation starts at 130 ms', () => {
  const p = sequence({ fromDots: false });
  for (const ms of [0, 60, 129.99]) {
    assert.equal(frameAt(p, ms).tube.glowInset, 0, `${ms}`);
    assert.equal(frameAt(p, ms).tube.glowOpacity, 1, `${ms}`);
  }
  assert.equal(frameAt(p, 130).tube.glowInset, 0.5);
  assert.equal(frameAt(p, 130).tube.glowOpacity, 1);
});

function glowMsAtEasedProgress(eased) {
  let low = 0;
  let high = 1;
  for (let i = 0; i < 60; i += 1) {
    const mid = (low + high) / 2;
    if (EASE_OUT(mid) < eased) {
      low = mid;
    } else {
      high = mid;
    }
  }
  return GLOW_START_MS + ((low + high) / 2) * GLOW_RUN_MS;
}

test('the glow reaches 0.9 opacity with its clip fully open at 35 % of the eased run', () => {
  const p = sequence({ fromDots: false });
  const peakMs = glowMsAtEasedProgress(0.35);
  const atPeak = frameAt(p, peakMs).tube;
  assert.ok(Math.abs(atPeak.glowOpacity - 0.9) < 1e-6, `${atPeak.glowOpacity}`);
  assert.ok(Math.abs(atPeak.glowInset) < 1e-6, `${atPeak.glowInset}`);
  const half = frameAt(p, (GLOW_START_MS + peakMs) / 2).tube;
  assert.ok(half.glowInset > 0 && half.glowInset < 0.5, `${half.glowInset}`);
  assert.ok(half.glowOpacity > 0.9 && half.glowOpacity < 1, `${half.glowOpacity}`);
  const after = frameAt(p, (peakMs + TUBE_END_MS) / 2).tube;
  assert.equal(after.glowInset, 0);
  assert.ok(after.glowOpacity > 0 && after.glowOpacity < 0.9, `${after.glowOpacity}`);
});

test('dot print holds each of 25 states for 36 ms', () => {
  const p = sequence({ fromDots: false });
  assert.equal(frameAt(p, TUBE_END_MS).dotStep, 0);
  assert.equal(frameAt(p, TUBE_END_MS + 36 * 12).dotStep, 12);
  assert.equal(frameAt(p, TUBE_END_MS + 36 * 24).dotStep, 24);
  assert.equal(p.typingStartMs, TUBE_END_MS + DOT_PRINT_MS + TYPING_LEAD_MS);
});

test('from dots skips the print', () => {
  const p = sequence({ fromDots: true });
  assert.equal(frameAt(p, 0).dotStep, 24);
  assert.equal(p.typingStartMs, TUBE_END_MS + TYPING_LEAD_MS);
});

test('roll band starts 350 ms before typing and wraps every 3400 ms', () => {
  const p = sequence({ fromDots: true });
  assert.equal(frameAt(p, 1029).rollTop, null);
  assert.equal(frameAt(p, 1030).rollTop, -140);
  assert.equal(frameAt(p, 1030 + 3400).rollTop, -140);
});

test('roll band travels to the design height in one period', () => {
  const p = sequence({ fromDots: true, designHeight: 900 });
  assert.equal(frameAt(p, 1030 + 1700).rollTop, (900 - 140) / 2);
  assert.ok(Math.abs(frameAt(p, 1030 + 3399.999).rollTop - 900) < 0.01);
});

test('typing waits 62 ms per character, more after a space', () => {
  const p = typeText(sequence({ fromDots: true }), { title: 'a b', nameStart: 2, subtitle: '' }, MIDDLE);
  const t = p.typingStartMs;
  assert.equal(frameAt(p, t).lines[0].shown, 1);
  assert.equal(frameAt(p, t + 62).lines[0].shown, 2);
  assert.equal(frameAt(p, t + 62 + 62 + 37.2 - 0.01).lines[0].shown, 2);
  assert.equal(frameAt(p, t + 62 + 62 + 37.2 + 0.01).lines[0].shown, 3);
});

test('jitter stays within 0.6 to 1.4 of the base', () => {
  for (const [random, expectedMs] of [[() => 0, TITLE_CHAR_MS * 0.6], [() => JUST_BELOW_ONE, TITLE_CHAR_MS * 1.4]]) {
    const { appearMs } = typed('abcd', '', random).lines[0];
    for (let i = 1; i < appearMs.length; i += 1) {
      assert.ok(Math.abs(appearMs[i] - appearMs[i - 1] - expectedMs) < 0.001, `${i}: ${appearMs[i] - appearMs[i - 1]}`);
    }
  }
});

test('burn settles over 550 ms with ease-out', () => {
  const p = typed('ab', '');
  const t = p.typingStartMs;
  assert.equal(frameAt(p, t).lines[0].burn[0], 0);
  assert.equal(frameAt(p, t + BURN_MS / 2).lines[0].burn[0], EASE_OUT(0.5));
  assert.equal(frameAt(p, t + BURN_MS).lines[0].burn[0], 1);
});

test('cursor moves to the subtitle 420 ms after line 1 ends', () => {
  const p = typed('ab', 'cd');
  const lineEndMs = p.lines[0].endMs;
  assert.equal(frameAt(p, p.typingStartMs - 1).cursorLine, null);
  assert.equal(frameAt(p, p.typingStartMs).cursorLine, 0);
  assert.equal(frameAt(p, lineEndMs + LINE_PAUSE_MS - 0.01).cursorLine, 0);
  assert.equal(frameAt(p, lineEndMs + LINE_PAUSE_MS).cursorLine, 1);
  assert.equal(p.lines[1].appearMs[0], lineEndMs + LINE_PAUSE_MS);
  assert.equal(frameAt(p, p.typingStartMs + 100).cursorPhaseMs, 100);
});

test('empty subtitle goes from the line pause to the hold', () => {
  const p = typed('ab', '');
  assert.equal(p.scanStartMs, p.lines[0].endMs + LINE_PAUSE_MS + HOLD_MS);
  assert.equal(frameAt(p, p.scanStartMs - 0.01).scanStep, null);
  assert.equal(frameAt(p, p.scanStartMs).scanStep, 0);
  assert.equal(frameAt(p, p.scanStartMs + 1000).cursorLine, 0);
});

test('hold runs 1500 ms after the subtitle ends', () => {
  const p = typed('ab', 'cd');
  assert.equal(p.scanStartMs, p.lines[1].endMs + HOLD_MS);
});

test('scan pass holds 31 states for 26 ms and then is done', () => {
  const p = typed('ab', 'cd');
  const start = p.scanStartMs;
  assert.equal(frameAt(p, start - 1).scanStep, null);
  assert.equal(frameAt(p, start).scanStep, 0);
  assert.equal(frameAt(p, start + SCAN_STEP_MS - 0.01).scanStep, 0);
  assert.equal(frameAt(p, start + SCAN_STEP_MS).scanStep, 1);
  assert.equal(frameAt(p, start + 780).scanStep, 30);
  assert.equal(frameAt(p, start + 805.99).done, false);
  assert.equal(frameAt(p, start + 806).done, true);
  assert.equal(frameAt(p, start + 806).scanStep, 30);
});

test('a frame before the text is typed shows no characters and no scan', () => {
  const p = sequence({ fromDots: true });
  const f = frameAt(p, p.typingStartMs + 10_000);
  assert.deepEqual([f.lines[0].shown, f.lines[1].shown], [0, 0]);
  assert.equal(f.scanStep, null);
  assert.equal(f.done, false);
});

test('text_is_fixed_when_typing_starts', () => {
  const head = sequence({ fromDots: true });
  const before = structuredClone(head);
  const title = 'Welcome back, gtkacz';
  const subtitle = 'おかえりなさい  ·  SESSION 0002';
  const p = typeText(head, { title, nameStart: 14, subtitle }, MIDDLE);
  assert.deepEqual(head, before);
  assert.equal(p.lines[0].text, title);
  assert.equal(p.lines[1].text, subtitle);
  for (let ms = 0; ms <= p.doneMs + 100; ms += 7) {
    const f = frameAt(p, ms);
    assert.ok(f.lines[0].shown <= title.length && f.lines[1].shown <= subtitle.length, `${ms}`);
    assert.equal(f.lines[0].burn.length, f.lines[0].shown);
    assert.equal(f.lines[1].burn.length, f.lines[1].shown);
  }
  const end = frameAt(p, p.doneMs);
  assert.deepEqual([end.lines[0].shown, end.lines[1].shown], [title.length, subtitle.length]);
});

test('resting frame is ground and every dot', () => {
  const f = restingFrame();
  assert.equal(f.dotStep, 24);
  assert.equal(f.tube, null);
  assert.equal(f.rollTop, null);
  assert.equal(f.scanStep, null);
  assert.equal(f.done, false);
  assert.deepEqual([f.lines[0].shown, f.lines[1].shown, f.cursorLine], [0, 0, null]);
});

test('subtitle parts and padding', () => {
  assert.equal(subtitleText({ japanese: 'mixed', sessionId: '2' }), 'おかえりなさい  ·  SESSION 0002');
  assert.equal(subtitleText({ japanese: 'off', sessionId: '2' }), 'SESSION 0002');
  assert.equal(subtitleText({ japanese: 'kanji', sessionId: '' }), 'おかえりなさい');
  assert.equal(subtitleText({ japanese: 'off', sessionId: '' }), '');
  assert.equal(subtitleText({ japanese: 'off', sessionId: 'c12345' }), 'SESSION c12345');
});

test('greeting reads unread before the widget', () => {
  assert.equal(subtitleText({ japanese: 'mixed', sessionId: '2', signals: 3 }), 'おかえりなさい  ·  SESSION 0002  ·  3 NEW SIGNALS');
  assert.equal(subtitleText({ japanese: 'off', sessionId: '', signals: 3 }), '3 NEW SIGNALS');
  assert.equal(subtitleText({ japanese: 'off', sessionId: '2', signals: 0 }), 'SESSION 0002');
});

test('login plays at start only without the marker', () => {
  assert.equal(initialGreeting({ mode: 'auto', greeted: false }).phase, 'playing');
  assert.equal(initialGreeting({ mode: 'auto', greeted: true }).phase, 'idle');
  assert.equal(initialGreeting({ mode: 'login', greeted: true }).kind, 'login');
  assert.equal(initialGreeting({ mode: 'off', greeted: false }).phase, 'idle');
  assert.equal(initialGreeting({ mode: 'auto', greeted: false }).fromDots, false);
});

test('arms on lock or sleep', () => {
  const locked = withSession(ENABLED, session({ locked: true }), CONNECTION);
  assert.equal(locked.phase, 'armed');
  assert.equal(locked.fromDots, true);
  assert.equal(withSession(ENABLED, session({ sleeping: true }), CONNECTION).phase, 'armed');
});

test('first value on a connection is a baseline', () => {
  const state = withSession(ENABLED, session({ greeting: 'unlock', greeting_seq: 5 }), CONNECTION);
  assert.equal(state.phase, 'idle');
  assert.deepEqual(state.seen, { connection: CONNECTION, seq: 5 });
  const reconnected = withSession(state, session({ greeting: 'unlock', greeting_seq: 9 }), CONNECTION + 1);
  assert.equal(reconnected.phase, 'idle');
});

test('a new greeting_seq plays from dots', () => {
  const armed = withSession(ENABLED, session({ locked: true, greeting_seq: 1 }), CONNECTION);
  const playing = withSession(armed, session({ greeting: 'unlock', greeting_seq: 2 }), CONNECTION);
  assert.equal(playing.phase, 'playing');
  assert.equal(playing.kind, 'unlock');
  assert.equal(playing.fromDots, true);
  const woken = withSession(armed, session({ greeting: 'wake', greeting_seq: 2 }), CONNECTION);
  assert.equal(woken.kind, 'wake');
});

test('a new greeting_seq with nothing armed plays without the dots', () => {
  const idle = withSession(ENABLED, session({ greeting_seq: 1 }), CONNECTION);
  const playing = withSession(idle, session({ greeting: 'unlock', greeting_seq: 2 }), CONNECTION);
  assert.equal(playing.phase, 'playing');
  assert.equal(playing.fromDots, false);
});

test('a repeated value does not restart a play', () => {
  const armed = withSession(ENABLED, session({ locked: true, greeting_seq: 1 }), CONNECTION);
  const playing = withSession(armed, session({ greeting: 'unlock', greeting_seq: 2 }), CONNECTION);
  const again = withSession(playing, session({ greeting: 'unlock', greeting_seq: 2 }), CONNECTION);
  assert.equal(again.phase, 'playing');
  assert.equal(again.kind, 'unlock');
  assert.equal(again.fromDots, true);
});

test('the login greeting survives the first session value', () => {
  const login = initialGreeting({ mode: 'auto', greeted: false });
  const after = withSession(login, session({ greeting_seq: 0 }), CONNECTION);
  assert.equal(after.phase, 'playing');
  assert.equal(after.kind, 'login');
  assert.deepEqual(after.seen, { connection: CONNECTION, seq: 0 });
});

test('disarms_when_neither_locked_nor_sleeping', () => {
  const armed = withSession(ENABLED, session({ locked: true, greeting_seq: 3 }), CONNECTION);
  const disarmed = withSession(armed, session({ greeting_seq: 3 }), CONNECTION);
  assert.equal(disarmed.phase, 'idle');
  assert.equal(withSession(armed, session({ sleeping: true, greeting_seq: 3 }), CONNECTION).phase, 'armed');
});

test('disconnect_disarms_an_armed_overlay', () => {
  const armed = withSession(ENABLED, session({ locked: true }), CONNECTION);
  assert.equal(disconnected(armed).phase, 'idle');
  const playing = initialGreeting({ mode: 'login', greeted: false });
  assert.equal(disconnected(playing), playing);
  assert.equal(disconnected(ENABLED), ENABLED);
});

test('lock_during_play_rearms', () => {
  const armed = withSession(ENABLED, session({ locked: true, greeting_seq: 1 }), CONNECTION);
  const playing = withSession(armed, session({ greeting: 'unlock', greeting_seq: 2 }), CONNECTION);
  const relocked = withSession(playing, session({ locked: true, greeting_seq: 2 }), CONNECTION);
  assert.equal(relocked.phase, 'armed');
  assert.equal(relocked.fromDots, true);
  const login = initialGreeting({ mode: 'login', greeted: false });
  assert.equal(withSession(login, session({ locked: true }), CONNECTION).phase, 'armed');
  const replayed = withSession(relocked, session({ greeting: 'unlock', greeting_seq: 3 }), CONNECTION);
  assert.equal(replayed.phase, 'playing');
});

test('skip_from_any_time_reveals_once', () => {
  const playing = initialGreeting({ mode: 'login', greeted: false });
  const idle = skipped(playing);
  assert.equal(idle.phase, 'idle');
  assert.equal(skipped(idle), idle);
  assert.equal(finished(playing).phase, 'idle');
  assert.equal(finished(idle), idle);
  const armed = withSession(ENABLED, session({ locked: true }), CONNECTION);
  assert.equal(skipped(armed), armed);
  assert.equal(finished(armed), armed);
});

test('off never arms', () => {
  const off = initialGreeting({ mode: 'off', greeted: false });
  assert.equal(withSession(off, session({ locked: true }), CONNECTION).phase, 'idle');
  const idle = withSession(off, session({ greeting_seq: 1 }), CONNECTION);
  assert.equal(withSession(idle, session({ greeting: 'unlock', greeting_seq: 2 }), CONNECTION).phase, 'idle');
});
