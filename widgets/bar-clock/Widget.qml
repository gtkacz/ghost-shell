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
    readonly property real millisPerMinute: 60000
    readonly property string alwaysSeconds: "always"
    readonly property string calendarApplet: "org.kde.plasma.digitalclock"

    property var settings

    // Each bar repaint costs a fixed few milliseconds, so by default the seconds tick only while the dashboard,
    // whose own clock shows them, is covered.
    readonly property bool showsSeconds: Data.covered || (Data.config?.bar?.clock?.seconds ?? "covered") === alwaysSeconds
    // The time as of the last minute boundary, for as long as no seconds hold keeps the frame clock's epoch moving.
    property double minuteMs: Date.now()

    readonly property var now: new Date(showsSeconds ? FrameClock.epochSecond * millisPerSecond : minuteMs)
    readonly property var parts: Fmt.clockParts(now)

    implicitWidth: hm.implicitWidth + seconds.implicitWidth + seconds.anchors.leftMargin + Theme.d(dateGapPx) + date.implicitWidth

    property bool holdsSeconds: false

    // Held while the bar shows them. A flag rather than paired calls, since visibleChanged fires once before
    // completion, with nothing yet to release.
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
        if (FrameClock.logsHolds) {
            console.info("bar-clock seconds hold " + wanted);
        }
    }

    function syncHold() {
        holdSeconds(visible && showsSeconds);
    }

    Component.onCompleted: syncHold()
    Component.onDestruction: holdSeconds(false)
    onVisibleChanged: syncHold()
    onShowsSecondsChanged: {
        minuteMs = Date.now();
        syncHold();
    }

    // Polled each second rather than timed to the boundary: Qt timers stop with the monotonic clock during sleep, so a
    // boundary timer would show the pre-sleep minute for up to a minute after a wake. An unchanged minute emits nothing,
    // so nothing repaints between boundaries.
    Timer {
        interval: root.millisPerSecond
        running: root.visible && !root.showsSeconds
        repeat: true
        triggeredOnStart: true
        onTriggered: root.minuteMs = Math.floor(Date.now() / root.millisPerMinute) * root.millisPerMinute
    }

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
            leftMargin: root.showsSeconds ? Theme.d(root.gapPx) : 0
            baseline: hm.baseline
        }
        text: root.showsSeconds ? root.parts.seconds : ""
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
