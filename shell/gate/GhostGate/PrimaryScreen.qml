pragma Singleton

import QtQuick

// A greeter makes one window per screen on one engine, and the keyboard may land on any of them; this is how the
// others reach the one that holds the prompt.
QtObject {
    property Item screen: null
}
