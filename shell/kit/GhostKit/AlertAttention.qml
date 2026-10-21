pragma Singleton

import QtQuick
import Quickshell
import "lib/attention.mjs" as Attention

// How much of its attention window each alert has spent this session, counted only while a box shows it, so an
// alert draws the eye for its whole window even if a cover, a view switch or a widget reload took it off the screen
// for a while. It reads FrameClock.elapsedMs, which stands still while the desktop is covered and is unmoved by
// wall-clock steps, and which an attracting box keeps running for exactly as long as it counts.
Singleton {
    readonly property var seen: new Map()

    function show(key) {
        Attention.showAlert(seen, key, FrameClock.elapsedMs);
    }

    function hide(key) {
        Attention.hideAlert(seen, key, FrameClock.elapsedMs);
    }

    function attentionLeftMs(key) {
        return Attention.attentionLeftMs(seen, key, FrameClock.elapsedMs);
    }

    function spend(key) {
        Attention.spendAttention(seen, key, FrameClock.elapsedMs);
    }
}
