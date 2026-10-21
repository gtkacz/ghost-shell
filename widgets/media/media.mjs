// The media widget's logic, apart from QML so that node can test it. A player is a plain object with MprisPlayer's
// property names and its playback state as a word: { playbackState: 'playing'|'paused'|'stopped', trackTitle,
// trackArtist, trackArtUrl, identity, desktopEntry, position, length }.

const PLAYING = 'playing';
const PAUSED = 'paused';
const STOPPED = 'stopped';

const STATUS_PLAYING = 'NOW PLAYING';
const STATUS_PAUSED = 'PAUSED';
const STATUS_STOPPED = 'STOPPED';
const LAST_PLAYED = 'LAST PLAYED';
const AGO = 'AGO';
const SEPARATOR = ' · ';

const NO_PLAYER = -1;
const MS_PER_SECOND = 1000;
const SECONDS_PER_MINUTE = 60;
const MINUTES_PER_HOUR = 60;
const HOURS_PER_DAY = 24;
const SECONDS_PER_HOUR = SECONDS_PER_MINUTE * MINUTES_PER_HOUR;
const SECONDS_PER_DAY = SECONDS_PER_HOUR * HOURS_PER_DAY;
const AGO_NOW = 'NOW';
const SECONDS_DIGITS = 2;
const MISSING_TIME = '-:--';

function text(value) {
  return typeof value === 'string' ? value : '';
}

// The index of the player to show: a playing one, the last active one first, else the last active one, else the
// first. `players` is [{ id, playing }].
export function pickPlayer(players, lastActiveId) {
  const list = players || [];
  const last = list.findIndex(player => player.id === lastActiveId);
  if (last !== NO_PLAYER && list[last].playing) {
    return last;
  }
  const playing = list.findIndex(player => player.playing);
  if (playing !== NO_PLAYER) {
    return playing;
  }
  if (last !== NO_PLAYER) {
    return last;
  }
  return list.length > 0 ? 0 : NO_PLAYER;
}

// Fmt.ago's wording. A widget's helper files can reach only their own folder, not the kit's libraries, so the rule
// is repeated here and a test holds the two together.
export function agoText(seconds) {
  if (seconds < SECONDS_PER_MINUTE) {
    return AGO_NOW;
  }
  if (seconds < SECONDS_PER_HOUR) {
    return Math.floor(seconds / SECONDS_PER_MINUTE) + 'M';
  }
  if (seconds < SECONDS_PER_DAY) {
    return Math.floor(seconds / SECONDS_PER_HOUR) + 'H';
  }
  return Math.floor(seconds / SECONDS_PER_DAY) + 'D';
}

function stoppedStatus(remembered, nowMs) {
  if (!remembered) {
    return STATUS_STOPPED;
  }
  const age = agoText(Math.floor((nowMs - remembered.atMs) / MS_PER_SECOND));
  const lastPlayed = age === AGO_NOW ? LAST_PLAYED + ' ' + AGO_NOW : LAST_PLAYED + ' ' + age + ' ' + AGO;
  return STATUS_STOPPED + SEPARATOR + lastPlayed;
}

// What the widget shows. A playing or paused player shows its own track; a stopped one, or none, shows the
// remembered track, which the widget dims, and no times.
export function mediaView(player, remembered, nowMs) {
  const mode = player ? player.playbackState : STOPPED;
  if (mode === PLAYING || mode === PAUSED) {
    return {
      mode,
      title: text(player.trackTitle),
      artist: text(player.trackArtist),
      artUrl: text(player.trackArtUrl),
      identity: text(player.identity),
      positionS: player.position,
      lengthS: player.length,
      status: mode === PLAYING ? STATUS_PLAYING : STATUS_PAUSED,
    };
  }
  return {
    mode: STOPPED,
    title: remembered ? remembered.title : '',
    artist: remembered ? text(remembered.artist) : '',
    artUrl: remembered ? text(remembered.artUrl) : '',
    identity: (player && text(player.identity)) || (remembered ? text(remembered.identity) : ''),
    positionS: null,
    lengthS: null,
    status: stoppedStatus(remembered, nowMs),
  };
}

export function rememberTrack(player, nowMs) {
  if (!player || !text(player.trackTitle)) {
    return null;
  }
  return {
    title: player.trackTitle,
    artist: text(player.trackArtist),
    artUrl: text(player.trackArtUrl),
    identity: text(player.identity),
    desktopEntry: text(player.desktopEntry),
    atMs: nowMs,
  };
}

// The remembered track from media.json, or null when the file holds anything else: any process can write it.
export function parseRemembered(json) {
  let value;
  try {
    value = JSON.parse(json);
  } catch (parseError) {
    return null;
  }
  if (value === null || typeof value !== 'object' || !text(value.title) || !Number.isFinite(value.atMs)) {
    return null;
  }
  return {
    title: value.title,
    artist: text(value.artist),
    artUrl: text(value.artUrl),
    identity: text(value.identity),
    desktopEntry: text(value.desktopEntry),
    atMs: value.atMs,
  };
}

// A fraction of the track, or null when there is nothing to show: a stream without a length, or an unknown position.
export function progress(positionS, lengthS) {
  if (!Number.isFinite(lengthS) || lengthS <= 0 || !Number.isFinite(positionS)) {
    return null;
  }
  return Math.min(1, Math.max(0, positionS / lengthS));
}

export function mmss(seconds) {
  if (!Number.isFinite(seconds)) {
    return MISSING_TIME;
  }
  const whole = Math.max(0, Math.floor(seconds));
  return Math.floor(whole / SECONDS_PER_MINUTE) + ':' + String(whole % SECONDS_PER_MINUTE).padStart(SECONDS_DIGITS, '0');
}
