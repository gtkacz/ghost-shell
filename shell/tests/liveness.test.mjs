import test from 'node:test';
import assert from 'node:assert/strict';
import { atConnect, isLive, sourceLive, withTopicState } from '../kit/GhostKit/lib/liveness.mjs';

const TOPIC = 'weather.alerts';
const OTHER = 'net.vpn';

const state = (live, topic = TOPIC) => ({ t: 'topic_state', topic, live });
const received = (messages) => messages.reduce(withTopicState, atConnect());
const liveIn = (record, connected = true, stale = false) => isLive(connected, stale, sourceLive(record, TOPIC));

test('a topic is live while no topic_state has arrived for it', () => {
  assert.equal(liveIn(atConnect()), true);
  assert.equal(liveIn(received([state(false, OTHER)])), true);
});

test('a topic is not live after live: false', () => assert.equal(liveIn(received([state(false)])), false));

test('a topic is live again after live: true', () => assert.equal(liveIn(received([state(false), state(true)])), true));

test('a reset at connect returns every topic to live', () => {
  const dead = received([state(false), state(false, OTHER)]);
  assert.equal(liveIn(dead), false);

  const reconnected = atConnect();

  assert.equal(liveIn(reconnected), true);
  assert.equal(sourceLive(reconnected, OTHER), true);
});

test('a topic is not live while disconnected or stale, whatever its topic_state', () => {
  for (const record of [atConnect(), received([state(true)]), received([state(false)])]) {
    assert.equal(liveIn(record, false, false), false);
    assert.equal(liveIn(record, true, true), false);
    assert.equal(liveIn(record, false, true), false);
  }
});

test('a topic_state leaves the record it was given unchanged', () => {
  const before = received([state(true)]);

  withTopicState(before, state(false));

  assert.equal(sourceLive(before, TOPIC), true);
});
