import QtQuick
import Quickshell
import GhostKit
import "../kit/GhostKit/lib/layout.mjs" as Layout

// One named view of the resolved layout: every slot gets a widget host and every tabs node a tabs
// host, placed from design pixels onto whole device pixels.
Item {
    id: root

    // Design pixels: the band kept free for the bar.
    readonly property real barBandPx: 30

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
    // The ClickAreas of the widgets and of the tab strips, which hand themselves to the view they sit in.
    property var clickAreas: []
    // Logical pixels, whole ones as the input region takes them, covering every ClickArea that is
    // visible and enabled, each tab among them; the rest of the view passes clicks through to the desktop.
    // The view fills the window from its origin, so window rects are view rects.
    readonly property var inputRects: clickAreas.filter(area => area && area.claimsInput).map(area => wholePixels(area.windowRect))

    property var warnedLayout: null

    // Edges rather than sizes are snapped, so neighbours keep their gutter exactly.
    function snap(logical) {
        return Math.round(logical * pixelRatio) / pixelRatio;
    }

    // Outward, so the region covers every pixel the area does.
    function wholePixels(rect) {
        const left = Math.floor(rect.x);
        const top = Math.floor(rect.y);
        return {
            x: left,
            y: top,
            width: Math.ceil(rect.x + rect.width) - left,
            height: Math.ceil(rect.y + rect.height) - top
        };
    }

    function claimInput(area) {
        clickAreas = clickAreas.concat([area]);
    }

    function releaseInput(area) {
        clickAreas = clickAreas.filter(claimed => claimed !== area);
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
                }
            }
        }
    }
}
