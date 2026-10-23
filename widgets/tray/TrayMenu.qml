import QtQuick
import Quickshell
import GhostKit

// A status item's menu in the panel style: one column of its entries, and beside it a column for each submenu opened
// so far. Each entry that has children opens them on click; any other entry is triggered and closes the menu.
Row {
    id: root

    // Design pixels, from the reference mockup's .tip.
    readonly property real textPx: 9.5
    readonly property real padXPx: 8
    readonly property real padYPx: 2
    readonly property real rowHeightPx: 18
    readonly property real separatorHeightPx: 5
    readonly property real markPx: 4
    readonly property real markGapPx: 6
    readonly property real fillAlpha: 0.96
    readonly property real hoverFillAlpha: 0.08
    // Letter spacing in em.
    readonly property real tracking: 0.04

    required property var menu
    // The entries whose submenus are open, outermost first.
    property var path: []

    signal triggered

    readonly property font menuFont: Qt.font({
        family: Theme.font,
        pixelSize: Theme.d(textPx),
        weight: Font.DemiBold,
        letterSpacing: Theme.d(textPx) * tracking
    })

    // The & marks a mnemonic key in the text a menu hands over.
    function labelOf(entry) {
        return entry.text.replace(/&(.)/g, "$1").toUpperCase();
    }

    Repeater {
        model: 1 + root.path.length

        Rectangle {
            id: column

            required property int index

            readonly property var handle: index === 0 ? root.menu : root.path[index - 1]

            anchors.top: parent.top
            implicitWidth: list.implicitWidth + Theme.d(2 * root.padXPx)
            implicitHeight: list.implicitHeight + Theme.d(2 * root.padYPx)
            color: Qt.alpha(Theme.ground, root.fillAlpha)
            border {
                width: Theme.d(1)
                color: Theme.hair
            }

            QsMenuOpener {
                id: opener

                menu: column.handle
            }

            Column {
                id: list

                x: Theme.d(root.padXPx)
                y: Theme.d(root.padYPx)

                Repeater {
                    model: opener.children

                    Item {
                        id: row

                        required property var modelData

                        readonly property bool checked: modelData.buttonType !== QsMenuButtonType.None && modelData.checkState === Qt.Checked

                        width: Theme.d(root.markPx + root.markGapPx) + label.implicitWidth + (modelData.hasChildren ? Theme.d(root.markGapPx + root.textPx) : 0)
                        height: Theme.d(modelData.isSeparator ? root.separatorHeightPx : root.rowHeightPx)

                        Rectangle {
                            visible: row.modelData.isSeparator
                            x: -Theme.d(root.padXPx)
                            anchors.verticalCenter: parent.verticalCenter
                            width: column.width
                            height: Theme.d(1)
                            color: Theme.hair2
                        }

                        Rectangle {
                            visible: !row.modelData.isSeparator
                            x: -Theme.d(root.padXPx)
                            width: column.width
                            height: parent.height
                            color: area.containsMouse && row.modelData.enabled ? Qt.alpha(Theme.signal, root.hoverFillAlpha) : "transparent"
                        }

                        Rectangle {
                            visible: row.checked
                            anchors.verticalCenter: parent.verticalCenter
                            width: Theme.d(root.markPx)
                            height: width
                            color: Theme.signal
                        }

                        Text {
                            id: label

                            visible: !row.modelData.isSeparator
                            anchors.verticalCenter: parent.verticalCenter
                            x: Theme.d(root.markPx + root.markGapPx)
                            text: root.labelOf(row.modelData)
                            textFormat: Text.PlainText
                            color: !row.modelData.enabled ? Theme.dim : (area.containsMouse ? Theme.hi : Theme.text)
                            font: root.menuFont
                        }

                        Glyph {
                            visible: row.modelData.hasChildren
                            anchors {
                                right: parent.right
                                verticalCenter: parent.verticalCenter
                            }
                            name: "chevron-right"
                            size: Theme.d(root.textPx)
                            color: Theme.dim
                        }

                        MouseArea {
                            id: area

                            x: -Theme.d(root.padXPx)
                            width: column.width
                            height: parent.height
                            enabled: !row.modelData.isSeparator && row.modelData.enabled
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (row.modelData.hasChildren) {
                                    root.path = root.path.slice(0, column.index).concat([row.modelData]);
                                    return;
                                }
                                row.modelData.triggered();
                                root.triggered();
                            }
                        }
                    }
                }
            }
        }
    }
}
