import test from 'node:test';
import assert from 'node:assert/strict';
import { ATTENTION_MS, attentionLeftMs, firstShown } from '../kit/GhostKit/lib/attention.mjs';

const T0 = 1_000_000;

test('an alert shown for the first time has its whole attention window left', () => {
  const seen = new Map();
  assert.equal(attentionLeftMs(firstShown(seen, 'a', T0), T0), ATTENTION_MS);
});

test('the attention window is 10 seconds', () => assert.equal(ATTENTION_MS, 10_000));

test('the window runs out after 10 seconds and not before', () => {
  assert.equal(attentionLeftMs(T0, T0 + ATTENTION_MS - 1), 1);
  assert.equal(attentionLeftMs(T0, T0 + ATTENTION_MS), 0);
  assert.equal(attentionLeftMs(T0, T0 + 5 * ATTENTION_MS), 0);
});

test('the same alert shown again later does not get a new window', () => {
  const seen = new Map();
  firstShown(seen, 'a', T0);
  assert.equal(firstShown(seen, 'a', T0 + 60_000), T0);
  assert.equal(attentionLeftMs(firstShown(seen, 'a', T0 + 60_000), T0 + 60_000), 0);
});

test('the same alert shown again inside the window keeps what is left of it', () => {
  const seen = new Map();
  firstShown(seen, 'a', T0);
  assert.equal(attentionLeftMs(firstShown(seen, 'a', T0 + 4_000), T0 + 4_000), ATTENTION_MS - 4_000);
});

test('a new alert gets its own window while an old one stays still', () => {
  const seen = new Map();
  firstShown(seen, 'old', T0);
  const later = T0 + 60_000;
  assert.equal(attentionLeftMs(firstShown(seen, 'old', later), later), 0);
  assert.equal(attentionLeftMs(firstShown(seen, 'new', later), later), ATTENTION_MS);
});

test('a clock that steps back does not give more than the whole window', () => {
  assert.equal(attentionLeftMs(T0, T0 - 5_000), ATTENTION_MS);
});
