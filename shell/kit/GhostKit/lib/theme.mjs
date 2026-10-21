const OPAQUE_PERCENT = 100;
const ALPHA_BYTE_MAX = 255;
const ALPHA_PER_PERCENT = ALPHA_BYTE_MAX / OPAQUE_PERCENT;
const MONOSPACE_TOLERANCE_PX = 0.5;
const NO_TAG = '';
const HEX_BYTE_WIDTH = 2;
const ZERO_PAD = '0';

const FIELD_BY_MODE = Object.freeze({ mixed: 'kanji', kanji: 'kanji', katakana: 'katakana' });

export const TAGS = Object.freeze({
  cpu: Object.freeze({ kanji: '演算', katakana: 'コア' }),
  gpu: Object.freeze({ kanji: '描画', katakana: 'グラフィック' }),
  mem: Object.freeze({ kanji: '記憶', katakana: 'メモリ' }),
  disk: Object.freeze({ kanji: '記録', katakana: 'ストレージ' }),
  sens: Object.freeze({ kanji: '温度', katakana: 'センサー' }),
  crit: Object.freeze({ kanji: '危険', katakana: 'クリティカル' }),
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

export function withAlpha(hex, pct) {
  const alpha = Math.min(ALPHA_BYTE_MAX, Math.max(0, Math.round(pct * ALPHA_PER_PERCENT)));
  const alphaHex = (ZERO_PAD + alpha.toString(16)).slice(-HEX_BYTE_WIDTH);
  return ('#' + alphaHex + hex.replace('#', '')).toUpperCase();
}

// Digits of unequal advance width mean the configured font is proportional, so numbers would shift.
export function isMonospace(width0, width1) {
  return Math.abs(width0 - width1) < MONOSPACE_TOLERANCE_PX;
}
