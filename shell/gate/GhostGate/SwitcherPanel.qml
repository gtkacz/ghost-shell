import QtQuick
import "lib/switcher.mjs" as Switcher
import "lib/theme.mjs" as ThemeLib

// The Readout layout of the window switcher: every window's name on the left and the selected one's live preview on the
// right. It knows nothing of KWin: the layout's main.qml hands it the model, the selection and the thumbnail.
Item {
    id: root

    // Design pixels, from the mockup's layout C.
    readonly property real panelWidthPx: 950
    readonly property real panelHeightPx: 390
    readonly property real paddingPx: 18
    readonly property real headerHeightPx: 34
    readonly property real headerTopPx: 14
    readonly property real headerBottomPx: 8
    readonly property real listWidthPx: 366
    readonly property real columnGapPx: 18
    readonly property real rowHeightPx: 30
    readonly property real rowPaddingPx: 8
    readonly property real rowGapPx: 8
    readonly property real rowBorderPx: 1
    readonly property real numberWidthPx: 14
    readonly property real numberTextPx: 9
    readonly property real iconPx: 12
    readonly property real appWidthPx: 62
    readonly property real rowTextPx: 10
    readonly property real previewWidthPx: 528
    readonly property real previewHeightPx: 299
    readonly property real previewBorderPx: 1
    readonly property real captionTopPx: 8
    readonly property real captionGapPx: 6
    readonly property real captionTextPx: 11
    readonly property real bracketArmPx: 12
    readonly property real bracketWidthPx: 1.5
    readonly property real bracketOutsetPx: 5
    readonly property int shownRows: 10
    readonly property int panelFillPct: 94
    readonly property int selectedFillPct: 8

    property var model
    property int currentIndex: 0
    property var nameOf: (windowId, caption) => Switcher.appName("", "", caption)
    default property alias preview: previewSlot.data

    readonly property var currentWindowId: list.currentItem ? list.currentItem.windowId : undefined
    readonly property int printStep: printer.step

    signal activated(int index)

    width: Theme.d(panelWidthPx)
    height: Theme.d(panelHeightPx)

    Timer {
        id: printer

        property int step: 1

        objectName: "printer"
        interval: Math.round(Switcher.PRINT_MS / Switcher.PRINT_STEPS)
        repeat: true
        running: step < Switcher.PRINT_STEPS
        onTriggered: step += 1
    }

    // Around the panel rather than inside it, so the print takes the frame down with the contents.
    Item {
        width: root.width
        height: root.height * root.printStep / Switcher.PRINT_STEPS
        clip: true

        Panel {
            id: frame

            width: root.width
            height: root.height
            label: "SWITCH WINDOW"
            tagKey: "switch"
            meta: (list.count > 0 ? root.currentIndex + 1 : 0) + " / " + list.count
            scrambles: false
            fill: ThemeLib.withAlpha(Theme.panel, root.panelFillPct)
            padding: Theme.d(root.paddingPx)
            headerHeightPx: root.headerHeightPx
            headerTopPx: root.headerTopPx
            headerBottomPx: root.headerBottomPx
            headerSidePx: root.paddingPx

            ListView {
                id: list

                function keepSelectionInView() {
                    positionViewAtIndex(currentIndex, ListView.Contain);
                }

                objectName: "list"
                width: Theme.d(root.listWidthPx)
                height: Theme.d(root.rowHeightPx) * root.shownRows
                model: root.model
                currentIndex: root.currentIndex
                interactive: false
                clip: true
                highlightFollowsCurrentItem: false
                onCurrentIndexChanged: keepSelectionInView()
                onCountChanged: keepSelectionInView()
                // The rows change size when the layout fits the theme after building the panel. This fires once they have moved,
                // where the list's own height changes too early, and deferred because it fires during the list's layout.
                onContentHeightChanged: Qt.callLater(keepSelectionInView)
                Component.onCompleted: keepSelectionInView()

                delegate: Rectangle {
                    id: row

                    required property string caption
                    required property var windowId
                    required property var icon
                    required property int index

                    readonly property bool selected: ListView.isCurrentItem
                    // Asked once, not bound: KWin's stacking order, which the layout's nameOf reads, has no change signal, so
                    // a binding logs a warning per row per opening. A window's app does not change while the panel is open.
                    property string app
                    readonly property string title: Switcher.titleWithoutApp(caption, app)

                    Component.onCompleted: app = root.nameOf(windowId, caption)

                    width: ListView.view.width
                    height: Theme.d(root.rowHeightPx)
                    color: selected ? ThemeLib.withAlpha(Theme.signal, root.selectedFillPct) : "transparent"
                    border.color: selected ? Theme.signal : "transparent"
                    border.width: Theme.d(root.rowBorderPx)

                    Text {
                        id: number

                        anchors {
                            left: parent.left
                            leftMargin: Theme.d(root.rowPaddingPx)
                            verticalCenter: parent.verticalCenter
                        }
                        width: Theme.d(root.numberWidthPx)
                        text: Switcher.rowNumber(row.index)
                        textFormat: Text.PlainText
                        color: Theme.dim
                        font {
                            family: Theme.font
                            pixelSize: Theme.d(root.numberTextPx)
                        }
                    }

                    SwitcherIcon {
                        id: rowIcon

                        objectName: "row-icon"
                        anchors {
                            left: number.right
                            leftMargin: Theme.d(root.rowGapPx)
                            verticalCenter: parent.verticalCenter
                        }
                        width: Theme.d(root.iconPx)
                        height: width
                        source: row.icon
                        selected: row.selected
                    }

                    Text {
                        id: appText

                        anchors {
                            left: rowIcon.right
                            leftMargin: Theme.d(root.rowGapPx)
                            verticalCenter: parent.verticalCenter
                        }
                        width: Theme.d(root.appWidthPx)
                        text: row.app
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: row.selected ? Theme.signal : Theme.dim
                        font {
                            family: Theme.font
                            pixelSize: Theme.d(root.rowTextPx)
                        }
                    }

                    Text {
                        anchors {
                            left: appText.right
                            leftMargin: Theme.d(root.rowGapPx)
                            right: parent.right
                            rightMargin: Theme.d(root.rowPaddingPx)
                            verticalCenter: parent.verticalCenter
                        }
                        text: row.title
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: row.selected ? Theme.hi : Theme.text
                        font {
                            family: Theme.font
                            pixelSize: Theme.d(root.rowTextPx)
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: root.activated(row.index)
                    }
                }
            }

            // Over the list rather than in the row: the brackets stand outside the row, and the list's clip would cut them.
            Item {
                id: brackets

                readonly property real outset: Theme.d(root.bracketOutsetPx)
                readonly property real arm: Theme.d(root.bracketArmPx)
                readonly property real thickness: Theme.d(root.bracketWidthPx)

                visible: list.currentItem !== null
                x: list.x - outset
                y: list.y - outset + (list.currentItem ? list.currentItem.y - list.contentY : 0)
                width: list.width + outset * 2
                height: Theme.d(root.rowHeightPx) + outset * 2

                Rectangle {
                    width: brackets.arm
                    height: brackets.thickness
                    color: Theme.signal
                }

                Rectangle {
                    width: brackets.thickness
                    height: brackets.arm
                    color: Theme.signal
                }

                Rectangle {
                    anchors {
                        right: parent.right
                        bottom: parent.bottom
                    }
                    width: brackets.arm
                    height: brackets.thickness
                    color: Theme.signal
                }

                Rectangle {
                    anchors {
                        right: parent.right
                        bottom: parent.bottom
                    }
                    width: brackets.thickness
                    height: brackets.arm
                    color: Theme.signal
                }
            }

            Glyph {
                objectName: "more-above"
                anchors {
                    horizontalCenter: list.horizontalCenter
                    bottom: list.top
                }
                name: "chevron-up"
                color: Theme.signal
                visible: !list.atYBeginning
            }

            Glyph {
                objectName: "more-below"
                anchors {
                    horizontalCenter: list.horizontalCenter
                    top: list.bottom
                }
                name: "chevron-down"
                color: Theme.signal
                visible: !list.atYEnd
            }

            Rectangle {
                id: previewFrame

                anchors {
                    left: list.right
                    leftMargin: Theme.d(root.columnGapPx)
                }
                width: Theme.d(root.previewWidthPx)
                height: Theme.d(root.previewHeightPx)
                color: "transparent"
                border.color: Theme.hair
                border.width: Theme.d(root.previewBorderPx)

                Item {
                    id: previewSlot

                    anchors {
                        fill: parent
                        margins: previewFrame.border.width
                    }
                }
            }

            SwitcherIcon {
                id: captionIcon

                anchors {
                    left: previewFrame.left
                    top: previewFrame.bottom
                    topMargin: Theme.d(root.captionTopPx)
                }
                width: Theme.d(root.iconPx)
                height: width
                source: list.currentItem ? list.currentItem.icon : ""
                selected: true
            }

            Text {
                anchors {
                    left: captionIcon.right
                    leftMargin: Theme.d(root.captionGapPx)
                    right: previewFrame.right
                    verticalCenter: captionIcon.verticalCenter
                }
                text: list.currentItem ? list.currentItem.app + " · " + list.currentItem.title : ""
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: Theme.hi
                font {
                    family: Theme.font
                    pixelSize: Theme.d(root.captionTextPx)
                }
            }
        }

        Scanlines {
            anchors.fill: frame
        }
    }
}
