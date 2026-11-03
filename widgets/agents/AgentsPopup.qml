import QtQuick
import GhostKit

// What the AI chip opens: each provider's 5-hour and weekly usage, then one row per session, idle ones collapsed.
Rectangle {
    id: root

    // Design pixels, from the reference mockup's .pop.wide.
    readonly property real widthPx: 640
    readonly property real padXPx: 12
    readonly property real padYPx: 10
    readonly property real fillAlpha: 0.98

    property var usage
    property var sessions

    implicitWidth: Theme.d(widthPx)
    implicitHeight: detail.implicitHeight + Theme.d(2 * padYPx)
    color: Qt.alpha(Theme.ground, fillAlpha)
    border {
        width: Theme.d(1)
        color: Theme.hair
    }

    AgentsDetail {
        id: detail

        x: Theme.d(root.padXPx)
        y: Theme.d(root.padYPx)
        width: parent.width - Theme.d(2 * root.padXPx)
        usage: root.usage
        sessions: root.sessions
    }
}
