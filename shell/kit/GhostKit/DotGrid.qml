import QtQuick
import Quickshell
import "lib/theme.mjs" as ThemeLib

// The background dot grid, still or moving as appearance.dots.motion says. A timed motion holds the frame clock while
// the grid is visible; the cursor motion holds it only while the pointer moves, and steps on its ticks rather than on
// every pointer event, so it never draws faster than the clock either.
ShaderEffect {
    id: root

    // Design pixels, from the reference mockup's radial-gradient dots.
    readonly property real dotRadiusPx: 1
    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1

    // Where the pointer is, in this item's coordinates, and whether it is over the desktop at all. The host
    // feeds these from a HoverHandler that sees the pointer over widgets too.
    property point pointerAt
    property bool pointerInside: false

    property color dotColor: ThemeLib.withAlpha(Theme.dot, Theme.dotOpacity)
    // Whole physical pixels, so every dot rasterises the same way across the screen.
    property real spacing: Math.max(1, Math.round(Theme.d(Theme.dotSpacing) * pixelRatio))
    property real radius: Theme.d(dotRadiusPx) * pixelRatio
    property size resolution: Qt.size(width * pixelRatio, height * pixelRatio)

    property int mode: Theme.dotMotion.mode
    // Read only for a timed motion, so a still or cursor grid does not redraw on other animations' ticks.
    property real time: Theme.dotMotion.timed ? FrameClock.elapsedMs / msPerSecond * Theme.dotSpeed : 0
    property real sizeRange: Theme.dotSizeRange
    property real brightness: Theme.dotBrightness
    property real featureUnit: Theme.d(1) * pixelRatio * Theme.dotScale
    property real cover: Theme.dotCover
    property point pointer: followsPointer ? Qt.point(pointerHold.shown.x * pixelRatio, pointerHold.shown.y * pixelRatio) : offItem

    readonly property int msPerSecond: 1000
    // Far enough off the item that the halo's tail rounds to nothing.
    readonly property point offItem: Qt.point(-1e6, -1e6)
    // Ticks the pointer may stand still before the clock is given back.
    readonly property int pointerIdleTicks: 2
    readonly property bool followsPointer: Theme.dotMotion.pointer && pointerInside && visible

    fragmentShader: Qt.resolvedUrl("shaders/dotgrid.frag.qsb")

    onFollowsPointerChanged: {
        if (followsPointer) {
            pointerHold.shown = pointerAt;
        } else {
            pointerHold.hold(false);
        }
    }
    onPointerAtChanged: {
        // The first move after a rest shows at once; the rest wait for a tick.
        if (followsPointer && !pointerHold.held) {
            pointerHold.shown = pointerAt;
            pointerHold.idle = 0;
            pointerHold.hold(true);
        }
    }
    Component.onCompleted: timedHold.hold(Theme.dotMotion.timed && visible)
    Component.onDestruction: {
        timedHold.hold(false);
        pointerHold.hold(false);
    }

    QtObject {
        id: timedHold

        readonly property bool wanted: Theme.dotMotion.timed && root.visible
        property bool held: false

        function hold(want) {
            if (want === held) {
                return;
            }
            held = want;
            if (want) {
                FrameClock.acquire();
            } else {
                FrameClock.release();
            }
        }

        onWantedChanged: hold(wanted)
    }

    QtObject {
        id: pointerHold

        property bool held: false
        property point shown: root.offItem
        property int idle: 0

        function hold(want) {
            if (want === held) {
                return;
            }
            held = want;
            if (want) {
                FrameClock.acquire();
            } else {
                FrameClock.release();
            }
        }
    }

    Connections {
        target: FrameClock
        enabled: pointerHold.held

        function onFrameChanged() {
            if (pointerHold.shown.x !== root.pointerAt.x || pointerHold.shown.y !== root.pointerAt.y) {
                pointerHold.shown = root.pointerAt;
                pointerHold.idle = 0;
            } else if (++pointerHold.idle >= root.pointerIdleTicks) {
                pointerHold.hold(false);
            }
        }
    }
}
