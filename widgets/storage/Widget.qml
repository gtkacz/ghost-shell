import QtQuick
import GhostKit

// Every drive: a live one with its mounts, temperature and used space, a dead one as a crit alert, an
// unmounted one as a dim line, and the read and write rates of them all. Rows that don't fit are
// dropped whole, the least important first.
Panel {
    id: root

    // Design pixels, from the reference mockup's Storage panel.
    readonly property real stackGapPx: 8
    readonly property real rowGapPx: 5
    readonly property real lineGapPx: 2
    readonly property real inlineGapPx: 6
    readonly property real textPx: 9.5
    readonly property real footerPx: 10
    readonly property real footerGapPx: 4
    // The mockup's .alert: 3 px of padding around a 14 px chip line inside a 1 px border.
    readonly property real alertPx: 22
    // Fractions of the drive, for the used-space meter.
    readonly property real usedWarn: 0.85
    readonly property real usedCrit: 0.95

    // When the rows don't all fit, lower ranks are kept first: a dead drive is the crit item, and a
    // drive with nothing mounted matters least.
    readonly property var rankByState: ({
            dead: 0,
            live: 1,
            unmounted: 2
        })

    property var settings

    readonly property var disks: Data.topics.disks || null
    readonly property bool live: Data.connected && !Data.stale
    readonly property var drives: disks ? disks.drives.filter(drive => settings.show_unmounted || drive.state !== "unmounted") : []
    readonly property real lineHeight: Math.ceil(line.height)
    readonly property real inlineGap: Theme.d(inlineGapPx)

    function rowHeight(drive) {
        switch (drive.state) {
        case "live":
            return 2 * lineHeight + Theme.d(lineGapPx);
        case "dead":
            return Theme.d(alertPx);
        default:
            return lineHeight;
        }
    }

    function total(key) {
        return disks ? disks.drives.reduce((sum, drive) => sum + drive[key], 0) : null;
    }

    // Missing and stale readings are dim, so "--" never reads as a value.
    function tone(value, liveColor) {
        return live && Number.isFinite(value) ? liveColor : Theme.dim;
    }

    function tempColor(value) {
        if (value >= Theme.tempCrit) {
            return tone(value, Theme.crit);
        }
        if (value >= Theme.tempWarn) {
            return tone(value, Theme.warn);
        }
        return tone(value, Theme.hi);
    }

    anchors.fill: parent
    label: "Storage"
    tagKey: "disk"
    meta: disks ? drives.length + (drives.length === 1 ? " DRIVE" : " DRIVES") : "--"

    FontMetrics {
        id: line

        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
            weight: Font.Medium
        }
    }

    TextMetrics {
        id: percentSpan

        font: line.font
        text: Fmt.pct(Theme.percent)
    }

    Item {
        id: list

        readonly property real rowGap: Theme.d(root.rowGapPx)
        // The drives that fit, each with its y, in the daemon's order.
        readonly property var placed: {
            const ranked = root.drives.map((drive, order) => ({
                        drive: drive,
                        order: order
                    })).sort((a, b) => root.rankByState[a.drive.state] - root.rankByState[b.drive.state] || a.order - b.order);
            const kept = new Set();
            let used = 0;
            for (const entry of ranked) {
                const needed = root.rowHeight(entry.drive) + (kept.size > 0 ? rowGap : 0);
                if (used + needed > height) {
                    break;
                }
                used += needed;
                kept.add(entry.order);
            }
            const rows = [];
            let y = 0;
            root.drives.forEach((drive, order) => {
                if (kept.has(order)) {
                    rows.push({
                        drive: drive,
                        y: y
                    });
                    y += root.rowHeight(drive) + rowGap;
                }
            });
            return rows;
        }
        // A Repeater per kind of row, each counting its rows rather than holding them, so a row keeps
        // its object through the 1 Hz data and an alert's arrival flicker plays once, not every second.
        readonly property var liveRows: placed.filter(row => row.drive.state === "live")
        readonly property var deadRows: placed.filter(row => row.drive.state === "dead")
        readonly property var unmountedRows: placed.filter(row => row.drive.state !== "live" && row.drive.state !== "dead")

        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: footer.top
            bottomMargin: Theme.d(root.stackGapPx)
        }

        Repeater {
            model: list.liveRows.length

            Item {
                id: liveRow

                required property int index
                readonly property var drive: list.liveRows[index].drive

                y: list.liveRows[index].y
                width: list.width
                height: root.rowHeight(drive)

                Text {
                    id: liveName

                    width: Math.min(implicitWidth, parent.width - liveTemp.width - root.inlineGap)
                    text: liveRow.drive.label
                    color: root.live ? Theme.hi : Theme.dim
                    elide: Text.ElideRight
                    font: line.font
                }

                Text {
                    anchors {
                        left: liveName.right
                        leftMargin: root.inlineGap
                        baseline: liveName.baseline
                    }
                    width: Math.max(0, liveTemp.x - root.inlineGap - x)
                    text: liveRow.drive.mounts.join(" · ")
                    color: Theme.dim
                    elide: Text.ElideRight
                    font: line.font
                }

                Digits {
                    id: liveTemp

                    anchors {
                        right: parent.right
                        baseline: liveName.baseline
                    }
                    text: Fmt.tempShort(liveRow.drive.temp_c)
                    pixelSize: Theme.d(root.textPx)
                    weight: Font.Medium
                    color: root.tempColor(liveRow.drive.temp_c)
                }

                SegMeter {
                    anchors {
                        left: parent.left
                        right: parent.right
                        rightMargin: percentSpan.advanceWidth + root.inlineGap
                        verticalCenter: livePercent.verticalCenter
                    }
                    value: Number.isFinite(liveRow.drive.used_pct) ? liveRow.drive.used_pct / Theme.percent : null
                    warnAt: root.usedWarn
                    critAt: root.usedCrit
                }

                Digits {
                    id: livePercent

                    anchors {
                        right: parent.right
                        top: liveName.bottom
                        topMargin: Theme.d(root.lineGapPx)
                    }
                    text: Fmt.pct(liveRow.drive.used_pct)
                    pixelSize: Theme.d(root.textPx)
                    weight: Font.Medium
                    color: root.tone(liveRow.drive.used_pct, Theme.hi)
                }
            }
        }

        Repeater {
            model: list.deadRows.length

            AlertBox {
                required property int index
                readonly property var drive: list.deadRows[index].drive

                y: list.deadRows[index].y
                width: list.width
                height: Theme.d(root.alertPx)
                level: "crit"
                chipKey: "crit"
                text: drive.label + " · " + drive.kernel_name
                trailing: "DEAD"
            }
        }

        Repeater {
            model: list.unmountedRows.length

            Item {
                id: unmountedRow

                required property int index
                readonly property var drive: list.unmountedRows[index].drive

                y: list.unmountedRows[index].y
                width: list.width
                height: root.rowHeight(drive)

                Text {
                    id: unmountedName

                    width: Math.min(implicitWidth, parent.width - unmountedState.width - root.inlineGap)
                    text: unmountedRow.drive.label
                    color: Theme.dim
                    elide: Text.ElideRight
                    font: line.font
                }

                Text {
                    anchors {
                        left: unmountedName.right
                        leftMargin: root.inlineGap
                    }
                    width: Math.max(0, unmountedState.x - root.inlineGap - x)
                    text: unmountedRow.drive.kernel_name
                    color: Theme.dim
                    elide: Text.ElideRight
                    font: line.font
                }

                Text {
                    id: unmountedState

                    anchors.right: parent.right
                    text: "UNMOUNTED"
                    color: Theme.dim
                    font: line.font
                }
            }
        }
    }

    Item {
        id: footer

        readonly property var readBps: root.total("read_bps")
        readonly property var writeBps: root.total("write_bps")

        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        height: readKey.implicitHeight

        Text {
            id: readKey

            text: "R"
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(root.footerPx)
                weight: Font.Medium
            }
        }

        Digits {
            anchors {
                left: readKey.right
                leftMargin: Theme.d(root.footerGapPx)
                baseline: readKey.baseline
            }
            text: Fmt.rate(footer.readBps)
            pixelSize: Theme.d(root.footerPx)
            weight: Font.Medium
            color: root.tone(footer.readBps, Theme.hi)
        }

        Text {
            anchors {
                right: writeValue.left
                rightMargin: Theme.d(root.footerGapPx)
                baseline: readKey.baseline
            }
            text: "W"
            color: Theme.dim
            font: readKey.font
        }

        Digits {
            id: writeValue

            anchors {
                right: parent.right
                baseline: readKey.baseline
            }
            text: Fmt.rate(footer.writeBps)
            pixelSize: Theme.d(root.footerPx)
            weight: Font.Medium
            color: root.tone(footer.writeBps, Theme.hi)
        }
    }
}
