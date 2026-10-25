import QtQuick
import "lib/theme.mjs" as ThemeLib

// A short status word in a thin box, from the reference mockup's .chip. It reads katakana in the default
// Japanese mode, and is absent when the mode is off.
Item {
    id: root

    // Design pixels, from the reference mockup's .chip.
    readonly property real textPx: 9
    readonly property real linePx: 14
    readonly property real sidePx: 5
    readonly property real borderPx: 1
    // Letter spacing in em.
    readonly property real tracking: 0.04
    // The mockup's border strength, as a percentage like withAlpha() takes.
    readonly property real borderAlphaPct: 35

    // A Theme.chip() key.
    property string key
    property bool scrambles: true
    property color color: Theme.signal
    // The mockup's .alert .chip sets the border to the full text colour instead.
    property color borderColor: ThemeLib.withAlpha(color, borderAlphaPct)

    readonly property bool hasText: label.text !== ""

    visible: hasText
    implicitWidth: hasText ? label.implicitWidth + Theme.d(2 * (sidePx + borderPx)) : 0
    implicitHeight: hasText ? Theme.d(linePx + 2 * borderPx) : 0

    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border {
            width: Theme.d(root.borderPx)
            color: root.borderColor
        }
    }

    ScrambleText {
        id: label

        anchors.centerIn: parent
        text: Theme.chip(root.key)
        scrambles: root.scrambles
        textFormat: Text.PlainText
        color: root.color
        font {
            family: Theme.fontCjk
            pixelSize: Theme.d(root.textPx)
            weight: Font.Medium
            letterSpacing: Theme.d(root.textPx) * root.tracking
        }
    }
}
