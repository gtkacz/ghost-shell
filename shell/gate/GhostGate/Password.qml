import QtQuick
import "lib/gate.mjs" as GateLib
import "lib/theme.mjs" as ThemeLib

// One lit cell per character; the characters themselves live only in the hidden input below and never reach the screen.
Item {
    id: root

    // Design pixels, from the reference mockup's .pw.
    readonly property real cellWidthPx: 9
    readonly property real cellHeightPx: 14
    readonly property real cursorHeightPx: 16
    readonly property real gapPx: 3
    readonly property real rowHeightPx: 18
    // The mockup's unlit cell strength, as a percentage like withAlpha() takes.
    readonly property real unlitAlphaPct: 6

    property bool busy: false
    readonly property int length: input.text.length
    readonly property var gauge: GateLib.cells(length)
    readonly property int lit: gauge.lit
    readonly property int cursor: gauge.cursor

    property int blinkStep: 0

    signal submitted(string password)

    function focusField() {
        input.forceActiveFocus();
    }

    implicitWidth: Theme.d(GateLib.CELLS * cellWidthPx + (GateLib.CELLS - 1) * gapPx)
    implicitHeight: Theme.d(rowHeightPx)

    TextInput {
        id: input

        width: 1
        height: 1
        opacity: 0
        focus: true
        echoMode: TextInput.NoEcho
        readOnly: root.busy
        inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText

        onAccepted: {
            if (root.busy || text === "") {
                return;
            }
            const password = text;
            text = "";
            root.submitted(password);
        }
        Keys.onEscapePressed: {
            if (!root.busy) {
                text = "";
            }
        }
    }

    Row {
        spacing: Theme.d(root.gapPx)

        Repeater {
            model: GateLib.CELLS

            Item {
                id: cell

                required property int index
                readonly property bool isCursor: index === root.cursor && !root.busy

                width: Theme.d(root.cellWidthPx)
                height: Theme.d(root.rowHeightPx)

                Rectangle {
                    anchors.centerIn: parent
                    width: parent.width
                    height: Theme.d(cell.isCursor ? root.cursorHeightPx : root.cellHeightPx)
                    color: cell.isCursor || cell.index < root.lit ? Theme.signal : ThemeLib.withAlpha(Theme.signal, root.unlitAlphaPct)
                    opacity: cell.isCursor ? Theme.blinkSteps[root.blinkStep] : 1
                }
            }
        }
    }

    Timer {
        interval: Theme.blinkStepMs
        repeat: true
        running: !root.busy && root.visible
        onTriggered: root.blinkStep = (root.blinkStep + 1) % Theme.blinkSteps.length
    }
}
