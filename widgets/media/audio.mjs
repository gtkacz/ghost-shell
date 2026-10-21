// The audio rows' logic, apart from QML so that node can test it. A node is a plain object made from a Quickshell
// PwNode: { isStream, isSink, isAudio, name, props }, where name is its node.name, known before it is bound, and props
// is its PipeWire property set, every value a string, which Quickshell reads only once the node is bound.

const PROCESS_ID = 'application.process.id';
const APP_NAME = 'application.name';
const NODE_NAME = 'node.name';
const CAPTURES_SINK = 'stream.capture.sink';
const PEAKS_ONLY = 'resample.peaks';
const PIPEWIRE_TRUE = 'true';
// The name Quickshell gives its level meters' streams, in any Quickshell process.
const QUICKSHELL_STREAM = 'quickshell';

const MIC_LIVE = 'MIC LIVE';
const IS_RECORDING = 'IS RECORDING';
const ARE_RECORDING = 'ARE RECORDING';
const SEPARATOR = ' · ';
const NAME_SEPARATOR = ', ';
const LIVE = 'LIVE';
const MUTED = 'MUTED';
const MISSING = '--';
const VOL = 'VOL';
const PERCENT = 100;

function text(value) {
  return typeof value === 'string' ? value : '';
}

// An audio stream that takes sound in: Quickshell marks a playback stream as a sink.
function isCapture(node) {
  return node.isStream && !node.isSink && node.isAudio;
}

// The streams the rows bind to read their properties: every capture but Quickshell's own level meters, which record
// nothing. Binding the shell's own meter is unsafe: destroying the binding and the meter together, as covering the
// dashboard does, sends PipeWire a destroy for a resource it already dropped with the stream, and Quickshell 0.2.1
// takes that error as fatal and stays disconnected from PipeWire.
export function needsBinding(node) {
  return isCapture(node) && node.name !== QUICKSHELL_STREAM;
}

function isRecording(node, ownPids) {
  const props = node.props || {};
  // A stream's properties arrive only once it is bound, and until then nothing says whose it is.
  if (!isCapture(node) || Object.keys(props).length === 0) {
    return false;
  }
  // A capture of an output's monitor, such as ghostd's spectrum, hears what plays rather than the microphone.
  if (props[CAPTURES_SINK] === PIPEWIRE_TRUE) {
    return false;
  }
  // A level meter's stream receives only peak levels, never the sound. The shell's own meter is one, and its node
  // carries no process id (only its client does), so the process ids alone would let it keep itself running.
  if (props[PEAKS_ONLY] === PIPEWIRE_TRUE) {
    return false;
  }
  return !ownPids.some(pid => String(pid) === props[PROCESS_ID]);
}

// The names of the apps recording, uppercased, each once, in the order their streams appear. Mute is not read: a muted
// microphone with an app recording is still live.
export function recordingApps(nodes, ownPids) {
  const names = [];
  for (const node of nodes || []) {
    if (!isRecording(node, ownPids || [])) {
      continue;
    }
    const name = (text(node.props[APP_NAME]) || text(node.props[NODE_NAME])).toUpperCase();
    if (!names.includes(name)) {
      names.push(name);
    }
  }
  return names;
}

// The shell opens a capture stream on the microphone only for the IN level, and only while another app records, the
// rows can be seen and the microphone is not muted (decision 2).
export function micLevelEnabled(apps, visible, muted) {
  return apps.length > 0 && visible && !muted;
}

export function micAlertText(apps) {
  if (apps.length === 0) {
    return '';
  }
  const verb = apps.length === 1 ? IS_RECORDING : ARE_RECORDING;
  return MIC_LIVE + SEPARATOR + apps.join(NAME_SEPARATOR) + ' ' + verb;
}

function percentText(volume) {
  return Number.isFinite(volume) ? String(Math.round(volume * PERCENT)) : MISSING;
}

export function volumeText(volume, muted) {
  if (muted) {
    return MUTED;
  }
  return Number.isFinite(volume) ? percentText(volume) + '%' : MISSING;
}

// The IN row's value: LIVE while any app records, muted or not.
export function inputText(apps, muted) {
  if (apps.length > 0) {
    return LIVE;
  }
  return muted ? MUTED : MISSING;
}

// The control row's reading of the output volume, as the mockup's `VOL 64`.
export function controlVolume(volume, muted) {
  return VOL + ' ' + (muted ? MUTED : percentText(volume));
}
