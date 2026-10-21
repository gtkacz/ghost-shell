// A record of each topic's last topic_state on one connection. The daemon sends one only for topics a
// provider has claimed, so a topic without one is as live as the daemon.
export function atConnect() {
  return new Map();
}

export function withTopicState(record, message) {
  return new Map(record).set(message.topic, message.live === true);
}

export function sourceLive(record, topic) {
  return record.get(topic) !== false;
}

export function isLive(connected, stale, sourceIsLive) {
  return connected && !stale && sourceIsLive;
}
