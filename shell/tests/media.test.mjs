import test from 'node:test';
import assert from 'node:assert/strict';
import { ago } from '../kit/GhostKit/lib/format.mjs';
import { pickPlayer, mediaView, rememberTrack, parseRemembered, progress, mmss, agoText } from '../../widgets/media/media.mjs';

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
    positionS: 38, lengthS: 104, status: 'NOW PLAYING',
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
