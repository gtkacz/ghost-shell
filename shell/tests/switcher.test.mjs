import test from 'node:test';
import assert from 'node:assert/strict';
import { appName, rowNumber, titleWithoutApp } from '../gate/GhostGate/lib/switcher.mjs';

for (const [desktop, cls, caption, want] of [
  ['com.mitchellh.ghostty', 'com.mitchellh.ghostty', 'zsh', 'GHOSTTY'],
  ['thorium-browser', 'thorium-browser', 'x', 'THORIUM'],
  ['org.kde.dolphin', 'dolphin', 'Downloads — Dolphin', 'DOLPHIN'],
  ['org.kde.dolphin.desktop', 'dolphin', 'x', 'DOLPHIN'],
  ['code', 'Code', 'x', 'CODE'],
  ['', 'Slack', '#general - Slack', 'SLACK'],
  ['thorium-hnpfjngllnobngcgfapefoaidbinmjnm-Default', 'thorium-browser', 'WhatsApp', 'WHATSAPP'],
  ['thorium-hnpfjngllnobngcgfapefoaidbinmjnm-Default', 'thorium-browser', '', 'WINDOW'],
  ['', '', 'Some Window', 'SOME WINDOW'],
  ['', '', '', 'WINDOW'],
  ['google-chrome', 'Google-chrome', 'x', 'GOOGLE CHROME'],
  ['google-chrome-stable', 'Google-chrome', 'x', 'GOOGLE CHROME'],
  ['', 'sublime_text', 'x', 'SUBLIME TEXT'],
  ['', 'steam_app_570', 'Dota 2', 'DOTA 2'],
  ['steam_app_570', 'steam_app_570', 'Dota 2', 'DOTA 2'],
  ['', 'steam_app_570', '', 'WINDOW'],
]) {
  test(`appName of ${desktop || cls || caption || 'nothing'} is ${want}`, () => {
    assert.equal(appName(desktop, cls, caption), want);
  });
}

for (const [caption, app, want] of [
  ['Toast.qml — ghost-shell — Visual Studio Code', 'CODE', 'Toast.qml — ghost-shell'],
  ['KWin/TabBox – Mozilla Firefox', 'FIREFOX', 'KWin/TabBox'],
  ['#general - Slack', 'SLACK', '#general'],
  ['New Tab - Google Chrome', 'GOOGLE CHROME', 'New Tab'],
  ['Derezzed - Daft Punk', 'SPOTIFY', 'Derezzed - Daft Punk'],
  ['Slack', 'SLACK', 'Slack'],
  ['', 'CODE', ''],
]) {
  test(`titleWithoutApp keeps "${want}" of "${caption}"`, () => {
    assert.equal(titleWithoutApp(caption, app), want);
  });
}

test('rowNumber pads to two digits', () => {
  assert.equal(rowNumber(0), '01');
  assert.equal(rowNumber(11), '12');
  assert.equal(rowNumber(99), '100');
});
