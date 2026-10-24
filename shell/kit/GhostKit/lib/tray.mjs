const LINK_UP = 'up';
const PHONE = 'phone';
const PERCENT = 100;
// Twenty notches span the range, so a step is five percent.
const NOTCHES_PER_RANGE = 20;

export const APPLETS = {
  network: 'org.kde.plasma.networkmanagement',
  bluetooth: 'org.kde.plasma.bluetooth',
  volume: 'org.kde.plasma.volume',
  kdeconnect: 'org.kde.kdeconnect',
  clipboard: 'org.kde.plasma.clipboard',
  brightness: 'org.kde.plasma.brightness',
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
