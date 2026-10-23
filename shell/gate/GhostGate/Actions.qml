import QtQuick
import "lib/gate.mjs" as GateLib

// The row of icon and label buttons, dim until hovered.
Row {
    id: root

    // Design pixels, from the reference mockup's .act.
    readonly property real gapPx: 18
    readonly property real innerGapPx: 6
    readonly property real iconPx: 13
    readonly property real textPx: 9.5
    // Letter spacing in em.
    readonly property real tracking: 0.14

    property var adapter
    readonly property var names: GateLib.actionsFor(adapter.mode, adapter.allowed)
    readonly property var labels: names.map((name) => GateLib.actionLabel(name))

    spacing: Theme.d(gapPx)

    Repeater {
        model: root.names

        Item {
            id: action

            required property string modelData

            width: line.implicitWidth
            height: line.implicitHeight

            Row {
                id: line

                spacing: Theme.d(root.innerGapPx)

                Glyph {
                    anchors.verticalCenter: parent.verticalCenter
                    name: action.modelData
                    size: Theme.d(root.iconPx)
                    color: hover.containsMouse ? Theme.signal : Theme.dim
                }

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: GateLib.actionLabel(action.modelData)
                    textFormat: Text.PlainText
                    color: hover.containsMouse ? Theme.hi : Theme.dim
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.textPx)
                        letterSpacing: Theme.d(root.textPx) * root.tracking
                    }
                }
            }

            MouseArea {
                id: hover

                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.adapter.act(action.modelData)
            }

        }
    }
}
