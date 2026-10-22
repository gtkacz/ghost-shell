// A text scrambles on its first write in each showing of its widget. The host showing a widget scopes the
// widget's keys, so a new showing forgets only its own scope's keys and an uncover, which keeps every
// scope's keys, shows each text directly.
const VIEW_END = '/';
const SCOPE_END = '|';

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

export function forget(claimed, prefix) {
  for (const key of [...claimed]) {
    if (key.startsWith(prefix)) {
      claimed.delete(key);
    }
  }
}
