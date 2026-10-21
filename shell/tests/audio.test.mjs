import test from 'node:test';
import assert from 'node:assert/strict';
import {
  recordingApps, micLevelEnabled, micAlertText, volumeText, inputText, controlVolume, needsBinding,
} from '../../widgets/media/audio.mjs';

const node = (props, extra = {}) => ({ isStream: true, isSink: false, isAudio: true, props, ...extra });
test('a capture stream is recording', () => assert.deepEqual(recordingApps([node({ 'application.name': 'Discord', 'application.process.id': '900' })], [42]), ['DISCORD']));
test('sink monitors and our own streams are not', () => assert.deepEqual(recordingApps([node({ 'application.name': 'ghostd', 'stream.capture.sink': 'true' }), node({ 'application.name': 'quickshell', 'application.process.id': '42' })], [42]), []));
test('playback streams and devices are not', () => assert.deepEqual(recordingApps([node({ 'application.name': 'Spotify' }, { isSink: true }), node({ 'node.name': 'alsa_input' }, { isStream: false })], []), []));
test('the level meter runs only while another app records', () => { assert.equal(micLevelEnabled(['DISCORD'], true, false), true); assert.equal(micLevelEnabled([], true, false), false); assert.equal(micLevelEnabled(['DISCORD'], false, false), false); assert.equal(micLevelEnabled(['DISCORD'], true, true), false); });
test('our own meter never keeps itself running', () => assert.equal(micLevelEnabled(recordingApps([node({ 'application.name': 'quickshell', 'application.process.id': '42' })], [42]), true, false), false));
test('alert text names one or several apps', () => { assert.equal(micAlertText(['DISCORD']), 'MIC LIVE · DISCORD IS RECORDING'); assert.equal(micAlertText(['DISCORD', 'OBS']), 'MIC LIVE · DISCORD, OBS ARE RECORDING'); });
test('volume text', () => { assert.equal(volumeText(0.64, false), '64%'); assert.equal(volumeText(0.64, true), 'MUTED'); });

// The properties Quickshell 0.2.1 gives its own PwNodePeakMonitor stream, read from a private PipeWire: no process id.
const shellMeter = {
  'application.name': 'Quickshell Peak Detect', 'media.class': 'Stream/Input/Audio', 'media.category': 'Monitor',
  'node.name': 'quickshell', 'resample.peaks': 'true', 'stream.capture.sink': 'false', 'stream.monitor': 'true',
  'target.object': '16',
};

test('the shell\'s own level meter is not recording, though its stream carries no process id', () => {
  assert.deepEqual(recordingApps([node(shellMeter)], [42]), []);
  assert.equal(micLevelEnabled(recordingApps([node(shellMeter)], [42]), true, false), false);
});

test('a recorder beside the shell\'s own meter is still found', () => {
  const recorder = node({ 'application.name': 'pw-record', 'node.name': 'pw-record' });
  assert.deepEqual(recordingApps([node(shellMeter), recorder], [42]), ['PW-RECORD']);
});

test('ghostd\'s spectrum capture of the output is not recording', () => {
  const spectrum = node({
    'application.name': 'ghostd', 'node.name': 'ghostd-spectrum', 'stream.capture.sink': 'true',
    'media.category': 'Capture',
  });
  assert.deepEqual(recordingApps([spectrum], [42]), []);
});

test('a stream whose properties have not arrived yet is not counted', () => {
  assert.deepEqual(recordingApps([node({})], [42]), []);
  assert.deepEqual(recordingApps([node(null)], [42]), []);
});

test('a video capture stream is not the microphone', () => {
  assert.deepEqual(recordingApps([node({ 'application.name': 'Firefox' }, { isAudio: false })], [42]), []);
});

test('names are the app name, else the node name, uppercased, once each, in order', () => {
  const nodes = [
    node({ 'application.name': 'OBS' }), node({ 'node.name': 'pw-record' }), node({ 'application.name': 'obs' }),
    node({ 'application.name': 'Discord' }),
  ];
  assert.deepEqual(recordingApps(nodes, []), ['OBS', 'PW-RECORD', 'DISCORD']);
});

test('a process id is matched as text, as PipeWire gives it', () => {
  const own = node({ 'application.name': 'quickshell', 'application.process.id': '42' });
  const other = node({ 'application.name': 'Discord', 'application.process.id': '420' });
  assert.deepEqual(recordingApps([own], ['42']), []);
  assert.deepEqual(recordingApps([other], [42]), ['DISCORD']);
});

test('a muted microphone with an app recording still reads LIVE', () => {
  assert.equal(inputText(['DISCORD'], true), 'LIVE');
  assert.equal(inputText(['DISCORD'], false), 'LIVE');
});

test('the input reads MUTED or -- while nothing records', () => {
  assert.equal(inputText([], true), 'MUTED');
  assert.equal(inputText([], false), '--');
});

test('no alert text while nothing records', () => assert.equal(micAlertText([]), ''));

test('volume text without a device', () => {
  assert.equal(volumeText(NaN, false), '--');
  assert.equal(volumeText(undefined, false), '--');
  assert.equal(volumeText(1.25, false), '125%');
});

test('the control row\'s volume', () => {
  assert.equal(controlVolume(0.64, false), 'VOL 64');
  assert.equal(controlVolume(0.64, true), 'VOL MUTED');
  assert.equal(controlVolume(NaN, false), 'VOL --');
});

test('only other apps\' audio captures are bound', () => {
  assert.equal(needsBinding(node({}, { name: 'pw-record' })), true);
  assert.equal(needsBinding(node({}, { name: 'pw-record', isSink: true })), false);
  assert.equal(needsBinding(node({}, { name: 'pw-record', isStream: false })), false);
  assert.equal(needsBinding(node({}, { name: 'pw-record', isAudio: false })), false);
});

test('Quickshell\'s own level meters are never bound', () => {
  assert.equal(needsBinding(node({}, { name: 'quickshell' })), false);
});
