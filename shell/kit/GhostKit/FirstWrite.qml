pragma Singleton

import QtQuick
import Quickshell
import "lib/firstwrite.mjs" as FirstWriteLib

// Which texts have already scrambled into place, so each does it only the first time it is written in
// each showing of its widget: a tab switch, a view arriving or a restyle shows the widget anew, while
// an uncover rebuilds it without showing it anew. The greeting calls reset() to let the dashboard's
// labels scramble again as it is revealed.
Singleton {
    readonly property var claimed: new Set()
    // Each host scope's slot signature as it last showed it, which outlives the host across a cover.
    readonly property var shownSlots: new Map()
    // Read once, so with the flag off a claim logs nothing. A headless check reads the scrambles from the log.
    readonly property bool logsClaims: Quickshell.env("GHOST_DEBUG") === "1"
    // The scope of a widget being rebuilt rather than shown anew, while createIn() creates it.
    property string rebuilding: ""

    // True the first time a key is claimed since the last reset() or new showing of its scope, unless its
    // widget is being rebuilt.
    function claim(key) {
        if (claimed.has(key)) {
            return false;
        }
        claimed.add(key);
        if (rebuilding !== "" && key.startsWith(FirstWriteLib.scopedKey(rebuilding, ""))) {
            return false;
        }
        if (logsClaims) {
            console.info("scramble " + key);
        }
        return true;
    }

    function hostScope(view, path) {
        return FirstWriteLib.hostScope(view, path);
    }

    // Called before the view's widgets are created.
    function showView(view) {
        FirstWriteLib.forget(claimed, FirstWriteLib.viewPrefix(view));
    }

    // Called before the host creates the slot's widget. The same slot again is a rebuild, as after an
    // uncover or a layout re-sent unchanged; another tab's slot or new settings are a new showing.
    function showSlot(scope, slot) {
        const signature = FirstWriteLib.slotSignature(slot);
        if (shownSlots.get(scope) === signature) {
            return;
        }
        shownSlots.set(scope, signature);
        FirstWriteLib.forget(claimed, FirstWriteLib.scopedKey(scope, ""));
    }

    // Creates a host's widget. Rebuilt, as after an uncover, its texts claim their first writes without
    // scrambling: a row that did not exist before the cover is not a new showing either.
    function createIn(scope, create) {
        rebuilding = FirstWriteLib.holdsClaims(claimed, scope) ? scope : "";
        try {
            return create();
        } finally {
            rebuilding = "";
        }
    }

    function reset() {
        claimed.clear();
    }
}
