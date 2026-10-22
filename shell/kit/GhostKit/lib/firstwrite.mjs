// A text scrambles on its first write in each showing of its widget. The host showing a widget scopes the
// widget's keys, so a new showing forgets only its own scope's keys and an uncover, which keeps every
// scope's keys, shows each text directly.
import { MISSING } from './format.mjs';

const VIEW_END = '/';
const SCOPE_END = '|';
const POSITION_MARK = '@';
const POSITION_STEP = '.';

export function hostScope(view, path) {
  return view + VIEW_END + path;
}

// Ends in the separator, so forgetting V1 leaves V10.
export function viewPrefix(view) {
  return view + VIEW_END;
}

// Outside a host, as in the gallery, the key is session-wide as before.
export function scopedKey(scope, key) {
  return scope === '' ? key : scope + SCOPE_END + key;
}

// The slot's geometry is left out, so a layout re-sent unchanged, or one that only moves a widget,
// is not a new showing of it.
export function slotSignature(slot) {
  return JSON.stringify([slot.widget, slot.source, slot.settings]);
}

// A text without a key of its own is known by its child index at each level below its host. A rebuild
// of the same widget puts it in the same place, so after an uncover it finds its first write claimed,
// and a row a Repeater recreates as its value changes is not written anew.
export function positionKey(steps) {
  return POSITION_MARK + steps.join(POSITION_STEP);
}

// A value shows the missing dash until its data arrives, and the data's arrival is its first write.
export function isWritten(text) {
  return text.trim() !== '' && text !== MISSING;
}

// A new showing has just forgotten its scope's claims, so a scope that still holds some is being rebuilt.
export function holdsClaims(claimed, scope) {
  const prefix = scopedKey(scope, '');
  for (const key of claimed) {
    if (key.startsWith(prefix)) {
      return true;
    }
  }
  return false;
}

export function forget(claimed, prefix) {
  for (const key of [...claimed]) {
    if (key.startsWith(prefix)) {
      claimed.delete(key);
    }
  }
}
