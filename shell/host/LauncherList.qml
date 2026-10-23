import QtQuick
import GhostKit
import "../kit/GhostKit/lib/launcher.mjs" as LauncherLib

// The launcher's scrolling list: the pins as a grid, the recent files and the apps, or the ranked results of a query.
Item {
    id: root

    // Design pixels, from the launcher mockup's .sub, .pins, .pin, .row and .file.
    readonly property real headTopPx: 8
    readonly property real headBottomPx: 4
    readonly property real headSidePx: 14
    readonly property real headGapPx: 8
    readonly property real headTextPx: 9
    readonly property real tagOpacity: 0.7
    readonly property real pinsSidePx: 10
    readonly property real pinsTopPx: 4
    readonly property real pinsBottomPx: 2
    readonly property real pinGapPx: 2
    readonly property real pinPadTopPx: 7
    readonly property real pinPadBottomPx: 5
    readonly property real pinPadSidePx: 2
    readonly property real pinIconPx: 22
    readonly property real pinNameGapPx: 4
    readonly property real pinNamePx: 8.5
    readonly property real appRowPx: 26
    readonly property real appPadLeftPx: 12
    readonly property real appPadRightPx: 14
    readonly property real appIconPx: 17
    readonly property real appGapPx: 10
    readonly property real appNamePx: 11
    readonly property real genericPx: 10
    readonly property real genericGapPx: 8
    readonly property real edgePx: 2
    readonly property real chipPx: 9
    readonly property real chipSidePx: 5
    readonly property real chipLinePx: 14
    readonly property real filePx: 24
    readonly property real filePadPx: 14
    readonly property real fileIconPx: 12
    readonly property real fileNamePx: 10.5
    readonly property real fileMetaPx: 9.5
    readonly property real emptyPadPx: 14
    readonly property real emptyPx: 10
    // The selected row's fill, as the mockup's rgba(163, 212, 204, .09), and the pin's .07.
    readonly property real rowFillAlpha: 0.09
    readonly property real pinFillAlpha: 0.07
    readonly property real chipBorderAlpha: 0.35
    // Letter spacing in em.
    readonly property real headTracking: 0.18
    readonly property real tagTracking: 0.04
    readonly property real pinNameTracking: 0.04
    readonly property real chipTracking: 0.08
    readonly property real emptyTracking: 0.1
    readonly property int pinColumns: LauncherLib.PIN_COLUMNS
    readonly property real secondsPerMs: 0.001

    readonly property var rowList: LauncherLib.rows(Launcher.items, Launcher.query)
    readonly property bool noMatch: Launcher.query !== "" && rowList.length === 0

    function chipFont() {
        return Qt.font({
            family: Theme.font,
            pixelSize: Theme.d(chipPx),
            weight: Font.Medium,
            letterSpacing: Theme.d(chipPx) * chipTracking
        });
    }

    Connections {
        target: Launcher

        function onIndexChanged() {
            const at = LauncherLib.rowOf(root.rowList, Launcher.index);
            if (at >= 0) {
                view.positionViewAtIndex(at, ListView.Contain);
            }
        }
    }

    ListView {
        id: view

        anchors.fill: parent
        clip: true
        model: root.rowList
        boundsBehavior: Flickable.StopAtBounds

        delegate: Item {
            id: row

            required property var modelData

            width: view.width
            height: loader.item ? loader.item.implicitHeight : 0

            Loader {
                id: loader

                width: parent.width
                sourceComponent: ({
                        head: headRow,
                        pins: pinsRow,
                        file: fileRow,
                        app: appRow
                    })[row.modelData.type]
            }

            Component {
                id: headRow

                Item {
                    implicitHeight: label.implicitHeight + Theme.d(root.headTopPx + root.headBottomPx)

                    Text {
                        id: label

                        x: Theme.d(root.headSidePx)
                        y: Theme.d(root.headTopPx)
                        text: row.modelData.label + (row.modelData.count !== null ? " " + row.modelData.count : "")
                        color: Theme.dim
                        font {
                            family: Theme.font
                            pixelSize: Theme.d(root.headTextPx)
                            weight: Font.DemiBold
                            letterSpacing: Theme.d(root.headTextPx) * root.headTracking
                        }
                    }

                    Text {
                        id: tag

                        anchors {
                            left: label.right
                            leftMargin: Theme.d(root.headGapPx)
                            baseline: label.baseline
                        }
                        text: Theme.tag(row.modelData.tag)
                        color: Theme.signal
                        opacity: root.tagOpacity
                        font {
                            family: Theme.fontCjk
                            pixelSize: Theme.d(root.headTextPx)
                            weight: Font.Medium
                            letterSpacing: Theme.d(root.headTextPx) * root.tagTracking
                        }
                    }

                    Rectangle {
                        anchors {
                            left: tag.right
                            leftMargin: Theme.d(root.headGapPx)
                            right: parent.right
                            verticalCenter: label.verticalCenter
                        }
                        height: Math.max(1, Math.round(Theme.d(1)))
                        color: Theme.hair2
                    }
                }
            }

            Component {
                id: pinsRow

                Item {
                    readonly property real cellWidth: (width - Theme.d(2 * root.pinsSidePx + (root.pinColumns - 1) * root.pinGapPx)) / root.pinColumns
                    readonly property real cellHeight: Theme.d(root.pinPadTopPx + root.pinIconPx + root.pinNameGapPx + root.pinPadBottomPx + 2) + nameProbe.implicitHeight

                    implicitHeight: Theme.d(root.pinsTopPx + root.pinsBottomPx) + Math.ceil(row.modelData.count / root.pinColumns) * cellHeight + Math.max(0, Math.ceil(row.modelData.count / root.pinColumns) - 1) * Theme.d(root.pinGapPx)

                    Text {
                        id: nameProbe

                        visible: false
                        text: "M"
                        font.pixelSize: Theme.d(root.pinNamePx)
                    }

                    Repeater {
                        model: row.modelData.count

                        Item {
                            id: pin

                            required property int index
                            readonly property int at: row.modelData.from + index
                            readonly property bool selected: Launcher.index === at && Launcher.powerFocus < 0
                            readonly property var app: Launcher.items[at].entry

                            x: Theme.d(root.pinsSidePx) + (index % root.pinColumns) * (parent.cellWidth + Theme.d(root.pinGapPx))
                            y: Theme.d(root.pinsTopPx) + Math.floor(index / root.pinColumns) * (parent.cellHeight + Theme.d(root.pinGapPx))
                            width: parent.cellWidth
                            height: parent.cellHeight

                            Rectangle {
                                anchors.fill: parent
                                color: pin.selected ? Qt.alpha(Theme.signal, root.pinFillAlpha) : "transparent"
                                border {
                                    width: Theme.d(1)
                                    color: pin.selected ? Theme.hair : "transparent"
                                }
                            }

                            AppIcon {
                                x: (parent.width - width) / 2
                                y: Theme.d(root.pinPadTopPx + 1)
                                name: pin.app.icon
                                size: Theme.d(root.pinIconPx)
                                focused: pin.selected
                            }

                            Text {
                                x: Theme.d(root.pinPadSidePx + 1)
                                y: Theme.d(root.pinPadTopPx + root.pinIconPx + root.pinNameGapPx + 1)
                                width: parent.width - Theme.d(2 * (root.pinPadSidePx + 1))
                                horizontalAlignment: Text.AlignHCenter
                                text: pin.app.name
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                color: pin.selected ? Theme.hi : (Launcher.burning === pin.at ? Theme.burn : Theme.dim)
                                font {
                                    family: Theme.font
                                    pixelSize: Theme.d(root.pinNamePx)
                                    letterSpacing: Theme.d(root.pinNamePx) * root.pinNameTracking
                                }
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: Launcher.launch(pin.at)
                            }
                        }
                    }
                }
            }

            Component {
                id: fileRow

                Item {
                    readonly property bool selected: Launcher.index === row.modelData.at && Launcher.powerFocus < 0
                    readonly property var file: Launcher.items[row.modelData.at].file

                    implicitHeight: Theme.d(root.filePx)

                    Rectangle {
                        anchors.fill: parent
                        color: parent.selected ? Qt.alpha(Theme.signal, root.rowFillAlpha) : "transparent"
                    }

                    Glyph {
                        id: fileGlyph

                        x: Theme.d(root.filePadPx)
                        anchors.verticalCenter: parent.verticalCenter
                        name: "file"
                        size: Theme.d(root.fileIconPx)
                        color: Theme.signal
                    }

                    Text {
                        id: age

                        anchors {
                            right: parent.right
                            rightMargin: Theme.d(root.filePadPx)
                            verticalCenter: parent.verticalCenter
                        }
                        text: Fmt.ago(Date.now() * root.secondsPerMs - parent.file.modified_s)
                        color: Theme.dim
                        font {
                            family: Theme.font
                            pixelSize: Theme.d(root.fileMetaPx)
                        }
                    }

                    Item {
                        anchors {
                            left: fileGlyph.right
                            leftMargin: Theme.d(root.appGapPx)
                            right: age.left
                            rightMargin: Theme.d(root.appGapPx)
                            verticalCenter: parent.verticalCenter
                        }
                        height: fileName.implicitHeight
                        clip: true

                        Text {
                            id: fileName

                            text: parent.parent.file.name
                            textFormat: Text.PlainText
                            color: Launcher.burning === row.modelData.at ? Theme.burn : Theme.text
                            font {
                                family: Theme.font
                                pixelSize: Theme.d(root.fileNamePx)
                            }
                        }

                        Text {
                            anchors {
                                left: fileName.right
                                leftMargin: Theme.d(root.genericGapPx)
                                baseline: fileName.baseline
                            }
                            text: parent.parent.file.dir
                            textFormat: Text.PlainText
                            color: Theme.dim
                            font {
                                family: Theme.font
                                pixelSize: Theme.d(root.fileMetaPx)
                            }
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: Launcher.launch(row.modelData.at)
                    }
                }
            }

            Component {
                id: appRow

                Item {
                    readonly property bool selected: Launcher.index === row.modelData.at && Launcher.powerFocus < 0
                    readonly property var app: Launcher.items[row.modelData.at].entry
                    readonly property var parts: LauncherLib.highlight(app.name, Launcher.query)
                    readonly property color nameColor: Launcher.burning === row.modelData.at ? Theme.burn : (selected ? Theme.hi : Theme.text)
                    readonly property font nameFont: Qt.font({
                        family: Theme.font,
                        pixelSize: Theme.d(root.appNamePx)
                    })

                    implicitHeight: Theme.d(root.appRowPx)

                    Rectangle {
                        anchors.fill: parent
                        color: parent.selected ? Qt.alpha(Theme.signal, root.rowFillAlpha) : "transparent"
                    }

                    Rectangle {
                        width: Theme.d(root.edgePx)
                        height: parent.height
                        color: Theme.signal
                        visible: parent.selected
                    }

                    AppIcon {
                        id: appIcon

                        x: Theme.d(root.appPadLeftPx)
                        anchors.verticalCenter: parent.verticalCenter
                        name: parent.app.icon
                        size: Theme.d(root.appIconPx)
                        focused: parent.selected
                    }

                    Rectangle {
                        id: chip

                        anchors {
                            right: parent.right
                            rightMargin: Theme.d(root.appPadRightPx)
                            verticalCenter: parent.verticalCenter
                        }
                        width: chipText.implicitWidth + Theme.d(2 * (root.chipSidePx + 1))
                        height: Theme.d(root.chipLinePx + 2)
                        color: "transparent"
                        border {
                            width: Theme.d(1)
                            color: parent.selected ? Qt.alpha(Theme.signal, root.chipBorderAlpha) : Theme.hair
                        }

                        Text {
                            id: chipText

                            anchors.centerIn: parent
                            text: LauncherLib.category(parent.parent.app.categories)
                            color: parent.parent.selected ? Theme.signal : Theme.dim
                            font: root.chipFont()
                        }
                    }

                    Item {
                        anchors {
                            left: appIcon.right
                            leftMargin: Theme.d(root.appGapPx)
                            right: chip.left
                            rightMargin: Theme.d(root.appGapPx)
                            verticalCenter: parent.verticalCenter
                        }
                        height: nameBefore.implicitHeight
                        clip: true

                        Row {
                            id: nameRow

                            Text {
                                id: nameBefore

                                text: parent.parent.parent.parts[0]
                                textFormat: Text.PlainText
                                color: parent.parent.parent.nameColor
                                font: parent.parent.parent.nameFont
                            }

                            Text {
                                text: parent.parent.parent.parts[1]
                                textFormat: Text.PlainText
                                color: Launcher.burning === row.modelData.at ? Theme.burn : Theme.hi
                                font: parent.parent.parent.nameFont
                            }

                            Text {
                                text: parent.parent.parent.parts[2]
                                textFormat: Text.PlainText
                                color: parent.parent.parent.nameColor
                                font: parent.parent.parent.nameFont
                            }
                        }

                        Text {
                            anchors {
                                left: nameRow.right
                                leftMargin: Theme.d(root.genericGapPx)
                                verticalCenter: nameRow.verticalCenter
                            }
                            text: parent.parent.app.genericName
                            textFormat: Text.PlainText
                            color: Theme.dim
                            font {
                                family: Theme.font
                                pixelSize: Theme.d(root.genericPx)
                            }
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: Launcher.launch(row.modelData.at)
                    }
                }
            }
        }
    }

    Text {
        visible: root.noMatch
        x: Theme.d(root.emptyPadPx)
        y: Theme.d(root.emptyPadPx)
        width: parent.width - Theme.d(2 * root.emptyPadPx)
        wrapMode: Text.Wrap
        text: "NO MATCH · ENTER RUNS \"" + Launcher.query.toUpperCase() + "\" IN KRUNNER"
        textFormat: Text.PlainText
        color: Theme.dim
        font {
            family: Theme.font
            pixelSize: Theme.d(root.emptyPx)
            letterSpacing: Theme.d(root.emptyPx) * root.emptyTracking
        }
    }
}
