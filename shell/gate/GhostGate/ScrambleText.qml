import QtQuick

// Panel and Chip scramble their text in GhostKit, which needs Quickshell's clock. The gate never scrambles, so this
// takes the same properties and ignores them.
Text {
    property real durationMs
    property string scrambleKey
    property bool scrambles

    textFormat: Text.PlainText
}
