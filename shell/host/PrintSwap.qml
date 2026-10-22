import QtQuick
import Quickshell
import GhostKit
import "../kit/GhostKit/lib/theme.mjs" as ThemeLib
import "../kit/GhostKit/lib/views.mjs" as ViewsLib

// Shows Views.current and prints each change in (spec §6.1): the incoming view is revealed from the
// top of the views' area in discrete steps, with a light scanline at its edge erasing the outgoing
// view below it. The outgoing view is destroyed when the print ends, so outside a print only one
// view's widgets exist.
Item {
    id: root

    // Design pixels and percent, from the reference mockup's .scanline.
    readonly property real scanlinePx: 2
    readonly property real scanlineOpacity: 55

    // The resolved layout, as Data.layout carries it.
    property var layout: null
    property real designWidth
    property real designHeight

    readonly property string view: Views.current
    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1

    // The page on screen, which during a print is the incoming one, and the outgoing page.
    property Item shown: null
    property Item leaving: null
    property int step: ViewsLib.PRINT_STEPS
    property int printMs: 0
    property double printStartedMs: 0
    readonly property bool printing: leaving !== null

    // Nothing takes clicks during a print, so none lands on a view that is being erased.
    readonly property var inputRects: shown && !printing ? shown.inputRects : []

    // Logical pixels, on whole physical ones: the edge between the incoming view above and the
    // outgoing one below.
    readonly property real sweepTop: shown ? shown.areaTop : 0
    readonly property real sweepHeight: shown ? shown.areaBottom - shown.areaTop : 0
    readonly property real edge: sweepTop + ViewsLib.revealRows(step, ViewsLib.PRINT_STEPS, Math.round(sweepHeight * pixelRatio)) / pixelRatio

    // A view replacing a shown one is a new showing of it. The view an uncover creates this item on is
    // not, and its binding settles before anything is shown.
    function follow() {
        if (shown) {
            FirstWrite.showView(view);
        }
        if (shown && !printing && Views.durationMs > 0) {
            beginPrint();
            return;
        }
        showAtOnce();
    }

    // The views are busy for at least as long as printing is true.
    function beginPrint() {
        Views.hostControl.busy = true;
        leaving = shown;
        leaving.outgoing = true;
        shown = createPage();
        step = 0;
        printMs = Views.durationMs;
        printStartedMs = Date.now();
        armStep();
    }

    function armStep() {
        stepTimer.interval = ViewsLib.stepDelay(step, ViewsLib.PRINT_STEPS, printMs, Date.now() - printStartedMs);
        stepTimer.restart();
    }

    function finish() {
        stepTimer.stop();
        step = ViewsLib.PRINT_STEPS;
        if (leaving) {
            discard(leaving);
            leaving = null;
        }
        Views.hostControl.busy = false;
    }

    // A change with no print, or one that lands during a print, shows the view at once.
    function showAtOnce() {
        finish();
        if (shown && shown.viewName === view) {
            return;
        }
        if (shown) {
            discard(shown);
        }
        shown = view !== "" ? createPage() : null;
    }

    function createPage() {
        return pageFactory.createObject(root, {
            viewName: view
        });
    }

    // destroy() takes effect only after this event-loop turn, and the page would draw until then.
    function discard(page) {
        page.visible = false;
        page.destroy();
    }

    onViewChanged: follow()
    Component.onCompleted: showAtOnce()
    Component.onDestruction: {
        if (printing) {
            Views.hostControl.busy = false;
        }
    }

    Component {
        id: pageFactory

        // One view, cut to the rows it shows while a print runs.
        Item {
            id: page

            property alias viewName: layoutView.viewName
            readonly property alias inputRects: layoutView.inputRects
            readonly property alias areaTop: layoutView.areaTop
            readonly property alias areaBottom: layoutView.areaBottom
            property bool outgoing: false

            y: outgoing ? root.edge : 0
            width: root.width
            height: (outgoing || !root.printing ? root.height : root.edge) - y
            clip: root.printing

            LayoutView {
                id: layoutView

                // Moved against the page, so the view stays where it is on screen while its page is cut.
                y: -page.y
                width: root.width
                height: root.height
                layout: root.layout
                designWidth: root.designWidth
                designHeight: root.designHeight
            }
        }
    }

    // A transient sequence, so it may have a timer of its own while it runs: 24 steps in 640 or
    // 380 ms are more than the frame clock's 15 per second can draw.
    Timer {
        id: stepTimer

        // Re-armed for every step with the wait to that step's due time, so the print takes its length.
        repeat: false
        onTriggered: {
            root.step += 1;
            if (root.step >= ViewsLib.PRINT_STEPS) {
                root.finish();
                return;
            }
            root.armStep();
        }
    }

    // Above the pages, which are created after it.
    Rectangle {
        z: 1
        visible: root.printing
        y: root.edge
        width: root.width
        height: Math.max(1, Math.round(Theme.d(root.scanlinePx) * root.pixelRatio)) / root.pixelRatio
        color: ThemeLib.withAlpha(Theme.hi, root.scanlineOpacity)
    }
}
