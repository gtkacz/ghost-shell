import QtQuick
import Quickshell
import Quickshell.Wayland
import GhostKit
import "../kit/GhostKit/lib/layout.mjs" as Layout
import "../kit/GhostKit/lib/notifications.mjs" as Rules
import "../kit/GhostKit/lib/theme.mjs" as ThemeLib
import "../kit/GhostKit/lib/views.mjs" as ViewsLib

// The notification centre: a panel dropped from the bell, under (or over) the bar, on a surface the size of the panel.
// Like the launcher it is on the Overlay layer and closes when it loses keyboard focus; the state and the actions are
// the Notifications singleton's, and this only draws them and forwards the keys.
Scope {
    id: root

    // Design pixels, from the notifications mockup's .centre, .search, .tools, .list and .foot.
    readonly property real panelWidthPx: 392
    readonly property real panelHeightPx: 560
    readonly property real screenSideGapPx: 12
    readonly property real searchMarginTopPx: 2
    readonly property real searchMarginBottomPx: 8
    readonly property real searchPadYPx: 3
    readonly property real searchPadXPx: 8
    readonly property real promptPx: 10
    readonly property real queryPx: 10
    readonly property real cursorWidthPx: 6
    readonly property real cursorHeightPx: 10
    readonly property real keyPadXPx: 5
    readonly property real keyTextPx: 8.5
    readonly property real toolsGapPx: 6
    readonly property real footPadTopPx: 8
    readonly property real footTextPx: 8.5
    // The header's line, from Panel: the meta sits at its right end.
    readonly property real headerTopPx: 9
    readonly property real headerLinePx: 11
    readonly property real headerSidePx: 12
    readonly property real metaTextPx: 10
    // The reveal: the same line the dashboard's print draws at its moving edge, in ten held steps.
    readonly property int revealSteps: 10
    readonly property int revealStepMs: 18
    readonly property real scanlinePx: 2
    readonly property real scanlineOpacity: 55
    // Letter spacing in em.
    readonly property real queryTracking: 0.08
    readonly property real toolTracking: 0.14
    readonly property real metaTracking: 0.1
    readonly property real emptyTracking: 0.2
    readonly property string metaSeparator: " · "

    required property var screen

    readonly property bool serving: Notifications.server === Rules.SERVER
    readonly property var allItems: Notifications.snapshot?.items ?? []
    readonly property var records: Notifications.records
    readonly property string emptyText: !serving ? (Notifications.server === "plasma" ? "PLASMA IS SERVING NOTIFICATIONS" : "NO SERVER") : "NO SIGNALS"

    component Tool: Rectangle {
        id: tool

        // An inline component cannot read the ids of the file around it, so its sizes are its own.
        readonly property real padXPx: 8
        readonly property real padYPx: 2
        readonly property real textPx: 8.5
        // Letter spacing in em.
        readonly property real tracking: 0.14

        property string label
        property bool lit: false

        signal pressed

        implicitWidth: toolText.implicitWidth + Theme.d(2 * padXPx + 2)
        implicitHeight: toolText.implicitHeight + Theme.d(2 * padYPx + 2)
        color: "transparent"
        border {
            width: Theme.d(1)
            color: tool.lit ? Theme.warn : Theme.hair
        }

        Text {
            id: toolText

            anchors.centerIn: parent
            text: tool.label
            textFormat: Text.PlainText
            color: tool.lit ? Theme.warn : Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(tool.textPx)
                weight: Font.Medium
                letterSpacing: Theme.d(tool.textPx) * tool.tracking
            }
        }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: tool.pressed()
        }
    }

    LazyLoader {
        active: Notifications.open && root.screen !== null

        PanelWindow {
            id: window

            readonly property bool atBottom: Data.config !== null && Data.config.bar.position === "bottom"
            // Whole device pixels, so the panel's edge meets the bar's.
            readonly property real bandPx: Math.round(Theme.d(Layout.BAR_BAND_PX) * devicePixelRatio) / devicePixelRatio
            readonly property real panelWidth: Theme.d(root.panelWidthPx)
            // The bell's right edge, or the toasts' side gap before the bell has been clicked, kept on the screen.
            readonly property real rightEdge: Notifications.anchorRect.width > 0 ? Notifications.anchorRect.x + Notifications.anchorRect.width : root.screen.width - Theme.d(root.screenSideGapPx)
            property int step: 1

            screen: root.screen
            anchors {
                top: !window.atBottom
                bottom: window.atBottom
                right: true
            }
            margins {
                top: window.atBottom ? 0 : window.bandPx
                bottom: window.atBottom ? window.bandPx : 0
                right: Math.max(0, Math.min(root.screen.width - window.rightEdge, root.screen.width - window.panelWidth))
            }
            implicitWidth: window.panelWidth
            implicitHeight: Theme.d(root.panelHeightPx)
            exclusionMode: ExclusionMode.Ignore
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "utility"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

            Item {
                id: stage

                readonly property real fullHeight: Theme.d(root.panelHeightPx)
                readonly property real revealed: ViewsLib.revealRows(window.step, root.revealSteps, Math.round(fullHeight * window.devicePixelRatio)) / window.devicePixelRatio

                width: window.panelWidth
                height: revealed
                y: window.atBottom ? parent.height - revealed : 0
                clip: true

                Item {
                    id: panelBox

                    // Read by every ScrambleText inside, so the header scrambles on each opening.
                    readonly property string firstWriteScope: FirstWrite.hostScope("centre", "panel")

                    width: stage.width
                    height: stage.fullHeight
                    anchors {
                        top: window.atBottom ? undefined : parent.top
                        bottom: window.atBottom ? parent.bottom : undefined
                    }

                    Panel {
                        id: panel

                        anchors.fill: parent
                        // Opaque, like the launcher's: at the mockup's 94 % the dashboard's bright text ghosts through.
                        fill: Theme.ground
                        label: "NOTIFICATIONS"
                        tagKey: "notif"

                        Item {
                            id: search

                            width: parent.width
                            height: Theme.d(root.searchMarginTopPx + root.searchMarginBottomPx) + field.height

                            Rectangle {
                                id: field

                                y: Theme.d(root.searchMarginTopPx)
                                width: parent.width
                                height: queryText.implicitHeight + Theme.d(2 * root.searchPadYPx + 2)
                                color: "transparent"
                                border {
                                    width: Theme.d(1)
                                    color: Theme.hair
                                }

                                Glyph {
                                    id: prompt

                                    x: Theme.d(root.searchPadXPx + 1)
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: "chevron-right"
                                    size: Theme.d(root.promptPx)
                                    color: Theme.signal
                                }

                                Text {
                                    id: queryText

                                    anchors {
                                        left: prompt.right
                                        leftMargin: Theme.d(root.searchPadXPx)
                                        verticalCenter: parent.verticalCenter
                                    }
                                    text: Notifications.query
                                    textFormat: Text.PlainText
                                    color: Theme.hi
                                    font {
                                        family: Theme.font
                                        pixelSize: Theme.d(root.queryPx)
                                        weight: Font.Medium
                                        letterSpacing: Theme.d(root.queryPx) * root.queryTracking
                                    }
                                }

                                Rectangle {
                                    x: queryText.x + queryText.width + Theme.d(1)
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: Theme.d(root.cursorWidthPx)
                                    height: Theme.d(root.cursorHeightPx)
                                    color: Theme.signal
                                    opacity: FrameClock.blinkOpacity
                                }

                                Text {
                                    anchors {
                                        left: queryText.right
                                        leftMargin: Theme.d(root.cursorWidthPx + root.searchPadXPx)
                                        verticalCenter: parent.verticalCenter
                                    }
                                    visible: Notifications.query === ""
                                    text: "FILTER"
                                    color: Theme.dim
                                    font {
                                        family: Theme.font
                                        pixelSize: Theme.d(root.queryPx)
                                        weight: Font.Medium
                                        letterSpacing: Theme.d(root.queryPx) * root.queryTracking
                                    }
                                }

                                Rectangle {
                                    anchors {
                                        right: parent.right
                                        rightMargin: Theme.d(root.searchPadXPx)
                                        verticalCenter: parent.verticalCenter
                                    }
                                    width: escText.implicitWidth + Theme.d(2 * root.keyPadXPx + 2)
                                    height: escText.implicitHeight + Theme.d(2)
                                    color: "transparent"
                                    border {
                                        width: Theme.d(1)
                                        color: Theme.hair
                                    }

                                    Text {
                                        id: escText

                                        anchors.centerIn: parent
                                        text: "ESC"
                                        color: Theme.dim
                                        font {
                                            family: Theme.font
                                            pixelSize: Theme.d(root.keyTextPx)
                                            weight: Font.Medium
                                        }
                                    }
                                }
                            }
                        }

                        Item {
                            id: tools

                            anchors.top: search.bottom
                            width: parent.width
                            height: visible ? clearAll.implicitHeight + Theme.d(root.toolsGapPx) : 0
                            visible: root.serving

                            Row {
                                spacing: Theme.d(root.toolsGapPx)

                                Tool {
                                    id: clearAll

                                    label: "MARK ALL READ"
                                    onPressed: Notifications.markAllRead()
                                }

                                Tool {
                                    label: "CLEAR"
                                    onPressed: Notifications.clear()
                                }
                            }

                            Tool {
                                anchors.right: parent.right
                                label: "DND"
                                lit: Notifications.dnd
                                onPressed: Notifications.toggleDnd()
                            }
                        }

                        Item {
                            id: foot

                            anchors.bottom: parent.bottom
                            width: parent.width
                            height: Theme.d(root.footPadTopPx) + footLeft.implicitHeight

                            Text {
                                id: footLeft

                                anchors {
                                    left: parent.left
                                    bottom: parent.bottom
                                }
                                visible: root.serving
                                text: root.records.length === 0 ? Rules.quietSince(root.allItems, Notifications.shellStartS) : Rules.footerLeft(root.records, Notifications.unread)
                                textFormat: Text.PlainText
                                color: Theme.dim
                                font {
                                    family: Theme.font
                                    pixelSize: Theme.d(root.footTextPx)
                                    weight: Font.Medium
                                    letterSpacing: Theme.d(root.footTextPx) * root.toolTracking
                                }
                            }

                            Text {
                                anchors {
                                    right: parent.right
                                    bottom: parent.bottom
                                }
                                text: "ENTER OPEN · DEL DISMISS"
                                color: Theme.dim
                                font: footLeft.font
                            }
                        }

                        Item {
                            anchors {
                                top: tools.bottom
                                bottom: foot.top
                                left: parent.left
                                right: parent.right
                            }

                            NotificationRows {
                                anchors.fill: parent
                                items: root.records
                                selected: Notifications.index
                                visible: root.records.length > 0
                                onActivated: id => Notifications.invoke(id, "default")
                                onDismissed: id => Notifications.dismiss(id)
                            }

                            Text {
                                anchors.centerIn: parent
                                visible: root.records.length === 0
                                text: root.emptyText
                                textFormat: Text.PlainText
                                color: Theme.dim
                                font {
                                    family: Theme.font
                                    pixelSize: Theme.d(root.queryPx)
                                    weight: Font.Medium
                                    letterSpacing: Theme.d(root.queryPx) * root.emptyTracking
                                }
                            }
                        }
                    }

                    // The panel's own meta is plain text, and here the counts of CRIT and WARN take their own colours.
                    Row {
                        id: meta

                        anchors {
                            right: parent.right
                            rightMargin: Theme.d(root.headerSidePx)
                        }
                        y: Theme.d(root.headerTopPx)
                        height: Theme.d(root.headerLinePx)

                        Repeater {
                            model: ScriptModel {
                                values: Rules.countsLine(root.allItems).split(root.metaSeparator)
                            }

                            Row {
                                id: part

                                required property string modelData
                                required property int index
                                readonly property string kind: modelData.split(" ").pop()

                                height: meta.height

                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: part.index > 0
                                    text: root.metaSeparator
                                    color: Theme.dim
                                    font: metaText.font
                                }

                                ScrambleText {
                                    id: metaText

                                    anchors.verticalCenter: parent.verticalCenter
                                    text: part.modelData
                                    scrambleKey: "centre:" + part.kind
                                    color: part.kind === "CRIT" ? Theme.crit : part.kind === "WARN" ? Theme.warn : Theme.dim
                                    font {
                                        family: Theme.font
                                        pixelSize: Theme.d(root.metaTextPx)
                                        weight: Font.Medium
                                        letterSpacing: Theme.d(root.metaTextPx) * root.metaTracking
                                    }
                                }
                            }
                        }
                    }
                }

                // Above the panel, on the edge the reveal is moving.
                Rectangle {
                    y: window.atBottom ? 0 : parent.height - height
                    width: parent.width
                    height: Math.max(1, Math.round(Theme.d(root.scanlinePx) * window.devicePixelRatio)) / window.devicePixelRatio
                    color: ThemeLib.withAlpha(Theme.hi, root.scanlineOpacity)
                    visible: window.step < root.revealSteps
                }
            }

            Timer {
                interval: root.revealStepMs
                repeat: true
                running: window.step < root.revealSteps
                onTriggered: window.step += 1
            }

            Item {
                property bool wasActive: false

                focus: true
                // The surface takes no exclusive focus, so activating anything else, by a click or a new window, closes the centre.
                Window.onActiveChanged: {
                    if (Window.active) {
                        wasActive = true;
                    } else if (wasActive) {
                        Notifications.close();
                    }
                }
                Keys.onPressed: event => {
                    const name = root.keyName(event.key);
                    if (name !== "") {
                        Notifications.key(name);
                    } else if (event.text.length === 1 && event.text >= " " && event.text !== "\x7f" && !(event.modifiers & (Qt.ControlModifier | Qt.MetaModifier | Qt.AltModifier))) {
                        Notifications.type(event.text);
                    } else {
                        return;
                    }
                    event.accepted = true;
                }
            }
        }
    }

    function keyName(key) {
        switch (key) {
        case Qt.Key_Escape:
            return "Escape";
        case Qt.Key_Return:
        case Qt.Key_Enter:
            return "Return";
        case Qt.Key_Backspace:
            return "Backspace";
        case Qt.Key_Delete:
            return "Delete";
        case Qt.Key_Up:
            return "Up";
        case Qt.Key_Down:
            return "Down";
        default:
            return "";
        }
    }
}
