import test from 'node:test';
import assert from 'node:assert/strict';
import { computeRects } from '../kit/GhostKit/lib/layout.mjs';

const slot = (widget, size = null, settings = {}) => ({ widget, source: 'builtin', size, settings, requires: [], animates: false, error: null });
const tab = (widget, title, settings = {}) => ({ widget, title, source: 'builtin', settings, error: null });

const DEFAULT_V1_FOR_TEST = {
  direction: 'row',
  size: null,
  children: [
    { direction: 'column', size: 300, children: [slot('processor', 250), slot('graphics', 204), slot('memory')] },
    {
      direction: 'column',
      size: null,
      children: [
        slot('clock', 262, { style: 'hero' }),
        { direction: 'row', size: null, children: [slot('media', 290, { style: 'stack' }), slot('placeholder', null, { label: 'Notifications' })] },
      ],
    },
    {
      direction: 'column',
      size: 300,
      children: [
        {
          direction: 'column',
          size: 332,
          children: [
            { size: null, default: 0, tabs: [tab('uplink', 'UPLINK'), tab('silicon', 'SILICON')] },
            slot('network', 109, { style: 'scope' }),
          ],
        },
        slot('storage'),
        slot('sensors', 158),
      ],
    },
  ],
};

const DEFAULT_V2_FOR_TEST = {
  direction: 'row',
  size: null,
  children: [
    { direction: 'column', size: 300, children: [slot('processor', 250), slot('graphics', 204), slot('memory')] },
    {
      direction: 'column',
      size: null,
      children: [
        slot('clock', 150, { style: 'band' }),
        {
          size: null,
          default: 0,
          tabs: [
            tab('uplink', 'UPLINK'),
            tab('silicon', 'SILICON'),
          ],
        },
        slot('media', 120, { style: 'strip' }),
      ],
    },
    { direction: 'column', size: 300, children: [slot('placeholder', 290, { label: 'Notifications' }), slot('network', 164), slot('storage')] },
  ],
};

const DEFAULT_MONITOR_FOR_TEST = {
  direction: 'column',
  size: null,
  children: [
    { direction: 'row', size: 104, children: [slot('clock', 250, { style: 'compact' }), slot('weather'), slot('media', 340, { style: 'compact' })] },
    {
      direction: 'row',
      size: null,
      children: [
        { direction: 'column', size: null, gutter: 6, children: ['cpu', 'gpu', 'mem', 'net', 'disk', 'therm'].map((metric) => slot('telemetry-lane', null, { metric })) },
        { direction: 'column', size: 340, children: [slot('storage', 188), slot('sensors')] },
      ],
    },
  ],
};

// The network scope's natural height, measured in the shell (108.65 at u 1.739).
const NETWORK_SCOPE_HEIGHT = 109;

const pick = (rects, widget) => {
  const { x, y, w, h } = rects.find((rect) => rect.widget === widget);
  return { x, y, w, h };
};

test('a split gutter spaces only its own children', () => {
  const tree = { direction: 'column', gutter: 6, children: [{ widget: 'a' }, { direction: 'row', children: [{ widget: 'b' }, { widget: 'c' }] }] };
  const r = computeRects(tree, { x: 0, y: 0, w: 100, h: 106 }, 12);
  assert.equal(pick(r, 'a').h, 50);
  assert.equal(pick(r, 'b').y, 56);
  assert.equal(pick(r, 'c').x, 56);
});

test('a null gutter falls back to the layout gutter', () => {
  const tree = { direction: 'row', gutter: null, children: [{ widget: 'a' }, { widget: 'b' }] };
  const r = computeRects(tree, { x: 0, y: 0, w: 112, h: 10 }, 12);
  assert.equal(pick(r, 'b').x, 62);
});

test('a zero gutter butts siblings together', () => {
  const tree = { direction: 'row', gutter: 0, children: [{ widget: 'a' }, { widget: 'b' }] };
  const r = computeRects(tree, { x: 0, y: 0, w: 100, h: 10 }, 12);
  assert.equal(pick(r, 'b').x, 50);
});

test('default v1 geometry', () => {
  const r = computeRects(DEFAULT_V1_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12);
  assert.deepEqual(pick(r, 'processor'), { x: 12, y: 42, w: 300, h: 250 });
  assert.equal(pick(r, 'storage').h, 666 - 332 - 158 - 24);
});

test('default v1 flex columns fill the space between the fixed ones', () => {
  const r = computeRects(DEFAULT_V1_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12);
  const notifications = r.find((rect) => rect.settings.label === 'Notifications');
  assert.deepEqual(pick(r, 'clock'), { x: 324, y: 42, w: 632, h: 262 });
  assert.equal(notifications.x, 324 + 290 + 12);
  assert.equal(notifications.w, 632 - 290 - 12);
  assert.equal(pick(r, 'sensors').x, 12 + 1256 - 300);
  assert.equal(r.overflow, undefined);
});

test('v1 scope column', () => {
  const r = computeRects(DEFAULT_V1_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12);
  assert.equal(pick(r, 'network').h, NETWORK_SCOPE_HEIGHT);
  assert.equal(r.find((x) => x.tabs).h, 332 - 12 - NETWORK_SCOPE_HEIGHT);
});

test('default v2 geometry', () => {
  const r = computeRects(DEFAULT_V2_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12);
  assert.deepEqual(pick(r, 'clock'), { x: 324, y: 42, w: 632, h: 150 });
  assert.equal(r.find((x) => x.tabs).h, 372);
  assert.equal(r.overflow, undefined);
});

test('default monitor geometry', () => {
  const r = computeRects(DEFAULT_MONITOR_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12);
  assert.deepEqual(pick(r, 'weather'), { x: 274, y: 42, w: 642, h: 104 });
  assert.deepEqual(pick(r, 'media'), { x: 928, y: 42, w: 340, h: 104 });
  assert.deepEqual(pick(r, 'storage'), { x: 928, y: 158, w: 340, h: 188 });
  assert.deepEqual(pick(r, 'sensors'), { x: 928, y: 358, w: 340, h: 350 });
  assert.equal(r.overflow, undefined);
});

test('monitor lanes are six, 6 px apart', () => {
  const lanes = computeRects(DEFAULT_MONITOR_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12).filter((x) => x.widget === 'telemetry-lane');
  assert.equal(lanes.length, 6);
  assert.equal(lanes[1].y - (lanes[0].y + lanes[0].h), 6);
});

test('rects carry their path and the slot fields', () => {
  const r = computeRects(DEFAULT_V1_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12);
  const media = r.find((rect) => rect.widget === 'media');
  assert.equal(r[0].path, '[0][0]');
  assert.equal(media.path, '[1][1][0]');
  assert.equal(media.widget, 'media');
  assert.equal(media.source, 'builtin');
  assert.equal(media.error, null);
});

test('flex children share the remaining space equally', () => {
  const r = computeRects({ direction: 'row', children: [slot('a', 100), slot('b'), slot('c')] }, { x: 0, y: 0, w: 412, h: 50 }, 12);
  assert.equal(pick(r, 'b').w, 144);
  assert.equal(pick(r, 'c').w, 144);
  assert.equal(pick(r, 'c').x, 100 + 12 + 144 + 12);
});

test('a lone slot fills the rect', () => {
  const r = computeRects(slot('a'), { x: 3, y: 4, w: 50, h: 60 }, 12);
  assert.deepEqual(pick(r, 'a'), { x: 3, y: 4, w: 50, h: 60 });
  assert.equal(r[0].path, '');
});

test('overflow shrinks instead of overlapping', () => {
  const r = computeRects({ direction: 'column', children: [{ widget: 'a', size: 400 }, { widget: 'b', size: 400 }] }, { x: 0, y: 0, w: 100, h: 500 }, 12);
  assert.ok(r.overflow);
  assert.ok(r[1].y >= r[0].y + r[0].h);
});

test('overflowing fixed sizes scale proportionally and starve the flex children', () => {
  const r = computeRects({ direction: 'row', children: [slot('a', 300), slot('b', 100), slot('flex')] }, { x: 0, y: 0, w: 212, h: 10 }, 6);
  assert.equal(r.overflow, true);
  assert.equal(pick(r, 'a').w, 150);
  assert.equal(pick(r, 'b').w, 50);
  assert.equal(pick(r, 'flex').w, 0);
});

test('no rect ever has a negative size', () => {
  const tight = { direction: 'row', children: [slot('a', 50), slot('b'), slot('c', 50)] };
  for (const rect of computeRects(tight, { x: 0, y: 0, w: 10, h: -5 }, 12)) {
    assert.ok(rect.w >= 0);
    assert.ok(rect.h >= 0);
  }
});

test('tabs node is one rect', () => {
  const r = computeRects({ tabs: [{ title: 'A', widget: 'a' }, { title: 'B', widget: 'b' }], default: 1 }, { x: 0, y: 0, w: 300, h: 200 }, 12);
  assert.equal(r.length, 1);
  assert.equal(r[0].tabs.length, 2);
  assert.equal(r[0].default, 1);
});

test('a tabs rect fills its slot and lists the flat tab entries', () => {
  const tabs = { size: 200, default: 0, tabs: [{ title: 'A', widget: 'a', source: 'builtin', settings: { k: 1 }, error: null, extra: true }] };
  const r = computeRects({ direction: 'row', children: [tabs, slot('b')] }, { x: 0, y: 0, w: 412, h: 80 }, 12);
  assert.deepEqual({ x: r[0].x, y: r[0].y, w: r[0].w, h: r[0].h, path: r[0].path }, { x: 0, y: 0, w: 200, h: 80, path: '[0]' });
  assert.deepEqual(r[0].tabs, [{ title: 'A', widget: 'a', source: 'builtin', settings: { k: 1 }, error: null }]);
  assert.equal(r[1].widget, 'b');
});
