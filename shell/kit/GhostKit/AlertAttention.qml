pragma Singleton

import QtQuick
import Quickshell
import "lib/attention.mjs" as Attention

// How much of its attention window each alert has spent this session, counted only while a box shows it, so an
// alert draws the eye for its whole window even if a cover, a view switch or a widget reload took it off the screen
// for a while. It reads FrameClock.elapsedMs, which stands still while the desktop is covered and is unmoved by
// wall-clock steps, and which an attracting box keeps running for exactly as long as it counts. The window is
// Theme.alertAttentionMs, read at each call so that a live change of the setting applies to alerts part-way through.
Singleton {
    readonly property var seen: new Map()

    function show(key) {
        Attention.showAlert(seen, key, FrameClock.elapsedMs);
    }

    function hide(key) {
        Attention.hideAlert(seen, key, FrameClock.elapsedMs, Theme.alertAttentionMs);
    }

    function attentionLeftMs(key) {
        return Attention.attentionLeftMs(seen, key, FrameClock.elapsedMs, Theme.alertAttentionMs);
    }

    function spend(key) {
        Attention.spendAttention(seen, key, FrameClock.elapsedMs);
    }

    // A hidden alert has no box to notice a shorter window, so this marks it spent, which a longer window set later
    // must not undo.
    Connections {
        target: Theme

        function onAlertAttentionMsChanged() {
            Attention.spendExhausted(seen, FrameClock.elapsedMs, Theme.alertAttentionMs);
        }
    }
}
