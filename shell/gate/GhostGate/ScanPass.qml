import QtQuick
import "lib/gate.mjs" as GateLib
import "lib/theme.mjs" as ThemeLib

// The success pass: a 2 px light line that sweeps down in held steps. The screen hides everything above `lineY`, so
// the pass ends on whatever is under the prompt.
Item {
    id: root

    // Design pixels and the mockup's .scan strength, as a percentage like withAlpha() takes.
    readonly property real lineHeightPx: 2
    readonly property real lineAlphaPct: 55

    property bool running: false
    property int step: 0
    property real startedAt: 0
    readonly property real lineY: height * step / GateLib.SCAN_STEPS

    signal finished

    function start() {
        startedAt = Date.now();
        step = 0;
        running = true;
    }

    Timer {
        interval: GateLib.SCAN_TICK_MS
        repeat: true
        running: root.running
        onTriggered: {
            root.step = GateLib.scanStep(Date.now() - root.startedAt);
            if (root.step >= GateLib.SCAN_STEPS) {
                root.running = false;
                root.finished();
            }
        }
    }

    Rectangle {
        visible: root.running
        y: root.lineY
        width: parent.width
        height: Theme.d(root.lineHeightPx)
        color: ThemeLib.withAlpha(Theme.hi, root.lineAlphaPct)
    }
}
