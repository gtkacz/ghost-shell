// How long the panel prints, top to bottom, when the switcher opens.
export const PRINT_MS = 110;
export const PRINT_STEPS = 6;

const NO_NAME = 'WINDOW';
const DESKTOP_SUFFIX = /\.desktop$/;
// A Chromium-style web app's id names its browser and an extension hash, so only its caption names the app.
const WEB_APP_ID = /^.+-[a-z]{32}-.+$/;
// A Proton game's id holds only its Steam number, so here too only the caption names it.
const STEAM_GAME_ID = /^steam_app_\d+$/;
const PACKAGING_SUFFIX = /-(browser|bin|stable|desktop)$/;
const WORD_SEPARATORS = /[-_]+/g;
// Greedy, so the split is at the last separator: an app's name ends a caption.
const LAST_SEGMENT = /^(.+) [—–-] (.+)$/;

function lastSegment(caption) {
  const parts = caption.match(LAST_SEGMENT);
  return parts ? parts[2] : caption;
}

function fromId(id) {
  return id.slice(id.lastIndexOf('.') + 1).replace(PACKAGING_SUFFIX, '').replace(WORD_SEPARATORS, ' ');
}

export function appName(desktopFileName, resourceClass, caption) {
  const desktop = desktopFileName.replace(DESKTOP_SUFFIX, '');
  const id = desktop || resourceClass;
  const name = id && !WEB_APP_ID.test(desktop) && !STEAM_GAME_ID.test(id) ? fromId(id) : lastSegment(caption);
  return (name || NO_NAME).toUpperCase();
}

// Only a last segment that names the app is dropped, so a media player's "Song - Artist" stays whole.
export function titleWithoutApp(caption, app) {
  const parts = caption.match(LAST_SEGMENT);
  return parts && parts[2].toLowerCase().includes(app.toLowerCase()) ? parts[1] : caption;
}

export function rowNumber(index) {
  return String(index + 1).padStart(2, '0');
}
