import QtQuick
import GhostKit
import "agents.mjs" as Agents

// What the AI chip opens: each provider's 5-hour and weekly usage, then one row per session, idle ones collapsed.
Rectangle {
    id: root

    // Design pixels, from the reference mockup's .pop.wide, .usage and .row.
    readonly property real widthPx: 640
    readonly property real padXPx: 12
    readonly property real padYPx: 10
    readonly property real textPx: 10
    readonly property real smallPx: 9
    readonly property real rowGapPx: 4
    readonly property real columnGapPx: 18
    readonly property real cellGapPx: 10
    readonly property real tagWidthPx: 34
    readonly property real windowWidthPx: 32
    // Two usage columns (4 cells of 32 + track + 44 + 80 and 3 gaps of 10) plus the 18 gap and 2 x 12 padding must fit widthPx: 2 x 296 + 18 + 24 = 634 of 640.
    readonly property real trackWidthPx: 110
    readonly property real trackHeightPx: 6
    readonly property real percentWidthPx: 44
    readonly property real resetWidthPx: 80
    readonly property real stateWidthPx: 110
    readonly property real agoWidthPx: 70
    readonly property real dotPx: 5
    readonly property real sectionGapPx: 6
    readonly property real fillAlpha: 0.98
    readonly property real trackAlpha: 0.08
    readonly property real blinkedAlpha: 0.25
    // Letter spacing in em.
    readonly property real headTracking: 0.22
    readonly property real tracking: 0.08
    readonly property real percent: 100
    readonly property int blinkPeriod: 2

    property var usage
    property var sessions

    readonly property double nowS: FrameClock.epochSecond
    readonly property var providers: usage ? usage.providers : []
    readonly property var all: sessions ? sessions.sessions : []
    readonly property int waiting: Agents.waitingCount(all)
    readonly property var shown: Agents.collapseIdle(Agents.sortSessions(all))

    implicitWidth: Theme.d(widthPx)
    implicitHeight: column.implicitHeight + Theme.d(2 * padYPx)
    color: Qt.alpha(Theme.ground, fillAlpha)
    border {
        width: Theme.d(1)
        color: Theme.hair
    }

    function levelColor(fraction) {
        const level = Fmt.level(fraction, Theme.usageWarn, Theme.usageCrit);
        return level === "crit" ? Theme.crit : level === "warn" ? Theme.warn : Theme.signal;
    }

    function stateColor(state) {
        return state === "waiting" ? Theme.warn : state === "busy" ? Theme.text : Theme.dim;
    }

    component Cell: Text {
        color: Theme.text
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
        textFormat: Text.PlainText
        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
            letterSpacing: Theme.d(root.textPx) * root.tracking
        }
    }

    component Head: Cell {
        color: Theme.dim
        font {
            pixelSize: Theme.d(root.smallPx)
            letterSpacing: Theme.d(root.smallPx) * root.headTracking
        }
    }

    component UsageRow: Row {
        id: usageRow

        required property var window

        spacing: Theme.d(root.cellGapPx)

        Cell {
            width: Theme.d(root.windowWidthPx)
            text: Agents.windowLabel(usageRow.window.length_s)
            color: Theme.dim
        }

        Item {
            width: Theme.d(root.trackWidthPx)
            height: Theme.d(root.trackHeightPx)
            anchors.verticalCenter: parent.verticalCenter

            Rectangle {
                anchors.fill: parent
                color: Qt.alpha(Theme.signal, root.trackAlpha)
            }

            Rectangle {
                width: parent.width * Math.min(usageRow.window.used_pct / root.percent, 1)
                height: parent.height
                color: root.levelColor(usageRow.window.used_pct / root.percent)
            }
        }

        Cell {
            width: Theme.d(root.percentWidthPx)
            horizontalAlignment: Text.AlignRight
            text: Fmt.pct(usageRow.window.used_pct)
            color: Theme.hi
        }

        Cell {
            width: Theme.d(root.resetWidthPx)
            horizontalAlignment: Text.AlignRight
            text: "↻ " + Agents.resetText(usageRow.window.resets_at_s, root.nowS)
            color: Theme.dim
        }
    }

    Column {
        id: column

        x: Theme.d(root.padXPx)
        y: Theme.d(root.padYPx)
        width: parent.width - Theme.d(2 * root.padXPx)
        spacing: Theme.d(root.rowGapPx)

        Item {
            width: parent.width
            height: header.implicitHeight

            Head {
                id: header

                text: "AI · USAGE AND SESSIONS"
            }

            Head {
                anchors.right: parent.right
                text: root.all.length + " SESSIONS" + (root.waiting > 0 ? " · " + root.waiting + " WAITING" : "")
            }
        }

        Row {
            spacing: Theme.d(root.columnGapPx)

            Repeater {
                model: root.providers

                Column {
                    id: block

                    required property var modelData

                    spacing: Theme.d(root.rowGapPx)

                    Cell {
                        text: Agents.providerTag(block.modelData.id) + " · " + block.modelData.label + (block.modelData.plan ? "  " + block.modelData.plan.toUpperCase() : "")
                        color: Theme.hi
                        font.weight: Font.Bold
                    }

                    Repeater {
                        model: block.modelData.available ? block.modelData.windows : []

                        UsageRow {
                            required property var modelData

                            window: modelData
                        }
                    }

                    Cell {
                        visible: !block.modelData.available
                        text: "--  " + (block.modelData.reason === "tap" ? "run scripts/install-agents.sh" : "no reading yet")
                        color: Theme.dim
                    }
                }
            }
        }

        Item {
            width: 1
            height: Theme.d(root.sectionGapPx)
        }

        Head {
            text: "SESSIONS"
        }

        Repeater {
            model: root.shown.rows

            Row {
                id: line

                required property var modelData

                spacing: Theme.d(root.cellGapPx)

                Cell {
                    width: Theme.d(root.tagWidthPx)
                    text: Agents.providerTag(line.modelData.provider)
                    color: Theme.dim
                    font.weight: Font.Bold
                }

                Row {
                    width: column.width - Theme.d(root.tagWidthPx + root.stateWidthPx + root.agoWidthPx + 3 * root.cellGapPx)
                    clip: true
                    spacing: Theme.d(root.cellGapPx)

                    Cell {
                        text: line.modelData.name
                        color: Theme.hi
                    }

                    Cell {
                        text: Agents.projectOf(line.modelData.cwd)
                        color: Theme.dim
                        font.pixelSize: Theme.d(root.smallPx)
                    }
                }

                Row {
                    width: Theme.d(root.stateWidthPx)
                    spacing: Theme.d(root.cellGapPx - root.dotPx)

                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Theme.d(root.dotPx)
                        height: width
                        color: root.stateColor(line.modelData.state)
                        opacity: line.modelData.state === "waiting" && FrameClock.epochSecond % root.blinkPeriod !== 0 ? root.blinkedAlpha : 1
                    }

                    Cell {
                        width: Theme.d(root.stateWidthPx - root.cellGapPx)
                        text: Agents.stateLabel(line.modelData)
                        color: root.stateColor(line.modelData.state)
                    }
                }

                Cell {
                    width: Theme.d(root.agoWidthPx)
                    horizontalAlignment: Text.AlignRight
                    text: Fmt.ago(root.nowS - line.modelData.since_s)
                    color: Theme.dim
                }
            }
        }

        Cell {
            visible: root.shown.hiddenIdle > 0
            text: root.shown.hiddenIdle + " more idle"
            color: Theme.dim
        }
    }
}
