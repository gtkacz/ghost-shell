import test from 'node:test';
import assert from 'node:assert/strict';
import { SCRAMBLE_CHARS, scrambleFrame } from '../kit/GhostKit/lib/scramble.mjs';

const FIRST = () => 0;
const LAST = () => 0.999999;

test('scramble frames', () => {
  assert.equal(scrambleFrame('AB', 0, FIRST), '! ');
  assert.equal(scrambleFrame('AB', 0.5, FIRST), 'A!');
  assert.equal(scrambleFrame('AB', 1, FIRST), 'AB');
  assert.equal(scrambleFrame('A B', 0.5, FIRST), 'A !');
});

test('a character settles at its own threshold and not before', () => {
  assert.equal(scrambleFrame('AB', 0.3, FIRST), 'A!');
  assert.equal(scrambleFrame('AB', 0.64, FIRST), 'A!');
  assert.equal(scrambleFrame('AB', 0.66, FIRST), 'AB');
});

test('a character stays blank until its scramble window opens', () => {
  assert.equal(scrambleFrame('AB', -0.2, FIRST), '  ');
  assert.equal(scrambleFrame('AB', 0.1, FIRST), '! ');
});

test('the random source picks from the scramble alphabet', () => {
  assert.equal(scrambleFrame('A', 0, LAST), SCRAMBLE_CHARS[SCRAMBLE_CHARS.length - 1]);
  assert.equal(scrambleFrame('A', 0, () => 0.5), SCRAMBLE_CHARS[Math.floor(SCRAMBLE_CHARS.length / 2)]);
});

test('the alphabet is the mockup alphabet', () => {
  assert.equal(SCRAMBLE_CHARS, '!<>-_/[]{}=+*^?#01ｱｲｳｴｵｶｷｸｹｺｻｼｽ');
});

test('an empty target stays empty', () => {
  assert.equal(scrambleFrame('', 0.5, FIRST), '');
});

test('a Japanese target settles to itself', () => {
  assert.equal(scrambleFrame('時刻', 1, FIRST), '時刻');
});
