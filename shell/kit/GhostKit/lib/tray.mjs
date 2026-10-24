const LINK_UP = 'up';
const PHONE = 'phone';
const PERCENT = 100;
// Twenty notches span the range, so a step is five percent.
const NOTCHES_PER_RANGE = 20;
// A backlight at zero can be fully dark, with nothing on screen to turn it back up, and one raw unit is as dark on a
// display whose range has thousands; ghostd clamps to the same share (DARK_PANEL_FLOOR_PCT in brightness.rs).
const MIN_BRIGHTNESS_PCT = 1;

export const APPLETS = {
  network: 'org.kde.plasma.networkmanagement',
  bluetooth: 'org.kde.plasma.bluetooth',
  volume: 'org.kde.plasma.volume',
  kdeconnect: 'org.kde.kdeconnect',
  clipboard: 'org.kde.plasma.clipboard',
  overflow: 'org.kde.plasma.systemtray',
};

export function networkState(links) {
  const anyLanUp = (links?.lan ?? []).some((link) => link.state === LINK_UP);
  return { glyph: 'network', dim: !anyLanUp };
}

export function bluetoothState({ enabled }) {
  return { glyph: 'bluetooth', dim: !enabled };
}

export function volumeState({ volume, muted }) {
  return { glyph: 'volume', dim: muted, label: `VOLUME ${Math.round(volume * PERCENT)}%` };
}

export function kdeConnectState(batteries) {
  const anyPhone = (batteries?.devices ?? []).some((device) => device.kind === PHONE);
  return { glyph: 'phone', dim: !anyPhone };
}

export function brightnessState({ nightLight }) {
  return { glyph: 'brightness', dim: !nightLight };
}

export function stepVolume(volume, notches) {
  const stepped = Math.round(volume * NOTCHES_PER_RANGE) + notches;
  return Math.min(NOTCHES_PER_RANGE, Math.max(0, stepped)) / NOTCHES_PER_RANGE;
}

// Quickshell's SystemTrayItem.Status enum: Passive, Active, NeedsAttention.
const STATUS_PASSIVE = 0;

export function showsInline(status) {
  return status !== STATUS_PASSIVE;
}

export function brightnessFloor(max) {
  return Math.max(1, Math.ceil((max * MIN_BRIGHTNESS_PCT) / PERCENT));
}

export function stepBrightness(brightness, max, notches) {
  const step = Math.max(1, Math.round(max / NOTCHES_PER_RANGE));
  return Math.min(max, Math.max(brightnessFloor(max), brightness + notches * step));
}

// The slider is cells, so a press anywhere in a cell sets the brightness that lights the row up to and including it.
export function cellBrightness(x, cells, cellStep, max) {
  if (cells <= 0) {
    return brightnessFloor(max);
  }
  const cell = Math.min(cells, Math.max(1, Math.floor(x / cellStep) + 1));
  return Math.max(brightnessFloor(max), Math.round((cell / cells) * max));
}


// KWin's "Toggle Night Color" flips `inhibited`, so the switch reads ON for an enabled Night Light that is not suspended,
// including by day in scheduled mode, when it is not tinting the screen (`running` is false).
export function nightLightSwitch(nightLight) {
  const usable = Boolean(nightLight?.available && nightLight?.enabled);
  const on = usable && !nightLight.inhibited;
  let status = 'UNAVAILABLE';
  if (usable) {
    status = on ? 'ON' : 'SUSPENDED';
  } else if (nightLight?.available) {
    status = 'DISABLED';
  }
  return { on, usable, status };
}

// What a slider row draws: the drag under the pointer, else the value just sent while the topic has not caught up, else
// the topic's. -1 is "none".
export function displayedBrightness(dragged, held, published) {
  if (dragged >= 0) {
    return dragged;
  }
  return held >= 0 ? held : published;
}

export function pendingAfter(pending, published) {
  return pending === published ? -1 : pending;
}

// Wheel notches add up from the target already sent, so notches faster than the round trip keep their steps.
export function wheelTarget(pending, published, max, notches) {
  return stepBrightness(pending >= 0 ? pending : published, max, notches);
}

// The bar's right-to-left reading order: the apps' status items sit between the built-ins and the bell, and the chevron
// closes the row because it opens the overflow of everything before it.
export function trayOrder(builtins, sniCount) {
  return [...builtins, ...Array(sniCount).fill('sni'), 'bell', 'chevron'];
}
