import QtQuick
import GhostKit
import "network.mjs" as Net

// The default route's upload and download history as a mirror graph, over its rates, the ping and whether the link is
// up, in two styles: a panel with the wired, VPN and Tailscale links below (the mockup's p-net), and a headerless one
// with the VPN and tailnet in one line each (the lower half of the mockup's uplink scope).
Panel {
    id: root

    readonly property string panelStyle: "panel"
    readonly property string scopeStyle: "scope"

    // Design pixels, from the reference mockup's p-net, .kv2 and .links.
    readonly property real stackGapPx: 8
    readonly property real graphPx: 40
    readonly property real graphCellWPx: 3
    readonly property real kvPx: 10
    readonly property real kvGapPx: 8
    readonly property real kvKeyGapPx: 4
    readonly property real linkPx: 9.5
    readonly property real linkNamePx: 70
    readonly property real linkGapPx: 8
    readonly property real linkRowGapPx: 2
    // The mockup's chevrons are .8em of their line.
    readonly property real glyphEm: 0.8

    // History series, named as the daemon records them.
    readonly property string netUpKey: "net_up"
    readonly property string netDownKey: "net_down"

    property var settings

    readonly property string style: settings.style
    readonly property var net: Data.topic("net").value
    readonly property var links: Data.topic("net.links").value
    readonly property bool netLive: Data.connected && !Data.stale && Data.topic("net").live
    readonly property bool linksLive: Data.connected && !Data.stale && Data.topic("net.links").live
    readonly property var upBps: net ? net.tx_bps : null
    readonly property var downBps: net ? net.rx_bps : null
    readonly property var pingMs: links ? links.ping_ms : null

    // Missing and stale readings are dim, so "--" never reads as a value.
    function tone(known, live) {
        return live && known ? Theme.hi : Theme.dim;
    }

    // Spreads its children across its width with the first at the left edge and the last at the right, as the
    // mockup's .kv2 does, and shrinks them when even the smallest gaps would overflow.
    component SpreadRow: Item {
        id: spread

        default property alias pairs: line.data
        readonly property real minGap: Theme.d(root.kvGapPx)
        readonly property real itemsWidth: Array.from(line.children).reduce((sum, child) => sum + child.implicitWidth, 0)
        readonly property real gaps: Math.max(1, line.children.length - 1)

        implicitHeight: line.implicitHeight

        InlineRow {
            id: line

            transformOrigin: Item.Left
            spacing: Math.max(spread.minGap, (spread.width - spread.itemsWidth) / spread.gaps)
            scale: Math.min(1, spread.width / (spread.itemsWidth + spread.gaps * spread.minGap))
        }
    }

    // A dim key, as a word or a chevron, before its value.
    component KvPair: InlineRow {
        id: pair

        property string key
        property string glyph
        property string value
        property color valueColor

        spacing: Theme.d(root.kvKeyGapPx)

        Glyph {
            anchors.verticalCenter: parent.verticalCenter
            visible: pair.glyph !== ""
            // Glyph warns about a name it lacks even while hidden.
            name: pair.glyph !== "" ? pair.glyph : "chevron-up"
            size: Theme.d(root.kvPx * root.glyphEm)
            color: Theme.dim
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: pair.key !== ""
            text: pair.key
            color: Theme.dim
            font: kvLine.font
        }

        Digits {
            anchors.verticalCenter: parent.verticalCenter
            text: pair.value
            pixelSize: Theme.d(root.kvPx)
            weight: Font.Medium
            color: pair.valueColor
        }
    }

    // The rates, the ping and a chip while the default interface is up. The chip keeps its place while hidden, so the
    // readings do not move when the link drops.
    component RatesRow: SpreadRow {
        id: rates

        property string chipKey

        KvPair {
            anchors.verticalCenter: parent.verticalCenter
            glyph: "chevron-up"
            value: Fmt.rate(root.upBps)
            valueColor: root.tone(Number.isFinite(root.upBps), root.netLive)
        }

        KvPair {
            anchors.verticalCenter: parent.verticalCenter
            glyph: "chevron-down"
            value: Fmt.rate(root.downBps)
            valueColor: root.tone(Number.isFinite(root.downBps), root.netLive)
        }

        KvPair {
            anchors.verticalCenter: parent.verticalCenter
            key: "PING"
            value: Net.pingText(root.pingMs)
            valueColor: root.tone(Number.isFinite(root.pingMs), root.linksLive)
        }

        Item {
            anchors.verticalCenter: parent.verticalCenter
            implicitWidth: chip.implicitWidth
            implicitHeight: chip.implicitHeight

            Chip {
                id: chip

                key: rates.chipKey
                opacity: root.netLive && Net.defaultUp(root.net) ? 1 : 0
            }
        }
    }

    anchors.fill: parent
    header: style === panelStyle
    label: "Network"
    tagKey: "net"
    meta: Net.metaLine(net, links)

    FontMetrics {
        id: kvLine

        font {
            family: Theme.font
            pixelSize: Theme.d(root.kvPx)
            weight: Font.Medium
        }
    }

    FontMetrics {
        id: linkLine

        font {
            family: Theme.font
            pixelSize: Theme.d(root.linkPx)
            weight: Font.Medium
        }
    }

    Column {
        id: stack

        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
        }
        spacing: Theme.d(root.stackGapPx)

        MirrorGraph {
            width: parent.width
            height: Theme.d(root.graphPx)
            up: Data.history[root.netUpKey] || []
            down: Data.history[root.netDownKey] || []
            floor: Net.NET_FLOOR_BPS
            cellW: root.graphCellWPx
            sourceLive: Data.topic("net").live
        }

        RatesRow {
            width: parent.width
            chipKey: root.style === root.panelStyle ? "online" : "live"
        }

        Loader {
            width: parent.width
            active: root.style === root.scopeStyle
            sourceComponent: tunnels
        }
    }

    Component {
        id: tunnels

        Item {
            id: tunnelLine

            readonly property real gap: Theme.d(root.kvGapPx)

            implicitHeight: vpnKey.implicitHeight

            Text {
                id: vpnKey

                text: "VPN"
                color: Theme.dim
                font: kvLine.font
            }

            Text {
                anchors {
                    left: vpnKey.right
                    leftMargin: Theme.d(root.kvKeyGapPx)
                    right: tailnet.left
                    rightMargin: tunnelLine.gap
                    baseline: vpnKey.baseline
                }
                text: Net.vpnLine(root.links)
                // VPN names are the user's own, which AutoText would render as markup.
                textFormat: Text.PlainText
                color: root.tone(text !== Net.MISSING, root.linksLive)
                elide: Text.ElideRight
                font: kvLine.font
            }

            KvPair {
                id: tailnet

                anchors {
                    right: parent.right
                    verticalCenter: vpnKey.verticalCenter
                }
                key: "TS"
                value: Net.tsLine(root.links)
                valueColor: root.tone(value !== Net.MISSING, root.linksLive)
            }
        }
    }

    Item {
        id: linkList

        readonly property real rowHeight: Math.ceil(linkLine.height)
        readonly property real rowGap: Theme.d(root.linkRowGapPx)
        readonly property var rows: Net.linkRows(root.links)
        readonly property var noRow: ({
                name: "",
                detail: "",
                state: ""
            })
        // Rows that don't fit are dropped whole, the last first.
        readonly property int fitting: Math.max(0, Math.floor((height + rowGap) / (rowHeight + rowGap)))

        anchors {
            top: stack.bottom
            topMargin: Theme.d(root.stackGapPx)
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        visible: root.style === root.panelStyle

        Repeater {
            // A count rather than the rows, so a row keeps its object through the data's updates.
            model: linkList.visible ? Math.min(linkList.rows.length, linkList.fitting) : 0

            Item {
                id: linkRow

                required property int index
                // A message that drops a link may reach this binding before the Repeater drops the row.
                readonly property var link: linkList.rows[index] ?? linkList.noRow

                y: index * (linkList.rowHeight + linkList.rowGap)
                width: linkList.width
                height: linkList.rowHeight

                Text {
                    id: linkName

                    width: Theme.d(root.linkNamePx)
                    text: linkRow.link.name
                    textFormat: Text.PlainText
                    color: Theme.dim
                    elide: Text.ElideRight
                    font: linkLine.font
                }

                Text {
                    anchors {
                        left: linkName.right
                        leftMargin: Theme.d(root.linkGapPx)
                        right: linkState.left
                        rightMargin: Theme.d(root.linkGapPx)
                        baseline: linkName.baseline
                    }
                    text: linkRow.link.detail
                    // Connection names and addresses come from NetworkManager and Tailscale.
                    textFormat: Text.PlainText
                    color: root.linksLive ? Theme.text : Theme.dim
                    elide: Text.ElideRight
                    font: linkLine.font
                }

                Text {
                    id: linkState

                    anchors {
                        right: parent.right
                        baseline: linkName.baseline
                    }
                    text: linkRow.link.state
                    textFormat: Text.PlainText
                    color: root.tone(linkRow.link.state !== Net.DOWN_TEXT, root.linksLive)
                    font: linkLine.font
                }
            }
        }
    }
}
