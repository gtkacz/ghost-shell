import QtQuick
import GhostKit

// A line of system facts that scrolls through the bar's middle, as the reference mockup's .tk, on a timer of its own
// that runs only while the line is wider than its slot.
Item {
    id: root

    anchors.fill: parent

    // Design pixels, from the mockup's .tk.
    readonly property real textPx: 10
    readonly property real copyGapPx: 24
    // Letter spacing in em, as the bar's.
    readonly property real tracking: 0.08
    readonly property real millisPerLoop: 60000
    readonly property real millisPerSecond: 1000
    // The bar repaints on every step and each repaint costs a few milliseconds, so the line steps five times a second
    // rather than at the frame clock's 15 fps; the scroll speed is unchanged.
    readonly property int stepsPerSecondToSaveBarRepaints: 5
    readonly property string critOpen: "<font color=\"" + Theme.crit + "\">"
    readonly property string critClose: "</font>"

    property var settings

    readonly property var segments: Fmt.tickerSegments({
        ticker: Data.topic("ticker").value,
        host: Data.topic("host").value,
        links: Data.topic("net.links").value,
        disks: Data.topic("disks").value,
        alerts: Data.topic("weather.alerts").value,
        nowS: Math.floor(Date.now() / millisPerSecond)
    })
    // A provider's words reach this as rich text, so they are escaped.
    readonly property string markup: segments.map(segment => segment.level === "crit" ? critOpen + escaped(segment.text) + critClose : escaped(segment.text)).join(" // ") + " //"

    readonly property real copyWidth: probe.implicitWidth + Theme.d(copyGapPx)
    readonly property bool overflows: visible && probe.implicitWidth > width
    // Whole device pixels, so a step moves the line instead of blurring it.
    readonly property real pixelRatio: Screen.devicePixelRatio
    readonly property real offset: overflows ? Math.round(stepMs % millisPerLoop / millisPerLoop * copyWidth * pixelRatio) / pixelRatio : 0

    // Read from the wall clock at each step, so a late tick never drifts the line.
    property double stepMs: Date.now()

    function escaped(text) {
        return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    }

    Timer {
        interval: root.millisPerSecond / root.stepsPerSecondToSaveBarRepaints
        repeat: true
        running: root.overflows
        onRunningChanged: {
            if (running) {
                root.stepMs = Date.now();
            }
            if (FrameClock.logsHolds) {
                console.info("ticker timer running " + running);
            }
        }
        onTriggered: root.stepMs = Date.now()
    }

    component Line: Text {
        textFormat: Text.RichText
        text: root.markup
        color: Theme.dim
        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
            letterSpacing: Theme.d(root.textPx) * root.tracking
        }
    }

    Line {
        id: probe

        visible: false
    }

    Item {
        anchors.fill: parent
        clip: true

        Row {
            x: -root.offset
            height: parent.height
            spacing: Theme.d(root.copyGapPx)

            Line {
                anchors.verticalCenter: parent.verticalCenter
            }

            Line {
                visible: root.overflows
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
}
