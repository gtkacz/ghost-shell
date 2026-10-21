pragma Singleton

import QtQuick
import Quickshell

// Which texts have already scrambled into place this session, so each does it only the first time it
// is written. The greeting calls reset() to let the dashboard's labels scramble again as it is revealed.
Singleton {
    readonly property var claimed: new Set()

    // True the first time a key is claimed since the last reset().
    function claim(key) {
        if (claimed.has(key)) {
            return false;
        }
        claimed.add(key);
        return true;
    }

    function reset() {
        claimed.clear();
    }
}
