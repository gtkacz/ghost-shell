import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  CELLS,
  actionLabel,
  actionsFor,
  alertDelay,
  alertOpacity,
  cells,
  failureText,
  metaText,
  minuteDelay,
  nextLayout,
  nextUser,
  loginUser,
  clampIndex,
  parseSnapshot,
  readSnapshot,
  resolveStyle,
  scanStep,
  sessionTag,
} from '../gate/GhostGate/lib/gate.mjs';
import { ARRIVAL_MS, ARRIVAL_OPACITIES, ARRIVAL_START_PERCENTS } from '../kit/GhostKit/lib/attention.mjs';
import { pickScreen } from '../kit/GhostKit/lib/scale.mjs';

test('cells lights one cell per character and parks the cursor on the next one', () => {
  assert.equal(CELLS, 28);
  assert.deepEqual(cells(0), { lit: 0, cursor: 0 });
  assert.deepEqual(cells(5), { lit: 5, cursor: 5 });
  assert.deepEqual(cells(27), { lit: 27, cursor: 27 });
  assert.deepEqual(cells(28), { lit: 28, cursor: 27 });
  assert.deepEqual(cells(40), { lit: 28, cursor: 27 });
});

test('parseSnapshot fills the default palette from an empty snapshot', () => {
  const snapshot = parseSnapshot({});
  assert.equal(snapshot.style, 'prompt');
  assert.equal(snapshot.user, '');
  assert.equal(snapshot.appearance.colors.ground, '#0a0f11');
  assert.equal(snapshot.appearance.colors.signal, '#a3d4cc');
  assert.equal(snapshot.appearance.panel_fill, 55);
});

test('parseSnapshot reads values from the strings QSettings hands over', () => {
  const snapshot = parseSnapshot({
    gate: { style: 'prompt', host: 'hal', display_model: 'M27Q' },
    appearance: { font: 'Other Mono', panel_fill: '40', japanese: 'off', alert_stripe_period_s: '2.5' },
    colors: { ground: '#112233' },
    dots: { spacing: '30' },
    scanlines: { opacity: '20' },
  });
  assert.equal(snapshot.host, 'hal');
  assert.equal(snapshot.displayModel, 'M27Q');
  assert.equal(snapshot.appearance.font, 'Other Mono');
  assert.equal(snapshot.appearance.panel_fill, 40);
  assert.equal(snapshot.appearance.japanese, 'off');
  assert.equal(snapshot.appearance.alert_stripe_period_s, 2.5);
  assert.equal(snapshot.appearance.colors.ground, '#112233');
  assert.equal(snapshot.appearance.dots.spacing, 30);
  assert.equal(snapshot.appearance.scanlines.opacity, 20);
});

test('parseSnapshot keeps the default for a value it cannot use', () => {
  assert.equal(parseSnapshot({ gate: { style: 'fancy' } }).style, 'prompt');
  assert.equal(parseSnapshot({ colors: { ground: 'red' } }).appearance.colors.ground, '#0a0f11');
  assert.equal(parseSnapshot({ appearance: { panel_fill: 'abc' } }).appearance.panel_fill, 55);
  assert.equal(parseSnapshot({ appearance: { japanese: 'pirate' } }).appearance.japanese, 'mixed');
});

test('readSnapshot asks for every snapshot key as section/key and parses the answers', () => {
  const ini = {
    'gate/display_model': 'M27Q',
    'gate/host': 'hal',
    'gate/user': 'gtkacz',
    'colors/ground': '#112233',
    'dots/spacing': '30',
  };
  const asked = [];
  const snapshot = readSnapshot((key) => {
    asked.push(key);
    return ini[key];
  });
  assert.equal(snapshot.displayModel, 'M27Q');
  assert.equal(snapshot.host, 'hal');
  assert.equal(snapshot.user, 'gtkacz');
  assert.equal(snapshot.appearance.colors.ground, '#112233');
  assert.equal(snapshot.appearance.dots.spacing, 30);
  assert.equal(snapshot.appearance.panel_fill, 55);
  for (const key of ['gate/style', 'appearance/japanese', 'colors/burn', 'dots/cover', 'scanlines/opacity']) {
    assert.ok(asked.includes(key), key);
  }
});

test('resolveStyle resolves every name to prompt for now', () => {
  assert.equal(resolveStyle('terminal'), 'prompt');
  assert.equal(resolveStyle('nonsense'), 'prompt');
});

test('failureText names the attempt, or upper-cases the system message', () => {
  assert.equal(failureText(2, ''), 'ACCESS DENIED · ATTEMPT 2');
  assert.equal(failureText(3, 'Account locked for 10 minutes'), 'ACCOUNT LOCKED FOR 10 MINUTES');
});

test('alertOpacity follows the arrival tables, then blinks, then holds', () => {
  const percentOf = (ms) => (100 * ms) / ARRIVAL_MS;
  for (let ms = 0; ms < ARRIVAL_MS; ms += 40) {
    let expected = ARRIVAL_OPACITIES[0];
    ARRIVAL_START_PERCENTS.forEach((start, i) => {
      if (percentOf(ms) >= start) {
        expected = ARRIVAL_OPACITIES[i];
      }
    });
    assert.equal(alertOpacity(ms), expected, `at ${ms} ms`);
  }
  assert.equal(alertOpacity(1600), 1);
  assert.equal(alertOpacity(1850), 0.5);
  assert.equal(alertOpacity(2100), 0);
  assert.equal(alertOpacity(2350), 0.5);
  assert.equal(alertOpacity(2600), 1);
  assert.equal(alertOpacity(9600), 1);
  assert.equal(alertOpacity(9850), 1);
  assert.equal(alertOpacity(60000), 1);
});

test('actionsFor offers the actions the mode and the system allow', () => {
  const all = { switchUser: true, sleep: true, restart: true, shutdown: true };
  assert.deepEqual(actionsFor('lock', all), ['switch-user', 'sleep']);
  assert.deepEqual(actionsFor('login', all), ['sleep', 'restart', 'power']);
  assert.deepEqual(actionsFor('lock', { switchUser: false, sleep: true }), ['sleep']);
  assert.deepEqual(actionsFor('login', { sleep: true, restart: false, shutdown: true }), ['sleep', 'power']);
});

test('nextLayout and nextUser wrap around', () => {
  assert.equal(nextLayout(['us', 'br'], 'us'), 'br');
  assert.equal(nextLayout(['us', 'br'], 'br'), 'us');
  assert.equal(nextLayout(['us'], 'us'), 'us');
  assert.equal(nextUser(['a', 'b', 'c'], 'c'), 'a');
  assert.equal(nextUser(['a', 'b', 'c'], 'a'), 'b');
});

test('scanStep maps the sweep time onto 0 to 30', () => {
  assert.equal(scanStep(0), 0);
  assert.equal(scanStep(400), 15);
  assert.equal(scanStep(800), 30);
  assert.equal(scanStep(5000), 30);
});

test('pickScreen returns the only screen when its model matches', () => {
  const screens = [{ model: 'X' }];
  assert.equal(pickScreen(screens, 'M27Q'), screens[0]);
});

test('the gate defaults for dots and scanlines equal GhostKit Theme.qml defaultAppearance', () => {
  const source = readFileSync(new URL('../kit/GhostKit/Theme.qml', import.meta.url), 'utf8');
  const block = (name) => {
    const body = source.match(new RegExp(`\\b${name}: \\{([^}]*)\\}`))[1];
    return Object.fromEntries([...body.matchAll(/(\w+): ([\d.]+)\b/g)].map(([, key, value]) => [key, Number(value)]));
  };
  const { appearance } = parseSnapshot({});
  for (const name of ['dots', 'scanlines']) {
    const kit = block(name);
    // speed only drives motion, which the resting frame never has.
    for (const [key, value] of Object.entries(kit).filter(([key]) => key in appearance[name])) {
      assert.equal(appearance[name][key], value, `${name}.${key}`);
    }
  }
  assert.equal(appearance.dots.scale, 1);
});

test('parseSnapshot reads the dot strengths', () => {
  const { dots } = parseSnapshot({ dots: { size_range: '3', brightness: '2.2', cover: '0.7', scale: '1.5' } }).appearance;
  assert.deepEqual([dots.size_range, dots.brightness, dots.cover, dots.scale], [3, 2.2, 0.7, 1.5]);
});

test('alertDelay runs to each arrival step, then each blink step, then stops', () => {
  assert.equal(alertDelay(0), 96);
  assert.equal(alertDelay(96), 64);
  assert.equal(alertDelay(1119), 1);
  assert.equal(alertDelay(1120), 480);
  assert.equal(alertDelay(1600), 250);
  assert.equal(alertDelay(1700), 150);
  assert.equal(alertDelay(9599), 1);
  assert.equal(alertDelay(9600), 0);
});

test('action labels, session tags and the meta line', () => {
  assert.equal(actionLabel('power'), 'SHUT DOWN');
  assert.equal(actionLabel('switch-user'), 'SWITCH USER');
  assert.equal(sessionTag('Plasma (Wayland)'), 'Plasma · Wayland');
  assert.equal(sessionTag('Plasma'), 'Plasma');
  assert.equal(metaText('lock', new Date(2026, 9, 23, 7, 5)), 'LOCKED 07:05');
  assert.equal(metaText('login', new Date(2026, 9, 23, 17, 45)), '17:45');
  assert.equal(minuteDelay(new Date(2026, 9, 23, 7, 5, 13, 500)), 46_500);
});

test('loginUser keeps the last user when listed and otherwise falls back to the first', () => {
  assert.equal(loginUser('b', ['a', 'b']), 'b');
  assert.equal(loginUser('', ['a', 'b']), 'a');
  assert.equal(loginUser('gone', ['a', 'b']), 'a');
  assert.equal(loginUser('', []), '');
});

test('clampIndex keeps an index inside the list', () => {
  assert.equal(clampIndex(1, 3), 1);
  assert.equal(clampIndex(-1, 3), 0);
  assert.equal(clampIndex(7, 3), 2);
  assert.equal(clampIndex(0, 0), 0);
});
