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
// KDE Connect republishes a phone's player on the desktop's bus, under a bus name holding this word.
const MIRROR_MARK = 'kdeconnect';
// Players hand over art as local files or from the web, and the shell loads nothing else.
const ART_SCHEME = /^(https|file):/i;

function text(value) {
  return typeof value === 'string' ? value : '';
}

function isMirror(player) {
  return [player.id, player.identity].some(name => text(name).toLowerCase().includes(MIRROR_MARK));
}

// The index of the player to show. Among the playing ones a local player beats a KDE Connect mirror, which plays
// whatever the phone does, and then the last active one wins; with none playing, the last active one, else the first.
// `players` is [{ id, identity, playing }].
export function pickPlayer(players, lastActiveId) {
  const list = players || [];
  const last = list.findIndex(player => player.id === lastActiveId);
  const playing = list.map((_, index) => index).filter(index => list[index].playing);
  if (playing.length > 0) {
    const local = playing.filter(index => !isMirror(list[index]));
    const candidates = local.length > 0 ? local : playing;
    return candidates.includes(last) ? last : candidates[0];
  }
  if (last !== NO_PLAYER) {
    return last;
  }
  return list.length > 0 ? 0 : NO_PLAYER;
}

// The art URL if it is one the shell loads, else no art, which shows the rings: any process can set a player's art.
export function allowedArtUrl(url) {
  const candidate = text(url);
  return ART_SCHEME.test(candidate) ? candidate : '';
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

function lastPlayed(remembered, nowMs) {
  if (!remembered) {
    return '';
  }
  const age = agoText(Math.floor((nowMs - remembered.atMs) / MS_PER_SECOND));
  return age === AGO_NOW ? LAST_PLAYED + ' ' + AGO_NOW : LAST_PLAYED + ' ' + age + ' ' + AGO;
}

// What the widget shows. A playing or paused player shows its own track; a stopped one, or none, shows the
// remembered track, which the widget dims, and no times. `detail` is the status after its state word.
export function mediaView(player, remembered, nowMs) {
  const mode = player ? player.playbackState : STOPPED;
  if (mode === PLAYING || mode === PAUSED) {
    return {
      mode,
      title: text(player.trackTitle),
      artist: text(player.trackArtist),
      artUrl: allowedArtUrl(player.trackArtUrl),
      identity: text(player.identity),
      positionS: player.position,
      lengthS: player.length,
      status: mode === PLAYING ? STATUS_PLAYING : STATUS_PAUSED,
      detail: '',
    };
  }
  const detail = lastPlayed(remembered, nowMs);
  return {
    mode: STOPPED,
    title: remembered ? remembered.title : '',
    artist: remembered ? text(remembered.artist) : '',
    artUrl: remembered ? allowedArtUrl(remembered.artUrl) : '',
    identity: (player && text(player.identity)) || (remembered ? text(remembered.identity) : ''),
    positionS: null,
    lengthS: null,
    status: detail !== '' ? STATUS_STOPPED + SEPARATOR + detail : STATUS_STOPPED,
    detail,
  };
}

// The status for a line whose chip already gives the state, so a narrow line keeps the age: LAST PLAYED 12M AGO.
export function shortStatus(view) {
  return view.detail !== '' ? view.detail : view.status;
}

export function rememberTrack(player, nowMs) {
  if (!player || !text(player.trackTitle)) {
    return null;
  }
  return {
    title: player.trackTitle,
    artist: text(player.trackArtist),
    artUrl: allowedArtUrl(player.trackArtUrl),
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
