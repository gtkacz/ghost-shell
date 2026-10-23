import QtQuick
import GhostKit

// The time with its seconds and the date, whose click opens KDE's calendar.
Item {
    id: root

    anchors.fill: parent

    // Design pixels, from the reference mockup's .bclock.
    readonly property real timePx: 15
    readonly property real secondsPx: 9
    readonly property real datePx: 10
    readonly property real gapPx: 3
    readonly property real dateGapPx: 8
    readonly property real millisPerSecond: 1000
    readonly property string calendarApplet: "org.kde.plasma.digitalclock"

    property var settings

    readonly property var now: new Date(FrameClock.epochSecond * millisPerSecond)
    readonly property var parts: Fmt.clockParts(now)

    implicitWidth: hm.implicitWidth + Theme.d(gapPx) + seconds.implicitWidth + Theme.d(dateGapPx) + date.implicitWidth

    property bool holdsSeconds: false

    // Held while the bar shows, as the seconds are the only thing that moves while the dashboard is covered. A flag
    // rather than paired calls, since visibleChanged fires once before completion, with nothing yet to release.
    function holdSeconds(wanted) {
        if (wanted === holdsSeconds) {
            return;
        }
        if (wanted) {
            FrameClock.acquireSeconds();
        } else {
            FrameClock.releaseSeconds();
        }
        holdsSeconds = wanted;
    }

    Component.onCompleted: holdSeconds(visible)
    Component.onDestruction: holdSeconds(false)
    onVisibleChanged: holdSeconds(visible)

    Text {
        id: hm

        anchors.verticalCenter: parent.verticalCenter
        text: root.parts.hours + ":" + root.parts.minutes
        color: Theme.hi
        font {
            family: Theme.font
            pixelSize: Theme.d(root.timePx)
            weight: Font.Bold
        }
    }

    Text {
        id: seconds

        anchors {
            left: hm.right
            leftMargin: Theme.d(root.gapPx)
            baseline: hm.baseline
        }
        text: root.parts.seconds
        color: Theme.signal
        font {
            family: Theme.font
            pixelSize: Theme.d(root.secondsPx)
        }
    }

    Text {
        id: date

        anchors {
            left: seconds.right
            leftMargin: Theme.d(root.dateGapPx)
            baseline: hm.baseline
        }
        text: Fmt.ddmmyy(root.now)
        color: Theme.dim
        font {
            family: Theme.font
            pixelSize: Theme.d(root.datePx)
        }
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: Data.call("plasma.popup", {
            applet: root.calendarApplet
        })
    }
}
