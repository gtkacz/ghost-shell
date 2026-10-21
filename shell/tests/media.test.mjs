import test from 'node:test';
import assert from 'node:assert/strict';
import { ago } from '../kit/GhostKit/lib/format.mjs';
import {
  pickPlayer, mediaView, rememberTrack, parseRemembered, progress, mmss, agoText, allowedArtUrl, shortStatus, fallStep, isDark,
} from '../../widgets/media/media.mjs';

const MINUTE_MS = 60000;

test('prefers a playing player, then the last active one', () => { assert.equal(pickPlayer([{ id: 'a', playing: false }, { id: 'b', playing: true }], 'a'), 1); assert.equal(pickPlayer([{ id: 'a', playing: false }, { id: 'b', playing: false }], 'b'), 1); assert.equal(pickPlayer([], 'b'), -1); });
test('paused keeps the track and says PAUSED', () => assert.equal(mediaView({ playbackState: 'paused', trackTitle: 'Derezzed', trackArtist: 'Daft Punk', position: 38, length: 104 }, null, 0).status, 'PAUSED'));
test('a vanished player shows the remembered track as stopped with its age', () => { const v = mediaView(null, { title: 'Derezzed', artist: 'Daft Punk', atMs: 0 }, 12 * 60000); assert.equal(v.status, 'STOPPED · LAST PLAYED 12M AGO'); assert.equal(v.title, 'Derezzed'); });
test('nothing remembered is plain STOPPED', () => assert.equal(mediaView(null, null, 0).status, 'STOPPED'));
test('no length hides progress', () => { assert.equal(progress(30, 0), null); assert.equal(progress(52, 104), 0.5); });
test('mmss', () => { assert.equal(mmss(104), '1:44'); assert.equal(mmss(38.9), '0:38'); });

test('among several playing players the last active one wins', () => {
  const players = [{ id: 'a', playing: true }, { id: 'b', playing: true }];
  assert.equal(pickPlayer(players, 'b'), 1);
  assert.equal(pickPlayer(players, 'gone'), 0);
});

test('with nothing playing and no last active player the first one is shown', () => {
  assert.equal(pickPlayer([{ id: 'a', playing: false }, { id: 'b', playing: false }], ''), 0);
});

test('a playing player shows its own track, identity and times', () => {
  const v = mediaView({
    playbackState: 'playing', trackTitle: 'Derezzed', trackArtist: 'Daft Punk', trackArtUrl: 'https://i.scdn.co/image/x',
    identity: 'Spotify', desktopEntry: 'spotify', position: 38, length: 104,
  }, { title: 'Older', artist: 'Someone', atMs: 0 }, 5 * MINUTE_MS);
  assert.deepEqual(v, {
    mode: 'playing', title: 'Derezzed', artist: 'Daft Punk', artUrl: 'https://i.scdn.co/image/x', identity: 'Spotify',
    positionS: 38, lengthS: 104, status: 'NOW PLAYING', detail: '',
  });
});

test('a player that is present but stopped shows the remembered track and no times', () => {
  const v = mediaView({ playbackState: 'stopped', trackTitle: '', identity: 'Spotify', position: 0, length: 0 },
    { title: 'Derezzed', artist: 'Daft Punk', artUrl: 'file:///art.png', identity: 'Spotify', atMs: 0 }, 2 * 3600000);
  assert.equal(v.mode, 'stopped');
  assert.equal(v.status, 'STOPPED · LAST PLAYED 2H AGO');
  assert.equal(v.title, 'Derezzed');
  assert.equal(v.artUrl, 'file:///art.png');
  assert.equal(v.identity, 'Spotify');
  assert.equal(progress(v.positionS, v.lengthS), null);
});

test('a player that has just quit was last played now, with no AGO', () => {
  assert.equal(mediaView(null, { title: 'Derezzed', artist: 'Daft Punk', atMs: 1000 }, 30000).status, 'STOPPED · LAST PLAYED NOW');
});

test('the vanished player keeps its identity from the remembered track', () => {
  assert.equal(mediaView(null, { title: 'Derezzed', identity: 'Spotify', atMs: 0 }, 0).identity, 'Spotify');
});

test('a track is remembered with its player and time', () => {
  assert.deepEqual(rememberTrack({
    playbackState: 'playing', trackTitle: 'Derezzed', trackArtist: 'Daft Punk', trackArtUrl: 'https://i.scdn.co/image/x',
    identity: 'Spotify', desktopEntry: 'spotify', position: 1, length: 104,
  }, 1234), { title: 'Derezzed', artist: 'Daft Punk', artUrl: 'https://i.scdn.co/image/x', identity: 'Spotify', desktopEntry: 'spotify', atMs: 1234 });
});

test('a track without a title is not remembered', () => {
  assert.equal(rememberTrack({ playbackState: 'playing', trackTitle: '', trackArtist: 'Daft Punk' }, 0), null);
  assert.equal(rememberTrack(null, 0), null);
});

test('progress is clamped and needs a known position', () => {
  assert.equal(progress(120, 104), 1);
  assert.equal(progress(-3, 104), 0);
  assert.equal(progress(NaN, 104), null);
  assert.equal(progress(30, NaN), null);
  assert.equal(progress(30, -1), null);
});

test('mmss of an unknown time is a placeholder', () => assert.equal(mmss(NaN), '-:--'));

test('the age reads the same as Fmt.ago', () => {
  for (const seconds of [-30, 0, 59, 60, 61, 3599, 3600, 86399, 86400, 86400 * 3 + 5000]) {
    assert.equal(agoText(seconds), ago(seconds), `at ${seconds} s`);
  }
});

test('a remembered track survives a round trip through media.json', () => {
  const track = { title: 'Derezzed', artist: 'Daft Punk', artUrl: 'https://i.scdn.co/image/x', identity: 'Spotify', desktopEntry: 'spotify', atMs: 1234 };
  assert.deepEqual(parseRemembered(JSON.stringify(track)), track);
});

test('a media.json that is not a remembered track reads as nothing remembered', () => {
  assert.equal(parseRemembered(''), null);
  assert.equal(parseRemembered('{"title":'), null);
  assert.equal(parseRemembered('null'), null);
  assert.equal(parseRemembered('[1]'), null);
  assert.equal(parseRemembered('{"title":"","atMs":1}'), null);
  assert.equal(parseRemembered('{"title":"Derezzed","atMs":"soon"}'), null);
  assert.deepEqual(parseRemembered('{"title":"Derezzed","atMs":5,"artist":7}'), { title: 'Derezzed', artist: '', artUrl: '', identity: '', desktopEntry: '', atMs: 5 });
});

const MIRROR = { id: 'org.mpris.MediaPlayer2.kdeconnect.mpris_229b827e', identity: 'Spotify - Galaxy S25 Ultra', playing: true };
const LOCAL = { id: 'org.mpris.MediaPlayer2.spotify', identity: 'Spotify', playing: true };

test('a local player beats a KDE Connect mirror when both play, even a last active mirror', () => {
  assert.equal(pickPlayer([MIRROR, LOCAL], ''), 1);
  assert.equal(pickPlayer([MIRROR, LOCAL], MIRROR.id), 1);
});

test('a mirror is known by its identity too, in any case', () => {
  const mirror = { id: 'org.mpris.MediaPlayer2.phone', identity: 'KDEConnect relay', playing: true };
  assert.equal(pickPlayer([mirror, LOCAL], mirror.id), 1);
});

test('among local players that play, the last active one still wins', () => {
  const other = { id: 'org.mpris.MediaPlayer2.firefox', identity: 'Firefox', playing: true };
  assert.equal(pickPlayer([LOCAL, MIRROR, other], other.id), 2);
});

test('a mirror that plays alone is shown', () => {
  assert.equal(pickPlayer([MIRROR, { ...LOCAL, playing: false }], LOCAL.id), 0);
});

test('only https and file art is loaded', () => {
  for (const url of ['https://i.scdn.co/image/x', 'file:///home/u/.cache/a.jpg', 'HTTPS://I.SCDN.CO/x', 'File:///a.png']) {
    assert.equal(allowedArtUrl(url), url, url);
  }
  for (const url of ['http://example.com/a.jpg', 'data:image/png;base64,AAAA', 'image://icon/x', 'qrc:/a.png', 'javascript:alert(1)', 'https', '/tmp/a.png', '', undefined]) {
    assert.equal(allowedArtUrl(url), '', String(url));
  }
});

test('other art reads as no art, from the player or the remembered track', () => {
  assert.equal(mediaView({ playbackState: 'playing', trackTitle: 'T', trackArtUrl: 'http://x/a.jpg', position: 1, length: 2 }, null, 0).artUrl, '');
  assert.equal(mediaView(null, { title: 'T', artUrl: 'data:image/png;base64,AA', atMs: 0 }, 0).artUrl, '');
  assert.equal(rememberTrack({ playbackState: 'playing', trackTitle: 'T', trackArtUrl: 'http://x/a.jpg' }, 0).artUrl, '');
});

test('a line whose chip says the state shows only the age of the remembered track', () => {
  const remembered = mediaView(null, { title: 'Derezzed', atMs: 0 }, 12 * MINUTE_MS);
  assert.equal(remembered.detail, 'LAST PLAYED 12M AGO');
  assert.equal(shortStatus(remembered), 'LAST PLAYED 12M AGO');
  assert.equal(shortStatus(mediaView(null, { title: 'Derezzed', atMs: 0 }, 0)), 'LAST PLAYED NOW');
  assert.equal(shortStatus(mediaView(null, null, 0)), 'STOPPED');
  assert.equal(shortStatus(mediaView({ playbackState: 'paused', trackTitle: 'T', position: 1, length: 2 }, null, 0)), 'PAUSED');
});

const BANDS = 32;
const BAND_MAX = 255;
// About what the daemon's analyzer takes to fall from full scale to dark at its own rate.
const MAX_FALL_STEPS = 25;

test('a fall step drops each band to the floor of 0.8 of its value', () => {
  assert.deepEqual(fallStep([255, 100, 6, 5, 1, 0]), [204, 80, 4, 4, 0, 0]);
});

test('a fall step matches the analyzer for every byte', () => {
  const every = Array.from({ length: BAND_MAX + 1 }, (_, band) => band);
  assert.deepEqual(fallStep(every), every.map(band => Math.floor((band * 4) / 5)));
});

test('a fall step leaves its input alone', () => {
  const bands = [255, 10];
  fallStep(bands);
  assert.deepEqual(bands, [255, 10]);
});

test('a band that is not a positive number falls to dark, and a huge one is capped', () => {
  assert.deepEqual(fallStep([NaN, -3, undefined, Infinity, 1e300]), [0, 0, 0, 204, 204]);
});

test('all bands at zero is dark and done', () => {
  const dark = new Array(BANDS).fill(0);
  assert.equal(isDark(dark), true);
  assert.equal(isDark([0, 0, 1]), false);
  assert.equal(isDark([]), true);
  assert.deepEqual(fallStep(dark), dark);
});

test('full scale reaches dark in a bounded number of steps', () => {
  let bands = new Array(BANDS).fill(BAND_MAX);
  let steps = 0;
  while (!isDark(bands)) {
    bands = fallStep(bands);
    steps += 1;
    assert.ok(steps <= MAX_FALL_STEPS, 'still falling after ' + steps + ' steps');
  }
  assert.ok(steps > 1);
});
