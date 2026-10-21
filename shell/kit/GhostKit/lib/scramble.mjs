export const SCRAMBLE_CHARS = '!<>-_/[]{}=+*^?#01ｱｲｳｴｵｶｷｸｹｺｻｼｽ';

const BLANK = ' ';
const FIRST_SETTLE = 0.3;
const SETTLE_SPAN = 0.7;
const SCRAMBLE_WINDOW = 0.45;

// `rand` is injected so a test can pin the noise; the caller owns the clock and passes the 0..1 progress.
export function scrambleFrame(target, progress, rand) {
  const count = target.length;
  let frame = '';
  for (let i = 0; i < count; i++) {
    const settleAt = FIRST_SETTLE + SETTLE_SPAN * (i / count);
    if (progress >= settleAt) {
      frame += target[i];
    } else if (progress > settleAt - SCRAMBLE_WINDOW) {
      frame += target[i] === BLANK ? BLANK : SCRAMBLE_CHARS[Math.floor(rand() * SCRAMBLE_CHARS.length)];
    } else {
      frame += BLANK;
    }
  }
  return frame;
}
