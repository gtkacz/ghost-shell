pragma Singleton

import QtQuick
import Quickshell
import "lib/attention.mjs" as Attention

// When each alert was first shown this session, so an alert draws the eye once: recreating the same one, for
// example after a view switch, a widget reload or an uncover, does not blink again.
Singleton {
    readonly property var seen: new Map()

    // The time the alert with this key was first shown, which is now for a key never shown before.
    function firstShown(key) {
        return Attention.firstShown(seen, key, Date.now());
    }

    function attentionLeftMs(key) {
        return Attention.attentionLeftMs(firstShown(key), Date.now());
    }
}
