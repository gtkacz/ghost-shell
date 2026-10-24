import QtQuick
import Quickshell
import "lib/notifications.mjs" as Rules

// The notification rows of the centre and of the dashboard's widget: app, one line of summary and body, age. The rows are
// keyed by record id, so a publish that changes none of them rebuilds none.
ListView {
    id: root

    // Design pixels, from the notifications mockup's .list li.
    readonly property real appColumnPx: 76
    readonly property real ageColumnPx: 28
    readonly property real columnGapPx: 8
    readonly property real padYPx: 4
    readonly property real padRightPx: 6
    readonly property real edgePx: 3
    readonly property real edgePadPx: 6
    readonly property real appTextPx: 9
    readonly property real msgTextPx: 10
    readonly property real crossPx: 7
    readonly property real crossRightPx: 9
    // Percentages, as withAlpha() takes them.
    readonly property real selectedFill: 0.45
    // Letter spacing in em.
    readonly property real appTracking: 0.1
    readonly property real textTracking: 0.08

    property var items: []
    property int selected: -1
    property bool showDismiss: true

    signal activated(int id)
    signal dismissed(int id)

    function toneOf(level) {
        return level === "crit" ? Theme.crit : level === "warn" ? Theme.warn : Theme.signal;
    }

    clip: true
    boundsBehavior: Flickable.StopAtBounds
    onSelectedChanged: {
        if (selected >= 0 && selected < count) {
            positionViewAtIndex(selected, ListView.Contain);
        }
    }

    model: ScriptModel {
        values: root.items
        objectProp: "id"
    }

    delegate: Item {
        id: row

        required property var modelData
        required property int index

        readonly property bool picked: index === root.selected
        readonly property bool leveled: modelData.level === "warn" || modelData.level === "crit"
        readonly property real leftPad: leveled ? Theme.d(root.edgePadPx) : 0
        readonly property color appTone: root.toneOf(modelData.level)
        readonly property color msgTone: leveled ? appTone : modelData.read ? Theme.text : Theme.hi
        // Reading the clock makes the age follow it whenever something else holds it.
        readonly property string age: {
            FrameClock.epochSecond;
            return Rules.rowText(modelData, Math.floor(Date.now() / 1000)).age;
        }

        width: ListView.view.width
        height: Theme.d(2 * root.padYPx) + msg.implicitHeight + ruleHeight
        readonly property real ruleHeight: Math.max(1, Math.round(Theme.d(1)))

        Rectangle {
            anchors.fill: parent
            color: Qt.alpha(Theme.hair, root.selectedFill)
            visible: row.picked
        }

        Rectangle {
            width: Theme.d(root.edgePx)
            height: parent.height - row.ruleHeight
            color: row.appTone
            visible: row.leveled
        }

        Text {
            x: row.leftPad
            width: Theme.d(root.appColumnPx)
            anchors.verticalCenter: msg.verticalCenter
            text: row.modelData.app.toUpperCase()
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: row.appTone
            font {
                family: Theme.font
                pixelSize: Theme.d(root.appTextPx)
                weight: Font.DemiBold
                letterSpacing: Theme.d(root.appTextPx) * root.appTracking
            }
        }

        Text {
            id: msg

            x: row.leftPad + Theme.d(root.appColumnPx + root.columnGapPx)
            y: Theme.d(root.padYPx)
            width: row.width - x - Theme.d(root.padRightPx + root.ageColumnPx + root.columnGapPx)
            text: Rules.rowMarkup(row.modelData, String(Theme.dim))
            textFormat: Text.StyledText
            elide: Text.ElideRight
            maximumLineCount: 1
            color: row.msgTone
            font {
                family: Theme.font
                pixelSize: Theme.d(root.msgTextPx)
                weight: Font.Medium
                letterSpacing: Theme.d(root.msgTextPx) * root.textTracking
            }
        }

        Text {
            anchors {
                right: parent.right
                rightMargin: Theme.d(root.padRightPx)
                verticalCenter: msg.verticalCenter
            }
            visible: !(row.picked && root.showDismiss)
            text: row.age
            textFormat: Text.PlainText
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(root.appTextPx)
                weight: Font.Medium
            }
        }

        Glyph {
            anchors {
                right: parent.right
                rightMargin: Theme.d(root.crossRightPx)
                verticalCenter: msg.verticalCenter
            }
            visible: row.picked && root.showDismiss
            name: "cross"
            size: Theme.d(root.crossPx)
            color: Theme.dim
        }

        Rectangle {
            y: parent.height - height
            width: parent.width
            height: row.ruleHeight
            color: Theme.hair2
        }

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton
            cursorShape: Qt.PointingHandCursor
            onClicked: mouse => {
                if (mouse.button === Qt.MiddleButton) {
                    root.dismissed(row.modelData.id);
                } else {
                    root.activated(row.modelData.id);
                }
            }
        }

        MouseArea {
            anchors {
                right: parent.right
                top: parent.top
                bottom: parent.bottom
            }
            width: Theme.d(root.crossRightPx + root.crossPx + root.columnGapPx)
            visible: row.picked && root.showDismiss
            cursorShape: Qt.PointingHandCursor
            onClicked: root.dismissed(row.modelData.id)
        }
    }
}
