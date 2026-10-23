import QtQuick
import GhostKit
import "../kit/GhostKit/lib/theme.mjs" as ThemeLib

// One group of the bar or dock: its slots, each in a WidgetHost like the dashboard's. A slot is as wide as its widget's
// implicitWidth and as high as the group, or, in a fill group, shares the group's width with the others. A slot that
// can't be shown becomes a small mark instead.
Item {
    id: root

    // Design pixels.
    readonly property real markPx: 8
    readonly property real tipPx: 9.5
    readonly property real tipPadXPx: 8
    readonly property real tipPadYPx: 2
    readonly property real tipFillPct: 96
    // The tips sit one pixel past the bar.
    readonly property real tipGapPx: 1
    // Letter spacing in em.
    readonly property real tipTracking: 0.04

    // The resolved slots of the group, as Data.layout.bar carries them.
    property var slots: []
    // Which group, left, centre, right or dock, so every slot has a place of its own under FirstWrite.
    property string area
    property real spacing
    property bool fill: false
    // The widest this group may be, so a slot that would take everything (the tabs) is held to what the bar has left.
    property real limit: Infinity

    implicitWidth: row.implicitWidth

    Row {
        id: row

        height: parent.height
        spacing: root.spacing

        Repeater {
            model: root.slots

            Item {
                id: cell

                required property var modelData
                required property int index

                // A square, as high as the group, for a slot that failed.
                readonly property real wanted: host.failure !== "" ? root.height : (host.instance ? host.instance.implicitWidth : 0)
                readonly property real share: (root.width - root.spacing * (root.slots.length - 1)) / root.slots.length

                width: root.fill ? share : Math.min(wanted, Math.max(0, root.limit - cell.x))
                height: root.height
                visible: width > 0

                WidgetHost {
                    id: host

                    anchors.fill: parent
                    slot: cell.modelData
                    firstWriteScope: FirstWrite.hostScope("bar", root.area + "[" + cell.index + "]")
                    errorComponent: errorMark
                }

                Component {
                    id: errorMark

                    Item {
                        id: mark

                        // Lets the smoke see that the shell drew the mark, which the daemon's report of the slot does not show.
                        Component.onCompleted: console.info("bar mark " + host.widgetId)

                        Rectangle {
                            anchors.centerIn: parent
                            width: Theme.d(root.markPx)
                            height: width
                            color: Theme.crit
                        }

                        HoverHandler {
                            id: hover
                        }

                        BarPopup {
                            anchorItem: mark
                            gap: Theme.d(root.tipGapPx)
                            open: hover.hovered
                            content: Component {
                                Rectangle {
                                    implicitWidth: tip.implicitWidth + Theme.d(2 * root.tipPadXPx)
                                    implicitHeight: tip.implicitHeight + Theme.d(2 * root.tipPadYPx)
                                    color: ThemeLib.withAlpha(Theme.ground, root.tipFillPct)
                                    border {
                                        width: Theme.d(1)
                                        color: Theme.hair
                                    }

                                    Text {
                                        id: tip

                                        anchors.centerIn: parent
                                        text: host.widgetId
                                        textFormat: Text.PlainText
                                        color: Theme.dim
                                        font {
                                            family: Theme.font
                                            pixelSize: Theme.d(root.tipPx)
                                            weight: Font.DemiBold
                                            letterSpacing: Theme.d(root.tipPx) * root.tipTracking
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
