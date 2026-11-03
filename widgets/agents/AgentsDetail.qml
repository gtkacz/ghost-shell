import QtQuick
import GhostKit
import "agents.mjs" as Agents

// Each provider's 5-hour and weekly usage, then one row per session. Wide in the chip's popup, with idle sessions
// collapsed; compact in a panel, where the usage loses its titles and the sessions fill what height is left.
Column {
    id: root

    // Design pixels, from the reference mockup's .pop.wide, .usage and .row.
    readonly property real textPx: 10
    readonly property real smallPx: 9
    readonly property real rowGapPx: compact ? 2 : 4
    readonly property real columnGapPx: 18
    readonly property real cellGapPx: compact ? 6 : 10
    readonly property real tagWidthPx: compact ? 20 : 34
    readonly property real windowWidthPx: compact ? 20 : 32
    // Two usage columns (4 cells of 32 + track + 44 + 80 and 3 gaps of 10) plus the 18 gap and 2 x 12 padding must fit the popup's widthPx: 2 x 296 + 18 + 24 = 634 of 640.
    readonly property real wideTrackWidthPx: 110
    readonly property real percentWidthPx: compact ? 34 : 44
    readonly property real resetWidthPx: compact ? 66 : 80
    readonly property real stateWidthPx: compact ? 60 : 110
    readonly property real agoWidthPx: compact ? 30 : 70
    readonly property real dotPx: 5
    readonly property real sectionGapPx: compact ? 3 : 6
    readonly property real blinkedAlpha: 0.25
    // Letter spacing in em.
    readonly property real headTracking: 0.22
    readonly property real tracking: 0.08
    readonly property real percent: 100
    readonly property int blinkPeriod: 2

    property var usage
    property var sessions
    property bool compact: false
    property bool holdsSeconds: false

    readonly property double nowS: FrameClock.epochSecond
    readonly property var providers: usage ? usage.providers : []
    readonly property var all: sessions ? sessions.sessions : []
    readonly property int waiting: Agents.waitingCount(all)
    readonly property string waitingCount: waiting > 0 ? " · " + waiting + " WAITING" : ""
    readonly property real trackWidth: compact
        ? Math.max(0, width - Theme.d(tagWidthPx + windowWidthPx + percentWidthPx + resetWidthPx + 4 * cellGapPx))
        : Theme.d(wideTrackWidthPx)
    // Only a panel has a height to fill; the popup grows with its rows.
    readonly property int rowsThatFit: Math.floor((height - sessionsHead.y - sessionsHead.height) / (rowMetrics.height + spacing))
    readonly property var sorted: Agents.sortSessions(all)
    readonly property var fitted: Agents.fitRows(sorted, rowsThatFit)
    readonly property var collapsed: Agents.collapseIdle(sorted)
    readonly property var rows: compact ? fitted.rows : collapsed.rows
    readonly property string moreText: compact
        ? (fitted.hidden > 0 ? fitted.hidden + " more" : "")
        : (collapsed.hiddenIdle > 0 ? collapsed.hiddenIdle + " more idle" : "")

    spacing: Theme.d(rowGapPx)
    clip: compact

    onVisibleChanged: holdSeconds(visible)
    Component.onCompleted: holdSeconds(visible)
    Component.onDestruction: holdSeconds(false)

    // The resets, the ages and the waiting blink read the second, which frame clock keeps current only for a holder.
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

    function stateColor(state) {
        return state === "waiting" ? Theme.warn : state === "busy" ? Theme.text : Theme.dim;
    }

    FontMetrics {
        id: rowMetrics

        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
        }
    }

    // Scrambles in a panel, as the dashboard's text does each time it shows; the popup opens plainly.
    component Cell: ScrambleText {
        // A scramble's frames can reach a taller fallback font, which must not move the rows that rowsThatFit counts.
        height: rowMetrics.height
        scrambles: root.compact
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
        property string tag

        spacing: Theme.d(root.cellGapPx)

        Cell {
            visible: root.compact
            width: Theme.d(root.tagWidthPx)
            text: usageRow.tag
            color: Theme.hi
            font.weight: Font.Bold
        }

        Cell {
            width: Theme.d(root.windowWidthPx)
            text: Agents.windowLabel(usageRow.window.length_s)
            color: Theme.dim
        }

        SegMeter {
            anchors.verticalCenter: parent.verticalCenter
            width: root.trackWidth
            value: usageRow.window.used_pct / root.percent
            warnAt: Theme.usageWarn
            critAt: Theme.usageCrit
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
            text: (root.compact ? "" : "↻ ") + Agents.resetText(usageRow.window.resets_at_s, root.nowS)
            color: Theme.dim
        }
    }

    Item {
        visible: !root.compact
        width: parent.width
        height: rowMetrics.height

        Head {
            id: header

            text: "AI · USAGE AND SESSIONS"
        }

        Head {
            anchors.right: parent.right
            text: root.all.length + " SESSIONS" + root.waitingCount
        }
    }

    Grid {
        columns: root.compact ? 1 : 2
        columnSpacing: Theme.d(root.columnGapPx)
        rowSpacing: Theme.d(root.sectionGapPx)

        Repeater {
            model: root.providers

            Column {
                id: block

                required property var modelData

                spacing: Theme.d(root.rowGapPx)

                Cell {
                    visible: !root.compact
                    text: Agents.providerTag(block.modelData.id) + " · " + block.modelData.label + (block.modelData.plan ? "  " + block.modelData.plan.toUpperCase() : "")
                    color: Theme.hi
                    font.weight: Font.Bold
                }

                Repeater {
                    model: block.modelData.available ? block.modelData.windows : []

                    UsageRow {
                        required property var modelData
                        required property int index

                        window: modelData
                        tag: index === 0 ? Agents.providerTag(block.modelData.id) : ""
                    }
                }

                Cell {
                    visible: !block.modelData.available
                    text: (root.compact ? Agents.providerTag(block.modelData.id) + "  " : "") + "--  " + (block.modelData.reason === "tap" ? "run scripts/install-agents.sh" : "no reading yet")
                    color: Theme.dim
                }
            }
        }
    }

    Item {
        width: 1
        height: Theme.d(root.sectionGapPx)
    }

    Item {
        id: sessionsHead

        width: parent.width
        height: rowMetrics.height

        Head {
            id: sessionsLabel

            text: "SESSIONS"
        }

        Head {
            visible: root.compact
            anchors.right: parent.right
            text: root.all.length + root.waitingCount
        }
    }

    Repeater {
        model: root.rows

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
                width: root.width - Theme.d(root.tagWidthPx + root.stateWidthPx + root.agoWidthPx + 3 * root.cellGapPx)
                clip: true
                spacing: Theme.d(root.cellGapPx)

                Cell {
                    width: root.compact ? parent.width : implicitWidth
                    text: line.modelData.name
                    color: Theme.hi
                }

                Cell {
                    visible: !root.compact
                    text: Agents.projectOf(line.modelData.cwd)
                    color: Theme.dim
                    font.pixelSize: Theme.d(root.smallPx)
                }
            }

            Row {
                width: Theme.d(root.stateWidthPx)
                spacing: Theme.d(root.cellGapPx - (root.compact ? 0 : root.dotPx))

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.d(root.dotPx)
                    height: width
                    color: root.stateColor(line.modelData.state)
                    opacity: line.modelData.state === "waiting" && FrameClock.epochSecond % root.blinkPeriod !== 0 ? root.blinkedAlpha : 1
                }

                Cell {
                    width: parent.width - parent.spacing - Theme.d(root.dotPx)
                    // A panel row has no room for the reason, which the chip's popup gives.
                    text: root.compact ? line.modelData.state.toUpperCase() : Agents.stateLabel(line.modelData)
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
        visible: root.moreText !== ""
        text: root.moreText
        color: Theme.dim
    }
}
