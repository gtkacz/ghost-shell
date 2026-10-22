const CPU_TOPIC = 'cpu';
const GPU_TOPIC = 'gpu';
const MEM_TOPIC = 'mem';
const SENSORS_TOPIC = 'sensors';
const DISKS_TOPIC = 'disks';
const NET_TOPIC = 'net';
// History series, named as the daemon records them.
const LOAD_KEY = 'load';
const CCD_KEYS = Object.freeze(['ccd0', 'ccd1']);
// Followed by the core's index in the cpu data's `cores`.
const CORE_KEY_PREFIX = 'core';
const CORE_KEY = new RegExp('^' + CORE_KEY_PREFIX + '[0-9]+$');
const GPU_LOAD_KEY = 'gpu_load';
const GPU_TEMP_KEY = 'temp_gpu';
const MEM_USED_KEY = 'mem_used';
const CPU_TEMP_KEY = 'temp_cpu';
const DISK_READ_KEY = 'disk_read';
const DISK_WRITE_KEY = 'disk_write';
const NVME_TEMP_KEY = 'temp_nvme';
// Every key but the per-core ones, with the topic that records it, as the daemon's history_topic.
const KEY_TOPICS = Object.freeze({
  [LOAD_KEY]: CPU_TOPIC,
  [CCD_KEYS[0]]: CPU_TOPIC,
  [CCD_KEYS[1]]: CPU_TOPIC,
  [GPU_LOAD_KEY]: GPU_TOPIC,
  [GPU_TEMP_KEY]: GPU_TOPIC,
  [MEM_USED_KEY]: MEM_TOPIC,
  [CPU_TEMP_KEY]: SENSORS_TOPIC,
  [DISK_READ_KEY]: DISKS_TOPIC,
  [DISK_WRITE_KEY]: DISKS_TOPIC,
  [NVME_TEMP_KEY]: DISKS_TOPIC,
  net_down: NET_TOPIC,
  net_up: NET_TOPIC,
});
// The daemon's CPU_PACKAGE_SENSORS.
const CPU_PACKAGE_SENSORS = Object.freeze(['k10temp/Tctl', 'coretemp/Package id 0']);
const NVME_KERNEL_NAME_PREFIX = 'nvme';
const LIVE_DRIVE_STATE = 'live';
const KIB_PER_GIB = 1048576;

function hasOwn(object, key) {
  return Object.prototype.hasOwnProperty.call(object, key);
}

// The topic a history series is recorded from, or null for a key no topic records.
export function topicOfKey(key) {
  if (CORE_KEY.test(key)) {
    return CPU_TOPIC;
  }
  return hasOwn(KEY_TOPICS, key) ? KEY_TOPICS[key] : null;
}

// After a history the daemon replays each topic's last value, which the history already ends with;
// appending it again would shift every graph by one sample. A topic without samples in the history
// was never published, so no replay follows.
export function fromHistory(series) {
  const replaying = {};
  for (const key of Object.keys(series)) {
    const topic = topicOfKey(key);
    if (topic !== null && series[key].length > 0) {
      replaying[topic] = true;
    }
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

// The load of the first two CCDs and of every core.
function cpuSamples(data) {
  const samples = { [LOAD_KEY]: data.load };
  const ccdCount = Math.min(data.ccds.length, CCD_KEYS.length);
  for (let i = 0; i < ccdCount; i += 1) {
    samples[CCD_KEYS[i]] = data.ccds[i].load;
  }
  data.cores.forEach((core, i) => {
    samples[CORE_KEY_PREFIX + i] = core.load;
  });
  return samples;
}

// Null for a GPU that is absent.
function gpuSamples(data) {
  return {
    [GPU_LOAD_KEY]: data.present ? data.load : null,
    [GPU_TEMP_KEY]: data.present ? data.temp_c : null,
  };
}

// The daemon keeps these as f32, so rounding them alike keeps the two series identical.
function memSamples(data) {
  return { [MEM_USED_KEY]: Math.fround(data.used_kib / KIB_PER_GIB) };
}

// The first CPU package reading, whatever its value.
function sensorsSamples(data) {
  const reading = data.temps.find((candidate) => CPU_PACKAGE_SENSORS.includes(candidate.id));
  return { [CPU_TEMP_KEY]: reading === undefined ? null : reading.value };
}

// The rates summed over the live drives, rounded to f32 as mem_used is, and the hottest live NVMe drive.
function disksSamples(data) {
  const live = data.drives.filter((drive) => drive.state === LIVE_DRIVE_STATE);
  const nvmeTemps = live
    .filter((drive) => drive.kernel_name.startsWith(NVME_KERNEL_NAME_PREFIX) && typeof drive.temp_c === 'number')
    .map((drive) => drive.temp_c);
  return {
    [DISK_READ_KEY]: Math.fround(live.reduce((sum, drive) => sum + drive.read_bps, 0)),
    [DISK_WRITE_KEY]: Math.fround(live.reduce((sum, drive) => sum + drive.write_bps, 0)),
    [NVME_TEMP_KEY]: nvmeTemps.length > 0 ? Math.max(...nvmeTemps) : null,
  };
}

// As the daemon records them, keyed by the topic they are recorded from.
const SAMPLES_OF = Object.freeze({
  [CPU_TOPIC]: cpuSamples,
  [GPU_TOPIC]: gpuSamples,
  [MEM_TOPIC]: memSamples,
  [SENSORS_TOPIC]: sensorsSamples,
  [DISKS_TOPIC]: disksSamples,
});

// Returns a new track: Data publishes `series` as is, so a published series is never changed.
export function withData(track, message, length) {
  const topic = message.topic;
  if (!hasOwn(SAMPLES_OF, topic)) {
    return track;
  }
  const lastSeq = Object.assign({}, track.lastSeq, { [topic]: message.seq });
  if (track.replaying[topic]) {
    return { series: track.series, lastSeq, replaying: Object.assign({}, track.replaying, { [topic]: false }) };
  }
  const gap = new Array(gapLength(track.lastSeq[topic], message.seq, length)).fill(null);
  const samples = SAMPLES_OF[topic](message.data);
  const series = Object.assign({}, track.series);
  for (const key of Object.keys(samples)) {
    series[key] = (series[key] || []).concat(gap, [samples[key]]).slice(-length);
  }
  return { series, lastSeq, replaying: track.replaying };
}
