import QtQuick
import GhostKit

// LLM usage and sessions: the chip in the bar or, with style "panel", a dashboard panel such as a tab.
Panel {
    id: root

    readonly property string panelStyle: "panel"

    property var settings

    readonly property bool inBar: settings.style !== panelStyle
    readonly property var usage: Data.topic("agents.usage").value
    readonly property var sessions: Data.topic("agents.sessions").value

    anchors.fill: parent
    header: !inBar
    framed: !inBar
    padding: inBar ? 0 : Theme.d(paddingPx)
    label: "AI"
    tagKey: "agents"
    implicitWidth: inBar ? styled.implicitWidth : 0

    Component {
        id: chip

        AgentsChip {
            usage: root.usage
            sessions: root.sessions
        }
    }

    Component {
        id: panel

        AgentsDetail {
            compact: true
            usage: root.usage
            sessions: root.sessions
        }
    }

    Loader {
        id: styled

        anchors.fill: parent
        sourceComponent: root.inBar ? chip : panel
    }
}
