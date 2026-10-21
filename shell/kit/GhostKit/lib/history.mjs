const CPU_TOPIC = 'cpu';
const GPU_TOPIC = 'gpu';
// History series, named as the daemon records them.
const LOAD_KEY = 'load';
const CCD_KEYS = Object.freeze(['ccd0', 'ccd1']);
const GPU_LOAD_KEY = 'gpu_load';
const HISTORY_KEYS = Object.freeze({
  [CPU_TOPIC]: Object.freeze([LOAD_KEY].concat(CCD_KEYS)),
  [GPU_TOPIC]: Object.freeze([GPU_LOAD_KEY]),
});

function hasHistory(topic) {
  return Object.prototype.hasOwnProperty.call(HISTORY_KEYS, topic);
}

// After a history the daemon replays each topic's last value, which the history already ends with;
// appending it again would shift every graph by one sample. A topic without samples in the history
// was never sampled, so no replay follows.
export function fromHistory(series) {
  const replaying = {};
  for (const topic of Object.keys(HISTORY_KEYS)) {
    replaying[topic] = HISTORY_KEYS[topic].some((key) => (series[key] || []).length > 0);
  }
  return { series, lastSeq: {}, replaying };
}

// The ticks a topic missed between two of its messages; more than the series holds would only evict
// each other. Without a previous message there is nothing to have missed.
export function gapLength(previousSeq, seq, length) {
  if (previousSeq === undefined) {
    return 0;
  }
  return Math.min(Math.max(0, seq - previousSeq - 1), length);
}

// As the daemon records them: the load of the first two CCDs, and null for a GPU that is absent.
function samplesOf(topic, data) {
  if (topic === GPU_TOPIC) {
    return { [GPU_LOAD_KEY]: data.present ? data.load : null };
  }
  const samples = { [LOAD_KEY]: data.load };
  const ccdCount = Math.min(data.ccds.length, CCD_KEYS.length);
  for (let i = 0; i < ccdCount; i += 1) {
    samples[CCD_KEYS[i]] = data.ccds[i].load;
  }
  return samples;
}

// Returns a new track: Data publishes `series` as is, so a published series is never changed.
export function withData(track, message, length) {
  const topic = message.topic;
  if (!hasHistory(topic)) {
    return track;
  }
  const lastSeq = Object.assign({}, track.lastSeq, { [topic]: message.seq });
  if (track.replaying[topic]) {
    return { series: track.series, lastSeq, replaying: Object.assign({}, track.replaying, { [topic]: false }) };
  }
  const gap = new Array(gapLength(track.lastSeq[topic], message.seq, length)).fill(null);
  const samples = samplesOf(topic, message.data);
  const series = Object.assign({}, track.series);
  for (const key of Object.keys(samples)) {
    series[key] = (series[key] || []).concat(gap, [samples[key]]).slice(-length);
  }
  return { series, lastSeq, replaying: track.replaying };
}
