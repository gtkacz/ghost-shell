const DIRECTION_ROW = 'row';

function orNull(value) {
  return value === undefined ? null : value;
}

function isFlex(node) {
  return node.size === null || node.size === undefined;
}

function fixedSize(child) {
  return Math.max(0, child.size);
}

// Fixed sizes are honoured first; when they cannot fit they shrink proportionally and flex children get nothing,
// so siblings never overlap.
function mainAxisExtents(children, total, gutter) {
  const gutters = gutter * Math.max(0, children.length - 1);
  const space = total - gutters;
  const available = Math.max(0, space);

  let fixedSum = 0;
  let flexCount = 0;
  for (let i = 0; i < children.length; i += 1) {
    if (isFlex(children[i])) {
      flexCount += 1;
    } else {
      fixedSum += fixedSize(children[i]);
    }
  }

  const overflow = fixedSum > space;
  const fixedScale = fixedSum > available ? available / fixedSum : 1;
  const flexShare = flexCount > 0 ? Math.max(0, available - fixedSum * fixedScale) / flexCount : 0;
  const extents = children.map((child) => (isFlex(child) ? flexShare : fixedSize(child) * fixedScale));
  return { extents, overflow };
}

function childRect(rect, horizontal, offset, extent) {
  return horizontal
    ? { x: offset, y: rect.y, w: extent, h: rect.h }
    : { x: rect.x, y: offset, w: rect.w, h: extent };
}

function placeTabs(node, rect, path) {
  return {
    path,
    widget: null,
    source: null,
    settings: null,
    error: null,
    x: rect.x,
    y: rect.y,
    w: rect.w,
    h: rect.h,
    tabs: node.tabs.map((tab) => ({
      title: tab.title,
      widget: orNull(tab.widget),
      source: orNull(tab.source),
      settings: orNull(tab.settings),
      error: orNull(tab.error),
    })),
    default: node.default,
  };
}

function placeSlot(node, rect, path) {
  return {
    path,
    widget: orNull(node.widget),
    source: orNull(node.source),
    settings: orNull(node.settings),
    error: orNull(node.error),
    x: rect.x,
    y: rect.y,
    w: rect.w,
    h: rect.h,
  };
}

function place(node, rect, gutter, path, out) {
  if (Array.isArray(node.tabs)) {
    out.rects.push(placeTabs(node, rect, path));
    return;
  }
  if (!Array.isArray(node.children)) {
    out.rects.push(placeSlot(node, rect, path));
    return;
  }

  // The override spaces this split's children only, so nested splits inherit the layout's gutter.
  const childGutter = node.gutter ?? gutter;
  const horizontal = node.direction === DIRECTION_ROW;
  const { extents, overflow } = mainAxisExtents(node.children, horizontal ? rect.w : rect.h, childGutter);
  out.overflow = out.overflow || overflow;

  let offset = horizontal ? rect.x : rect.y;
  for (let i = 0; i < node.children.length; i += 1) {
    place(node.children[i], childRect(rect, horizontal, offset, extents[i]), gutter, path + '[' + i + ']', out);
    offset += extents[i] + childGutter;
  }
}

export function computeRects(node, rect, gutter) {
  const bounds = { x: rect.x, y: rect.y, w: Math.max(0, rect.w), h: Math.max(0, rect.h) };
  const out = { rects: [], overflow: false };
  place(node, bounds, gutter, '', out);
  if (out.overflow) {
    out.rects.overflow = true;
  }
  return out.rects;
}

export const BAR_BAND_PX = 30;
export const DOCK_BAND_PX = 58;
const POSITION_BOTTOM = 'bottom';

// The rect the views share once the bar's band is kept free on its side and, in layout B, the dock's on the other.
export function contentArea({ designWidth, designHeight, margin, position, hasDock }) {
  const atBottom = position === POSITION_BOTTOM;
  const dockBand = hasDock ? DOCK_BAND_PX : 0;
  const top = atBottom ? dockBand : BAR_BAND_PX;
  const bottom = atBottom ? BAR_BAND_PX : dockBand;
  return {
    x: margin,
    y: margin + top,
    w: designWidth - 2 * margin,
    h: designHeight - 2 * margin - top - bottom,
  };
}
