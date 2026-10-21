import QtQuick
import "lib/theme.mjs" as ThemeLib

// Text whose digits never shift as they change. A font with equal digit widths already guarantees that;
// with any other font each digit sits centred in a cell as wide as "0".
Item {
    id: root

    readonly property var digitPattern: /^[0-9]$/

    property string text
    required property real pixelSize
    property int weight: Font.Bold
    property color color: Data.connected && !Data.stale ? Theme.hi : Theme.dim

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

    Text {
        id: whole

        width: root.width
        visible: root.monospace
        text: root.text
        color: root.color
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: root.pixelSize
            weight: root.weight
        }
    }

    Row {
        id: cells

        visible: !root.monospace

        Repeater {
            // A count rather than the characters, so a changing value rebinds the cells instead of rebuilding them.
            model: root.monospace ? 0 : root.text.length

            Text {
                required property int index
                readonly property string character: root.text.charAt(index)

                width: root.digitPattern.test(character) ? root.cellWidth : implicitWidth
                // A glyph wider than "0" would otherwise spill into its neighbours' cells.
                clip: true
                horizontalAlignment: Text.AlignHCenter
                text: character
                color: root.color
                font: whole.font
            }
        }
    }
}
