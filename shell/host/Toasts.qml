import QtQuick
import Quickshell
import Quickshell.Wayland
import GhostKit
import "../kit/GhostKit/lib/layout.mjs" as Layout
import "../kit/GhostKit/lib/notifications.mjs" as Rules

// New notifications as cards under the bar at the screen's right edge, on a surface the size of the stack so that the
// desktop around it keeps its clicks. The host keeps two lists: the ids that have had their toast (never shown again
// this session) and the ids on screen now, which the queue rules keep in their slots.
Scope {
    id: root

    // Design pixels, from the notifications mockup's .toasts and .more.
    readonly property real stackWidthPx: 360
    readonly property real stackSideGapPx: 12
    readonly property real stackBarGapPx: 8
    readonly property real stackSpacingPx: 8
    readonly property real moreTextPx: 8.5
    readonly property real morePadRightPx: 2
    // Letter spacing in em.
    readonly property real moreTracking: 0.14

    required property var screen

    readonly property var items: Notifications.snapshot?.items ?? []
    readonly property var settings: Data.config?.notifications ?? null
    property var shownIds: new Set()
    property var visibleIds: []
    readonly property var queue: Rules.toastQueue({
        items: items,
        shown: shownIds,
        visible: visibleIds,
        shellStartS: Notifications.shellStartS,
        maxPopups: settings?.max_popups ?? 0,
        suppressed: Notifications.suppressed
    })
    readonly property bool atBottom: Data.config?.bar.position === "bottom"

    // Logged once per toast, however often it leaves and comes back for a free slot.
    readonly property var announced: new Set()
    // The ids whose end is logged, so an end that two paths reach is logged once.
    readonly property var endedIds: new Set()

    function sync() {
        const ids = queue.show;
        if (ids.join() === visibleIds.join()) {
            return;
        }
        for (const id of ids) {
            if (!announced.has(id)) {
                announced.add(id);
                console.info("toast " + id + " " + items.find(item => item.id === id)?.app);
            }
        }
        for (const id of visibleIds) {
            if (!ids.includes(id) && !shownIds.has(id) && !items.some(item => item.id === id)) {
                logEnd(id, "gone");
            }
        }
        visibleIds = ids;
    }

    function logEnd(id, reason) {
        if (!endedIds.has(id)) {
            endedIds.add(id);
            console.info("toast " + id + " ended " + reason);
        }
    }

    // The toast has had its time, whatever ended it: it never comes back, even if the history keeps the record.
    function finish(id, reason) {
        logEnd(id, reason);
        shownIds = new Set(shownIds).add(id);
    }

    function dismiss(id) {
        finish(id, "dismissed");
        Notifications.dismiss(id);
    }

    function activate(id, key) {
        finish(id, "invoked");
        Notifications.invoke(id, key);
    }

    function expire(id) {
        finish(id, "timeout");
        if (Notifications.widgetVisible) {
            Data.call("notifications.read", {
                ids: [id]
            });
        }
    }

    // Deferred: sync() writes visibleIds, which the queue is computed from, and a write inside its own evaluation is a binding loop.
    onQueueChanged: Qt.callLater(sync)
    Component.onCompleted: sync()

    LazyLoader {
        active: root.screen !== null && root.queue.show.length > 0

        PanelWindow {
            id: window

            // Whole device pixels, so the stack starts where the bar ends.
            readonly property real bandPx: Math.round(Theme.d(Layout.BAR_BAND_PX) * devicePixelRatio) / devicePixelRatio
            // A Column reads 0 until its first layout, and a layer surface resized faster than the compositor answers is
            // left at a stale size with the content clipped, so the size is taken once the content holds still.
            property int settledHeight: 0
            readonly property int settleMs: 50

            Timer {
                id: settle

                interval: window.settleMs
                running: true
                onTriggered: window.settledHeight = Math.ceil(stack.implicitHeight)
            }

            Connections {
                target: stack

                function onImplicitHeightChanged() {
                    settle.restart();
                }
            }

            screen: root.screen
            anchors {
                top: !root.atBottom
                bottom: root.atBottom
                right: true
            }
            margins {
                top: root.atBottom ? 0 : window.bandPx + Theme.d(root.stackBarGapPx)
                bottom: root.atBottom ? window.bandPx + Theme.d(root.stackBarGapPx) : 0
                right: Theme.d(root.stackSideGapPx)
            }
            exclusionMode: ExclusionMode.Ignore
            color: "transparent"
            implicitWidth: Theme.d(root.stackWidthPx)
            implicitHeight: settledHeight
            visible: settledHeight > 0
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "utility"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

            Component {
                id: waitingLabel

                Text {
                    width: parent.width
                    text: "+" + root.queue.waiting + " WAITING"
                    textFormat: Text.PlainText
                    horizontalAlignment: Text.AlignRight
                    rightPadding: Theme.d(root.morePadRightPx)
                    color: Theme.dim
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.moreTextPx)
                        weight: Font.Medium
                        letterSpacing: Theme.d(root.moreTextPx) * root.moreTracking
                    }
                }
            }

            Column {
                id: stack

                width: Theme.d(root.stackWidthPx)
                spacing: Theme.d(root.stackSpacingPx)

                // The label sits at the far end from the bar: below a top bar's stack and above a bottom bar's.
                Loader {
                    width: parent.width
                    active: root.atBottom && root.queue.waiting > 0
                    sourceComponent: waitingLabel
                }

                Repeater {
                    // Newest nearest the bar, which a bottom bar puts at the end of the column.
                    model: ScriptModel {
                        values: root.atBottom ? root.queue.show.slice().reverse() : root.queue.show
                    }

                    Toast {
                        id: toast

                        required property int modelData

                        width: stack.width
                        record: root.items.find(item => item.id === modelData)
                        timeoutS: root.settings?.popup_timeout_s ?? 6
                        showIcon: root.settings?.icons ?? true
                        onTimedOut: root.expire(modelData)
                        onDismissRequested: root.dismiss(modelData)
                        onActivated: key => root.activate(modelData, key)
                    }
                }

                Loader {
                    width: parent.width
                    active: !root.atBottom && root.queue.waiting > 0
                    sourceComponent: waitingLabel
                }
            }
        }
    }
}
