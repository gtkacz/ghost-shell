import QtQuick
import GhostKit

// The dashboard's notifications: the newest records as rows, with the counts under them. The desktop layer cannot
// give a field the keyboard, so the search row only opens the centre, whose field takes it. Looking at the panel for
// long enough marks everything read.
Panel {
    id: root

    // Design pixels, from the reference mockup's .nsearch, .notifs and .nfoot.
    readonly property real bodyGapPx: 8
    readonly property real searchGapPx: 8
    readonly property real searchPadYPx: 2
    readonly property real searchPadXPx: 8
    readonly property real searchTextPx: 10
    readonly property real magnifierPx: 11
    readonly property real keyTextPx: 9
    readonly property real keyPadXPx: 5
    readonly property real footTextPx: 8.5
    readonly property real scrollGlyphPx: 9
    readonly property real scrollGapPx: 4
    // Percentages, as Qt.alpha() takes them.
    readonly property real searchFill: 0.4
    // Letter spacing in em.
    readonly property real searchTracking: 0.04
    readonly property real footTracking: 0.14
    readonly property real emptyTracking: 0.2

    property var settings

    readonly property bool serving: Notifications.serving
    readonly property var items: Notifications.snapshot?.items ?? []
    readonly property bool locked: Data.topic("session").value?.locked ?? false
    // Data.covered also holds while the greeting hides the dashboard, so the greeting needs no input of its own.
    readonly property bool lookedAt: visible && !Data.covered && !locked
    readonly property string emptyText: !serving ? (Notifications.server === "plasma" ? "PLASMA IS SERVING NOTIFICATIONS" : "NO SERVER") : "NO SIGNALS"

    anchors.fill: parent
    label: "Notifications"
    tagKey: "notif"
    meta: Notifications.unread + " NEW"

    // Restarted whenever it becomes true, so every look at the panel gets its whole dwell.
    onLookedAtChanged: Notifications.widgetVisible = lookedAt
    Component.onCompleted: Notifications.widgetVisible = lookedAt
    Component.onDestruction: Notifications.widgetVisible = false

    Timer {
        interval: Notifications.widgetReadDwellMs
        running: root.lookedAt
        onTriggered: {
            const due = Notifications.widgetReadDue({
                visibleMs: interval,
                covered: Data.covered,
                greetingPlaying: false,
                locked: root.locked
            });
            if (due && Notifications.unread > 0) {
                Notifications.markAllRead();
            }
        }
    }

    Rectangle {
        id: search

        width: parent.width
        height: queryText.implicitHeight + Theme.d(2 * root.searchPadYPx + 2)
        color: Qt.alpha(Theme.ground, root.searchFill)
        border {
            width: Theme.d(1)
            color: Theme.hair
        }

        Glyph {
            id: magnifier

            x: Theme.d(root.searchPadXPx + 1)
            anchors.verticalCenter: parent.verticalCenter
            name: "search"
            size: Theme.d(root.magnifierPx)
            color: Theme.dim
        }

        Text {
            id: queryText

            anchors {
                left: magnifier.right
                leftMargin: Theme.d(root.searchGapPx)
                verticalCenter: parent.verticalCenter
            }
            text: "FILTER NOTIFICATIONS"
            textFormat: Text.PlainText
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(root.searchTextPx)
                weight: Font.Medium
                letterSpacing: Theme.d(root.searchTextPx) * root.searchTracking
            }
        }

        Rectangle {
            anchors {
                right: parent.right
                rightMargin: Theme.d(root.searchPadXPx)
                verticalCenter: parent.verticalCenter
            }
            width: slash.implicitWidth + Theme.d(2 * root.keyPadXPx + 2)
            height: slash.implicitHeight + Theme.d(2)
            color: "transparent"
            border {
                width: Theme.d(1)
                color: Theme.hair
            }

            Text {
                id: slash

                anchors.centerIn: parent
                text: "/"
                color: Theme.dim
                font {
                    family: Theme.font
                    pixelSize: Theme.d(root.keyTextPx)
                    weight: Font.Medium
                }
            }
        }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: Notifications.show()
        }
    }

    Item {
        anchors {
            top: search.bottom
            topMargin: Theme.d(root.bodyGapPx)
            bottom: foot.top
            bottomMargin: Theme.d(root.bodyGapPx)
            left: parent.left
            right: parent.right
        }

        NotificationRows {
            anchors.fill: parent
            items: root.serving ? root.items : []
            showDismiss: false
            visible: root.serving && root.items.length > 0
            onActivated: id => Notifications.invoke(id, "default")
        }

        Text {
            anchors.centerIn: parent
            visible: !root.serving || root.items.length === 0
            text: root.emptyText
            textFormat: Text.PlainText
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(root.searchTextPx)
                weight: Font.Medium
                letterSpacing: Theme.d(root.searchTextPx) * root.emptyTracking
            }
        }
    }

    Item {
        id: foot

        anchors {
            bottom: parent.bottom
            left: parent.left
            right: parent.right
        }
        height: counts.implicitHeight

        Text {
            id: counts

            visible: root.serving
            text: Notifications.countsLine(root.items)
            textFormat: Text.PlainText
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(root.footTextPx)
                weight: Font.Medium
                letterSpacing: Theme.d(root.footTextPx) * root.footTracking
            }
        }

        Glyph {
            id: scrollGlyph

            anchors {
                right: parent.right
                verticalCenter: parent.verticalCenter
            }
            name: "chevron-down"
            size: Theme.d(root.scrollGlyphPx)
            color: Theme.dim
        }

        Text {
            anchors {
                right: scrollGlyph.left
                rightMargin: Theme.d(root.scrollGapPx)
                verticalCenter: parent.verticalCenter
            }
            text: "SCROLL"
            color: Theme.dim
            font: counts.font
        }
    }
}
