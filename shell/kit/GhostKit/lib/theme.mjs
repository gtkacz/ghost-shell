const OPAQUE_PERCENT = 100;
const ALPHA_BYTE_MAX = 255;
const ALPHA_PER_PERCENT = ALPHA_BYTE_MAX / OPAQUE_PERCENT;
const MONOSPACE_TOLERANCE_PX = 0.5;
const NO_TAG = '';
const HEX_BYTE_WIDTH = 2;
const RGB_HEX_LENGTH = 6;
const ZERO_PAD = '0';
const MS_PER_SECOND = 1000;
const MIN_TIMING_MS = 1;
const MAX_TIMING_MS = 2 ** 31 - 1;

const FIELD_BY_MODE = Object.freeze({ mixed: 'kanji', kanji: 'kanji', katakana: 'katakana' });
// Chips read as status words rather than panel titles, so `mixed` takes the katakana for them (spec §5.3).
const CHIP_FIELD_BY_MODE = Object.freeze({ mixed: 'katakana', kanji: 'kanji', katakana: 'katakana' });

export const TAGS = Object.freeze({
  cpu: Object.freeze({ kanji: '演算', katakana: 'コア' }),
  gpu: Object.freeze({ kanji: '描画', katakana: 'グラフィック' }),
  mem: Object.freeze({ kanji: '記憶', katakana: 'メモリ' }),
  disk: Object.freeze({ kanji: '記録', katakana: 'ストレージ' }),
  sens: Object.freeze({ kanji: '温度', katakana: 'センサー' }),
  clock: Object.freeze({ kanji: '時刻', katakana: 'タイム' }),
  wx: Object.freeze({ kanji: '気象', katakana: 'ウェザー' }),
  media: Object.freeze({ kanji: '音声', katakana: 'シグナル' }),
  notif: Object.freeze({ kanji: '通知', katakana: 'ログ' }),
  net: Object.freeze({ kanji: '通信', katakana: 'ネット' }),
  scope: Object.freeze({ kanji: '衛星', katakana: 'アップリンク' }),
  die: Object.freeze({ kanji: '回路', katakana: 'シリコン' }),
  tree: Object.freeze({ kanji: '系譜', katakana: 'プロセス' }),
  telem: Object.freeze({ kanji: '遠隔', katakana: 'テレメトリ' }),
  switch: Object.freeze({ kanji: '窓切替', katakana: 'スイッチ' }),
});

export const CHIPS = Object.freeze({
  main: Object.freeze({ kanji: '主画面', katakana: 'メイン' }),
  mon: Object.freeze({ kanji: '監視', katakana: 'モニター' }),
  live: Object.freeze({ kanji: '実況', katakana: 'ライブ' }),
  online: Object.freeze({ kanji: '接続', katakana: 'オンライン' }),
  play: Object.freeze({ kanji: '再生', katakana: 'プレイ' }),
  alert: Object.freeze({ kanji: '警報', katakana: 'アラート' }),
  crit: Object.freeze({ kanji: '危険', katakana: 'クリティカル' }),
  pause: Object.freeze({ kanji: '一時停止', katakana: 'ポーズ' }),
  stop: Object.freeze({ kanji: '停止', katakana: 'ストップ' }),
});

function has(object, key) {
  return Object.prototype.hasOwnProperty.call(object, key);
}

export function tag(key, mode) {
  if (!has(TAGS, key) || !has(FIELD_BY_MODE, mode)) {
    return NO_TAG;
  }
  return TAGS[key][FIELD_BY_MODE[mode]];
}

export function chip(key, mode) {
  if (!has(CHIPS, key) || !has(CHIP_FIELD_BY_MODE, mode)) {
    return NO_TAG;
  }
  return CHIPS[key][CHIP_FIELD_BY_MODE[mode]];
}

// The alpha byte withAlpha() writes for `pct`, for shaders that take the byte rather than the colour.
export function alphaByte(pct) {
  return Math.min(ALPHA_BYTE_MAX, Math.max(0, Math.round(pct * ALPHA_PER_PERCENT)));
}

// QML hands over a `color` whose string form is #RRGGBB, or #AARRGGBB when translucent; keeping the last six digits
// replaces any existing alpha.
export function withAlpha(hex, pct) {
  const rgb = String(hex).replace('#', '').slice(-RGB_HEX_LENGTH);
  const alpha = alphaByte(pct);
  const alphaHex = (ZERO_PAD + alpha.toString(16)).slice(-HEX_BYTE_WIDTH);
  return ('#' + alphaHex + rgb).toUpperCase();
}

// Digits of unequal advance width mean the configured font is proportional, so numbers would shift.
export function isMonospace(width0, width1) {
  return Math.abs(width0 - width1) < MONOSPACE_TOLERANCE_PX;
}

// A timing the config gives in seconds, as the whole milliseconds the shell counts in. A value that rounds to nothing
// still counts as one, so a timing the daemon accepts can never become a divisor of zero. One too long for a QML int
// or a Timer interval, which hold 32 bits, counts as the longest they hold (about 25 days).
export function secondsToMs(seconds) {
  return Math.min(MAX_TIMING_MS, Math.max(MIN_TIMING_MS, Math.round(seconds * MS_PER_SECOND)));
}

// appearance.dots.motion, in the order of dotgrid.frag's `mode` switch. A timed motion moves by itself and so holds the
// frame clock; the cursor motion moves only with the pointer.
const DOT_MOTIONS = Object.freeze(['none', 'breathe', 'sweep', 'ripple', 'clouds', 'rain', 'packets', 'interference', 'cursor']);
const UNTIMED_DOT_MOTIONS = Object.freeze(['none', 'cursor']);
const POINTER_DOT_MOTION = 'cursor';

// A name the daemon would reject, or none from a daemon older than the key, keeps the grid still.
export function dotMotion(name) {
  const mode = Math.max(0, DOT_MOTIONS.indexOf(name));
  const known = DOT_MOTIONS[mode];
  return { mode, timed: !UNTIMED_DOT_MOTIONS.includes(known), pointer: known === POINTER_DOT_MOTION };
}
