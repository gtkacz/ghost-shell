import test from 'node:test';
import assert from 'node:assert/strict';
import { TAGS, tag, chip, withAlpha, isMonospace } from '../kit/GhostKit/lib/theme.mjs';

test('tags', () => {
  assert.equal(tag('cpu', 'mixed'), '演算');
  assert.equal(tag('cpu', 'katakana'), 'コア');
  assert.equal(tag('cpu', 'off'), '');
});

test('mixed mode uses the kanji tags', () => {
  for (const key of Object.keys(TAGS)) {
    assert.equal(tag(key, 'mixed'), tag(key, 'kanji'));
  }
});

test('every key in the constraints table resolves in both scripts', () => {
  const expected = {
    cpu: ['演算', 'コア'],
    gpu: ['描画', 'グラフィック'],
    mem: ['記憶', 'メモリ'],
    disk: ['記録', 'ストレージ'],
    sens: ['温度', 'センサー'],
  };
  for (const [key, [kanji, katakana]] of Object.entries(expected)) {
    assert.equal(tag(key, 'kanji'), kanji);
    assert.equal(tag(key, 'katakana'), katakana);
  }
});

test('every panel tag in the constraints table resolves in both scripts', () => {
  const expected = {
    clock: ['時刻', 'タイム'],
    wx: ['気象', 'ウェザー'],
    media: ['音声', 'シグナル'],
    notif: ['通知', 'ログ'],
    net: ['通信', 'ネット'],
    scope: ['衛星', 'アップリンク'],
    die: ['回路', 'シリコン'],
    tree: ['系譜', 'プロセス'],
    telem: ['遠隔', 'テレメトリ'],
    switch: ['窓切替', 'スイッチ'],
  };
  for (const [key, [kanji, katakana]] of Object.entries(expected)) {
    assert.equal(tag(key, 'kanji'), kanji);
    assert.equal(tag(key, 'katakana'), katakana);
    assert.equal(tag(key, 'mixed'), kanji);
    assert.equal(tag(key, 'off'), '');
  }
});

test('new tags', () => {
  assert.equal(tag('clock', 'mixed'), '時刻');
  assert.equal(tag('wx', 'katakana'), 'ウェザー');
  assert.equal(tag('core', 'mixed'), '');
});

test('crit is a chip and not a panel tag', () => {
  assert.equal(tag('crit', 'mixed'), '');
  assert.equal(tag('crit', 'kanji'), '');
  assert.equal(tag('crit', 'katakana'), '');
});

test('chips are katakana in mixed', () => {
  assert.equal(chip('live', 'mixed'), 'ライブ');
  assert.equal(chip('live', 'kanji'), '実況');
  assert.equal(chip('crit', 'katakana'), 'クリティカル');
  assert.equal(chip('alert', 'off'), '');
  assert.equal(chip('nope', 'mixed'), '');
});

test('every chip in the constraints table resolves in both scripts', () => {
  const expected = {
    main: ['主画面', 'メイン'],
    mon: ['監視', 'モニター'],
    live: ['実況', 'ライブ'],
    online: ['接続', 'オンライン'],
    play: ['再生', 'プレイ'],
    alert: ['警報', 'アラート'],
    crit: ['危険', 'クリティカル'],
    pause: ['一時停止', 'ポーズ'],
    stop: ['停止', 'ストップ'],
  };
  for (const [key, [kanji, katakana]] of Object.entries(expected)) {
    assert.equal(chip(key, 'kanji'), kanji);
    assert.equal(chip(key, 'katakana'), katakana);
    assert.equal(chip(key, 'mixed'), katakana);
    assert.equal(chip(key, 'off'), '');
  }
});

test('an unknown chip has no text, including inherited object keys', () => {
  assert.equal(chip('constructor', 'kanji'), '');
  assert.equal(chip('live', 'nope'), '');
});

test('an unknown key has no tag, including inherited object keys', () => {
  assert.equal(tag('nope', 'kanji'), '');
  assert.equal(tag('constructor', 'kanji'), '');
});

test('alpha', () => assert.equal(withAlpha('#0a0f11', 55), '#8C0A0F11'));

test('withAlpha clamps the percentage to the byte range', () => {
  assert.equal(withAlpha('#a3d4cc', 0), '#00A3D4CC');
  assert.equal(withAlpha('#a3d4cc', 100), '#FFA3D4CC');
  assert.equal(withAlpha('#a3d4cc', 140), '#FFA3D4CC');
  assert.equal(withAlpha('#a3d4cc', -10), '#00A3D4CC');
});

test('withAlpha accepts a qml colour value', () => {
  const qmlColor = { toString: () => '#a3d4cc' };
  assert.equal(withAlpha(qmlColor, 55), '#8CA3D4CC');
});

test('withAlpha replaces the alpha of an #AARRGGBB input', () => {
  assert.equal(withAlpha('#ffa3d4cc', 55), '#8CA3D4CC');
  assert.equal(withAlpha({ toString: () => '#80a3d4cc' }, 6), '#0FA3D4CC');
});

test('isMonospace accepts widths within half a pixel', () => {
  assert.equal(isMonospace(8.4, 8.4), true);
  assert.equal(isMonospace(8.4, 8.8), true);
  assert.equal(isMonospace(8.4, 9.2), false);
  assert.equal(isMonospace(9.2, 8.4), false);
});
