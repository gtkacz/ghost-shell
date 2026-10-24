import QtQuick
import GhostKit
import "agents.mjs" as Agents

// The AI chip in the bar: each provider's weekly meter, the session count and how many sessions wait, with the
// usage and sessions popup under it.
Item {
    id: root

    anchors.fill: parent

    // Design pixels, from the reference mockup's .chip and .st.
    readonly property real chipGapPx: 8
    readonly property real chipPadXPx: 8
    readonly property real chipPadYPx: 3
    readonly property real labelGapPx: 5
    readonly property real textPx: 10
    readonly property real meterCells: 10
    readonly property real popupGapPx: 4
    // Letter spacing in em.
    readonly property real labelTracking: 0.12
    readonly property real percent: 100

    property var settings
    property bool popupOpen: false

    readonly property var usage: Data.topic("agents.usage").value
    readonly property var sessions: Data.topic("agents.sessions").value
    readonly property var providers: usage ? usage.providers : []
    readonly property int sessionCount: sessions ? sessions.sessions.length : 0
    readonly property int waiting: sessions ? Agents.waitingCount(sessions.sessions) : 0

    implicitWidth: chip.width

    function levelColor(fraction) {
        const level = Fmt.level(fraction, Theme.usageWarn, Theme.usageCrit);
        return level === "crit" ? Theme.crit : level === "warn" ? Theme.warn : Theme.text;
    }

    component Label: Text {
        color: Theme.dim
        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
            letterSpacing: Theme.d(root.textPx) * root.labelTracking
        }
    }

    component Value: Text {
        color: Theme.text
        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
        }
    }

    // Always drawn, transparent at rest, so the chip's width does not jump when a session starts waiting.
    Rectangle {
        id: chip

        anchors.verticalCenter: parent.verticalCenter
        width: row.implicitWidth + Theme.d(2 * root.chipPadXPx)
        height: row.implicitHeight + Theme.d(2 * root.chipPadYPx)
        color: "transparent"
        border {
            width: Theme.d(1)
            color: root.waiting > 0 ? Theme.warn : "transparent"
        }

        Row {
            id: row

            anchors.centerIn: parent
            spacing: Theme.d(root.chipGapPx)

            Label {
                text: "AI"
                color: Theme.hi
            }

            Repeater {
                model: root.providers

                Row {
                    id: provider

                    required property var modelData

                    readonly property var weekly: Agents.weeklyOf(modelData)
                    readonly property real used: weekly && modelData.available ? weekly.used_pct : NaN

                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Theme.d(root.labelGapPx)

                    Label {
                        text: Agents.providerTag(provider.modelData.id)
                    }

                    SegMeter {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Theme.d(root.meterCells * cellPx + (root.meterCells - 1) * gapPx)
                        value: provider.used / root.percent
                        warnAt: Theme.usageWarn
                        critAt: Theme.usageCrit
                        sourceLive: provider.modelData.available
                    }

                    Value {
                        text: Fmt.pct(provider.used)
                        color: root.levelColor(provider.used / root.percent)
                    }
                }
            }

            Label {
                text: "·"
            }

            Value {
                text: root.sessionCount
                color: Theme.hi
            }

            Value {
                visible: root.waiting > 0
                text: root.waiting + " WAIT"
                color: Theme.warn
            }
        }
    }

    ClickArea {
        anchors.fill: chip
        onClicked: root.popupOpen = !root.popupOpen
    }

    BarPopup {
        anchorItem: chip
        gap: Theme.d(root.popupGapPx)
        open: root.popupOpen
        catchOutside: true
        onDismissed: root.popupOpen = false
        content: Component {
            AgentsPopup {
                usage: root.usage
                sessions: root.sessions
            }
        }
    }
}
