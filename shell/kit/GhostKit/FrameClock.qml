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
    // 33 ms is 30.30 frames/s, for the one thing that needs it: a visible, playing spectrum (spike decision 3).
    readonly property int fastIntervalMs: 33
    readonly property int blinkStepMs: 250
    readonly property var blinkSteps: [1, 0.5, 0, 0.5]

    readonly property bool running: ticker.running
    readonly property int frame: ticker.frame
    readonly property real elapsedMs: ticker.elapsedMs
    readonly property real blinkOpacity: blinkSteps[Math.floor(elapsedMs / blinkStepMs) % blinkSteps.length]
    // Whole seconds since the epoch, kept current only while something holds acquireSeconds().
    readonly property double epochSecond: seconds.epoch
    // Wall-clock milliseconds as of the latest tick, which is as current as the ticker runs: unlike elapsedMs it
    // does not drift from the wall clock, so a holder of acquire() can step on wall-clock boundaries with it.
    readonly property double epochMs: ticker.epochMs

    // Read once, so with the flag off a hold change costs one bool read and logs nothing.
    // scripts/smoke-holds.sh reads the counts from the log to find a holder that never releases.
    readonly property bool logsHolds: Quickshell.env("GHOST_DEBUG") === "1"

    function acquire() {
        ticker.users += 1;
        ticker.epochMs = Date.now();
        if (logsHolds) {
            logHolds();
        }
    }

    function release() {
        if (ticker.users === 0) {
            console.warn("FrameClock.release() called without a matching acquire()");
            return;
        }
        ticker.users -= 1;
        if (logsHolds) {
            logHolds();
        }
    }

    // Runs the clock like acquire(), at the fast interval for as long as any fast holder remains.
    function acquireFast() {
        ticker.fastUsers += 1;
        if (logsHolds) {
            logHolds();
        }
    }

    function releaseFast() {
        if (ticker.fastUsers === 0) {
            console.warn("FrameClock.releaseFast() called without a matching acquireFast()");
            return;
        }
        ticker.fastUsers -= 1;
        if (logsHolds) {
            logHolds();
        }
    }

    function acquireSeconds() {
        seconds.users += 1;
        seconds.sync();
        if (logsHolds) {
            logHolds();
        }
    }

    function releaseSeconds() {
        if (seconds.users === 0) {
            console.warn("FrameClock.releaseSeconds() called without a matching acquireSeconds()");
            return;
        }
        seconds.users -= 1;
        if (logsHolds) {
            logHolds();
        }
    }

    function logHolds() {
        console.info("holds normal=" + ticker.users + " fast=" + ticker.fastUsers + " seconds=" + seconds.users);
    }

    // A shell whose widgets never hold the clock still logs its counts once, so the harness has a baseline.
    Component.onCompleted: {
        if (logsHolds) {
            logHolds();
        }
    }

    Timer {
        id: ticker

        property int users: 0
        property int fastUsers: 0
        property int frame: 0
        property real elapsedMs: 0
        property double epochMs: Date.now()

        interval: fastUsers > 0 ? root.fastIntervalMs : root.intervalMs
        repeat: true
        running: users + fastUsers > 0
        onTriggered: {
            frame += 1;
            epochMs = Date.now();
            elapsedMs += interval;
            // The tick that crosses a second boundary carries the new second, so a held clock needs no
            // timer of its own.
            if (seconds.users > 0) {
                seconds.sync();
            }
        }
    }

    QtObject {
        id: seconds

        readonly property int msPerSecond: 1000

        property int users: 0
        property double epoch: Math.floor(Date.now() / msPerSecond)

        // True while the ticker is stopped and a seconds holder remains: the single-shot timer below is
        // then the only source of frames, and it stops the moment the ticker resumes.
        readonly property bool aloneWanted: users > 0 && !ticker.running

        function sync() {
            epoch = Math.floor(Date.now() / msPerSecond);
        }

        onAloneWantedChanged: {
            if (aloneWanted) {
                alone.arm();
            } else {
                alone.stop();
            }
        }
    }

    // Re-armed to each wall-clock second boundary rather than repeating at 1000 ms, which would drift
    // away from the boundary and show a second late.
    Timer {
        id: alone

        function arm() {
            seconds.sync();
            interval = seconds.msPerSecond - Date.now() % seconds.msPerSecond;
            restart();
        }

        repeat: false
        onTriggered: arm()
    }
}
