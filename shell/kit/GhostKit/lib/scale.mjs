const DESIGN_WIDTH = 1280;

export function designUnit(logicalWidth) {
  return logicalWidth / DESIGN_WIDTH;
}

// Indexed loop rather than Array methods because Quickshell exposes screens as a QML list, not always a JS array.
export function pickScreen(screens, model) {
  if (screens.length === 0) {
    return null;
  }
  const wanted = (model || '').toLowerCase();
  if (wanted !== '') {
    for (let i = 0; i < screens.length; i += 1) {
      if ((screens[i].model || '').toLowerCase().indexOf(wanted) !== -1) {
        return screens[i];
      }
    }
  }
  return screens[0];
}
