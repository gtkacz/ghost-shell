import QtQuick
import "lib/theme.mjs" as ThemeLib

// Text whose digits never shift as they change. A font with equal digit widths already guarantees that;
// with any other font each digit sits centred in a cell as wide as "0".
Item {
    id: root

    readonly property var digitPattern: /^[0-9]$/

    property string text
    property bool scrambles: true
    required property real pixelSize
    property int weight: Font.Bold
    // Letter spacing in em. The cells below measure "0" with it, so a font without equal digit widths keeps the
    // same spacing between its cells.
    property real tracking: 0
    // Whether the value's own source is live, such as Data.topic(name).live.
    property bool sourceLive: true
    readonly property bool live: Data.connected && !Data.stale && sourceLive
    property color color: live ? Theme.hi : Theme.dim

    readonly property bool monospace: ThemeLib.isMonospace(zero.advanceWidth, one.advanceWidth)
    readonly property real cellWidth: zero.advanceWidth

    implicitWidth: monospace ? whole.implicitWidth : cells.implicitWidth
    implicitHeight: whole.implicitHeight
    baselineOffset: whole.baselineOffset

    // TextMetrics rather than FontMetrics.advanceWidth(), whose result a binding would not refresh when the
    // font changes, for example once the surface sets the design unit.
    TextMetrics {
        id: zero

        font: whole.font
        text: "0"
    }

    TextMetrics {
        id: one

        font: whole.font
        text: "1"
    }

    // Hidden by opacity rather than visibility, so it still scrambles its first write and the cells
    // below can show the scramble's frames.
    ScrambleText {
        id: whole

        width: root.width
        opacity: root.monospace ? 1 : 0
        text: root.text
        scrambles: root.scrambles
        // Callers pass daemon and provider values, which AutoText would render as markup.
        textFormat: Text.PlainText
        color: root.color
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: root.pixelSize
            weight: root.weight
            letterSpacing: root.pixelSize * root.tracking
        }
    }

    InlineRow {
        id: cells

        visible: !root.monospace

        Repeater {
            // A count rather than the characters, so a changing value rebinds the cells instead of rebuilding them.
            model: root.monospace ? 0 : whole.text.length

            Text {
                required property int index
                readonly property string character: whole.text.charAt(index)

                width: root.digitPattern.test(character) ? root.cellWidth : implicitWidth
                // A glyph wider than "0" would otherwise spill into its neighbours' cells.
                clip: true
                horizontalAlignment: Text.AlignHCenter
                text: character
                textFormat: Text.PlainText
                color: root.color
                font: whole.font
            }
        }
    }
}
