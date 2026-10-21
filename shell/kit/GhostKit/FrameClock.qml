pragma Singleton

import QtQuick
import Quickshell

// The only clock for decorative motion. Every animation steps on its tick, because each independent
// timer or QML Animation adds frames of its own (measured: Animation types run at about 62.5 fps and
// cost three times the CPU).
Singleton {
    id: root

    // 67 ms is 14.93 frames/s; 66 ms would be 15.15 and break the 15 frames/s ceiling.
    readonly property int intervalMs: 67
    readonly property int blinkStepMs: 250
    readonly property var blinkSteps: [1, 0.5, 0, 0.5]

    readonly property int frame: ticker.frame
    readonly property real elapsedMs: ticker.elapsedMs
    readonly property real blinkOpacity: blinkSteps[Math.floor(elapsedMs / blinkStepMs) % blinkSteps.length]

    function acquire() {
        ticker.users += 1;
    }

    function release() {
        if (ticker.users === 0) {
            console.warn("FrameClock.release() called without a matching acquire()");
            return;
        }
        ticker.users -= 1;
    }

    Timer {
        id: ticker

        property int users: 0
        property int frame: 0
        property real elapsedMs: 0

        interval: root.intervalMs
        repeat: true
        running: users > 0
        onTriggered: {
            frame += 1;
            elapsedMs += interval;
        }
    }
}
