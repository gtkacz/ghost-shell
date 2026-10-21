import QtQuick
import Quickshell
import GhostKit
import "../kit/GhostKit/lib/layout.mjs" as Layout

// One named view of the resolved layout: every slot gets a widget host and every tabs node a tabs
// host, placed from design pixels onto whole device pixels.
Item {
    id: root

    // Design pixels: the band kept free for the bar, and the height of a tab strip.
    readonly property real barBandPx: 30
    readonly property real tabStripPx: 26

    // The resolved layout, as Data.layout carries it.
    property var layout: null
    property string viewName
    property real designWidth
    property real designHeight

    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1
    readonly property var node: layout && layout.views[viewName] ? layout.views[viewName] : null
    readonly property var rects: {
        if (!node) {
            return [];
        }
        const margin = layout.margin;
        return Layout.computeRects(node, {
            x: margin,
            y: margin + barBandPx,
            w: designWidth - 2 * margin,
            h: designHeight - 2 * margin - barBandPx
        }, layout.gutter);
    }
    // Logical pixels: the top and bottom of the area the slots share, as rects places them.
    readonly property real areaTop: layout ? snap(Theme.d(layout.margin + barBandPx)) : 0
    readonly property real areaBottom: layout ? snap(Theme.d(designHeight - layout.margin)) : 0
    readonly property real stripHeight: snap(Theme.d(tabStripPx))
    // Logical pixels, whole ones as the input region takes them, covering every tab strip; the rest of
    // the view passes clicks through to the desktop.
    readonly property var inputRects: rects.filter(place => place.tabs).map(place => {
        const top = snap(Theme.d(place.y));
        const left = Math.floor(snap(Theme.d(place.x)));
        const right = Math.ceil(snap(Theme.d(place.x + place.w)));
        return {
            x: left,
            y: Math.floor(top),
            width: right - left,
            height: Math.ceil(top + stripHeight) - Math.floor(top)
        };
    })

    property var warnedLayout: null

    // Edges rather than sizes are snapped, so neighbours keep their gutter exactly.
    function snap(logical) {
        return Math.round(logical * pixelRatio) / pixelRatio;
    }

    // Once per layout, not on every recomputation of it.
    onRectsChanged: {
        if (rects.overflow && warnedLayout !== layout) {
            warnedLayout = layout;
            console.warn("view \"" + viewName + "\" does not fit the screen; its fixed sizes were shrunk to fit");
        }
    }

    Repeater {
        model: root.rects

        Item {
            id: cell

            required property var modelData

            x: root.snap(Theme.d(modelData.x))
            y: root.snap(Theme.d(modelData.y))
            width: root.snap(Theme.d(modelData.x + modelData.w)) - x
            height: root.snap(Theme.d(modelData.y + modelData.h)) - y

            Loader {
                anchors.fill: parent
                sourceComponent: cell.modelData.tabs ? tabsComponent : slotComponent
            }

            Component {
                id: slotComponent

                WidgetHost {
                    slot: cell.modelData
                }
            }

            Component {
                id: tabsComponent

                TabsHost {
                    node: cell.modelData
                    view: root.viewName
                    stripHeight: root.stripHeight
                }
            }
        }
    }
}
