import QtQuick
import QtQuick.Shapes
import GhostKit

// The tab style: one tab per window, most recently used first, and "+N" for the windows that do not fit.
Item {
    id: root

    // Design pixels, from the reference mockup's .tab.
    readonly property real tabWidthPx: 128
    readonly property real moreWidthPx: 32
    readonly property real tabHeightPx: 22
    readonly property real padLeftPx: 6
    readonly property real padRightPx: 14
    readonly property real chamferPx: 8
    readonly property real indexPx: 9
    readonly property real iconPx: 12
    readonly property real namePx: 9.5
    readonly property real gapPx: 6
    readonly property real morePadXPx: 8
    readonly property real popupGapPx: 1
    // Letter spacing in em.
    readonly property real nameTracking: 0.08

    required property var host

    readonly property var split: Tasks.tabs(host.windows, width, Theme.d(tabWidthPx), Theme.d(moreWidthPx))
    readonly property var taskOfWindow: {
        const byWindow = ({});
        for (const task of host.tasks) {
            for (const window of task.windows) {
                byWindow[window.id] = task;
            }
        }
        return byWindow;
    }
    readonly property font nameFont: Qt.font({
        family: Theme.font,
        pixelSize: Theme.d(namePx),
        weight: Font.DemiBold,
        letterSpacing: Theme.d(namePx) * nameTracking
    })
    property bool moreOpen: false

    // Asks for every window, so the slot is as wide as its tabs when it is not given a share of the bar.
    implicitWidth: Theme.d(tabWidthPx) * host.windows.length

    Row {
        height: parent.height

        Repeater {
            model: root.split.shown

            Item {
                id: tab

                required property var modelData
                required property int index

                readonly property var task: root.taskOfWindow[modelData.id]
                readonly property bool selected: modelData.active
                readonly property real chamfer: Theme.d(root.chamferPx)

                width: Math.min(Theme.d(root.tabWidthPx), Theme.d(root.padLeftPx + root.padRightPx) + natural.width)
                height: Theme.d(root.tabHeightPx)
                anchors.verticalCenter: parent.verticalCenter

                Shape {
                    anchors.fill: parent
                    preferredRendererType: Shape.CurveRenderer

                    ShapePath {
                        fillColor: tab.selected ? Theme.signal : (area.containsMouse ? Qt.alpha(Theme.signal, 0.08) : "transparent")
                        strokeColor: "transparent"
                        startX: 0
                        startY: 0

                        PathLine {
                            x: tab.width - tab.chamfer
                            y: 0
                        }
                        PathLine {
                            x: tab.width
                            y: tab.chamfer
                        }
                        PathLine {
                            x: tab.width
                            y: tab.height
                        }
                        PathLine {
                            x: 0
                            y: tab.height
                        }
                        PathLine {
                            x: 0
                            y: 0
                        }
                    }
                }

                // Sizes the tab to its content; the name is elided when that is past the tab's largest width.
                Row {
                    id: natural

                    readonly property real room: Theme.d(root.tabWidthPx - root.padLeftPx - root.padRightPx) - number.implicitWidth - icon.width - 2 * spacing

                    x: Theme.d(root.padLeftPx)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Theme.d(root.gapPx)

                    Text {
                        id: number

                        anchors.verticalCenter: parent.verticalCenter
                        text: Tasks.pad2(tab.index + 1)
                        color: tab.selected ? Theme.ground : Theme.dim
                        font {
                            family: Theme.font
                            pixelSize: Theme.d(root.indexPx)
                        }
                    }

                    AppIcon {
                        id: icon

                        anchors.verticalCenter: parent.verticalCenter
                        name: tab.task ? root.host.iconOf(tab.task) : ""
                        monoColor: tab.selected ? Theme.ground : Theme.signal
                        size: Theme.d(root.iconPx)
                    }

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, natural.room)
                        text: tab.task ? root.host.nameOf(tab.task).toUpperCase() : ""
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: tab.selected ? Theme.ground : Theme.text
                        font: root.nameFont
                    }
                }

                MouseArea {
                    id: area

                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.host.perform(tab.task, Tasks.tabClick(tab.modelData))
                }
            }
        }

        Item {
            id: more

            visible: root.split.hidden > 0
            width: moreText.implicitWidth + Theme.d(2 * root.morePadXPx)
            height: parent.height

            Text {
                id: moreText

                anchors.centerIn: parent
                text: "+" + root.split.hidden
                color: Theme.dim
                font: root.nameFont
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.moreOpen = true
            }

            BarPopup {
                anchorItem: more
                gap: Theme.d(root.popupGapPx)
                open: root.moreOpen
                catchOutside: true
                onDismissed: root.moreOpen = false
                content: Component {
                    TitleList {
                        appName: "Windows"
                        windows: root.host.windows.slice(root.split.shown.length)
                        onPicked: id => {
                            root.moreOpen = false;
                            root.host.activate(id);
                        }
                    }
                }
            }
        }
    }
}
