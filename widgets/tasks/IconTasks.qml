import QtQuick
import GhostKit

// The icon style: one cell per app, with a mark under the icon for each window.
Item {
    id: root

    // Design pixels, from the reference mockup's .task, .pips and .dock .task.
    readonly property real cellPx: 28
    readonly property real iconPx: 17
    readonly property real gapPx: 1
    readonly property real dockCellWidthPx: 40
    readonly property real dockCellHeightPx: 38
    readonly property real dockIconPx: 22
    readonly property real dockGapPx: 2
    // Layout B's dock pads its cells by this much, which a tip has to clear as well as the one pixel past the bar.
    readonly property real dockPadPx: 6
    readonly property real tipGapPx: 1
    readonly property real markWidthPx: 4
    readonly property real focusedMarkWidthPx: 12
    readonly property real markHeightPx: 2
    readonly property real markGapPx: 2
    readonly property real markInsetPx: 2
    readonly property real badgeTextPx: 8
    readonly property real badgePadXPx: 3
    readonly property real badgeLinePx: 10
    readonly property real separatorWidthPx: 1
    readonly property real separatorHeightPx: 24
    readonly property real separatorMarginPx: 6
    readonly property real hoverFillAlpha: 0.08
    readonly property real notRunningOpacity: 0.4
    readonly property int hoverOpenMs: 300
    readonly property int hoverCloseMs: 200

    required property var host

    // The dock is the only place layout B has tasks, and its cells are larger.
    readonly property bool inDock: Data.config !== null && Data.config.bar.layout === "b"
    // Marks sit on the side facing the screen's middle, so under a bar at the top and over one at the bottom.
    readonly property bool marksAtTop: !inDock && Data.config !== null && Data.config.bar.position === "bottom"
    readonly property real tileWidth: Theme.d(inDock ? dockCellWidthPx : cellPx)
    readonly property real tileHeight: Theme.d(inDock ? dockCellHeightPx : cellPx)
    readonly property string firstUnpinned: {
        const tasks = host.tasks;
        const index = tasks.findIndex(task => !task.pinned);
        return index > 0 ? tasks[index].appId : "";
    }

    implicitWidth: row.implicitWidth

    Row {
        id: row

        height: parent.height
        spacing: Theme.d(root.inDock ? root.dockGapPx : root.gapPx)

        Repeater {
            model: root.host.appIds

            Row {
                id: slot

                required property string modelData

                readonly property var task: root.host.byApp[modelData] ?? {
                    appId: modelData,
                    pinned: false,
                    windows: [],
                    focused: false
                }
                readonly property bool running: task.windows.length > 0
                readonly property var entry: root.host.entryOf(task.appId)
                readonly property var badge: Tasks.badge(root.host.launcherEntries, task.appId)
                readonly property var markInfo: Tasks.marks(task)
                readonly property color markColor: badge.urgent ? Theme.warn : Theme.signal
                readonly property color focusedMarkColor: badge.urgent ? Theme.warn : Theme.hi
                readonly property bool listOpen: (shown || clickOpen) && !menuOpen
                readonly property bool wantsList: area.containsMouse || listHovered
                // The hover list settles after a delay either way, so a pointer passing over the bar opens nothing.
                property bool shown: false
                property bool clickOpen: false
                property bool menuOpen: false
                property bool listHovered: false

                function closeLists() {
                    settle.stop();
                    shown = false;
                    clickOpen = false;
                    listHovered = false;
                }

                function press(button) {
                    if (button === Qt.RightButton) {
                        closeLists();
                        menuOpen = true;
                        return;
                    }
                    if (button === Qt.MiddleButton) {
                        root.host.launch(task);
                        return;
                    }
                    const action = Tasks.click(task);
                    if (action.kind === "list") {
                        clickOpen = true;
                    } else {
                        closeLists();
                        root.host.perform(task, action);
                    }
                }

                function menuItems() {
                    const items = [];
                    const actions = entry ? entry.actions : [];
                    for (let index = 0; index < actions.length; index++) {
                        items.push({
                            label: actions[index].name,
                            kind: "action",
                            rule: false,
                            action: actions[index]
                        });
                    }
                    if (entry) {
                        items.push({
                            label: task.pinned ? "Unpin" : "Pin",
                            kind: "pin",
                            rule: items.length > 0
                        });
                    }
                    if (running) {
                        items.push({
                            label: "Close",
                            kind: "close",
                            rule: items.length > 0
                        });
                    }
                    return items;
                }

                function choose(item) {
                    menuOpen = false;
                    if (item.kind === "action") {
                        Launch.action(item.action);
                    } else if (item.kind === "pin") {
                        root.host.setPinned(task, !task.pinned);
                    } else {
                        root.host.closeApp(task);
                    }
                }

                spacing: Theme.d(root.inDock ? root.dockGapPx : root.gapPx)
                height: parent.height

                onWantsListChanged: {
                    if (wantsList === shown) {
                        settle.stop();
                        return;
                    }
                    settle.interval = wantsList ? root.hoverOpenMs : root.hoverCloseMs;
                    settle.restart();
                }
                // A closed popup never reports that the pointer left it.
                onListOpenChanged: {
                    if (!listOpen) {
                        listHovered = false;
                    }
                }

                Connections {
                    target: root.host

                    function onListRequested(appId) {
                        if (appId === slot.task.appId) {
                            slot.clickOpen = true;
                        }
                    }
                }

                Timer {
                    id: settle

                    onTriggered: slot.shown = slot.wantsList
                }

                Item {
                    visible: root.inDock && slot.task.appId === root.firstUnpinned
                    width: Theme.d(2 * root.separatorMarginPx + root.separatorWidthPx)
                    height: parent.height

                    Rectangle {
                        anchors.centerIn: parent
                        width: Theme.d(root.separatorWidthPx)
                        height: Theme.d(root.separatorHeightPx)
                        color: Theme.hair
                    }
                }

                Item {
                    id: cell

                    width: root.tileWidth
                    height: parent.height

                    Rectangle {
                        anchors.centerIn: parent
                        width: root.tileWidth
                        height: root.tileHeight
                        color: area.containsMouse ? Qt.alpha(Theme.signal, root.hoverFillAlpha) : "transparent"
                    }

                    AppIcon {
                        id: icon

                        anchors.centerIn: parent
                        name: root.host.iconOf(slot.task)
                        focused: slot.task.focused
                        size: Theme.d(root.inDock ? root.dockIconPx : root.iconPx)
                        opacity: slot.running ? 1 : root.notRunningOpacity
                    }

                    Rectangle {
                        visible: slot.badge.count !== null
                        anchors {
                            right: icon.right
                            top: icon.top
                            rightMargin: -Theme.d(root.badgePadXPx)
                            topMargin: -Theme.d(root.badgeLinePx / 2)
                        }
                        width: countText.implicitWidth + Theme.d(2 * root.badgePadXPx)
                        height: Theme.d(root.badgeLinePx)
                        color: Theme.signal

                        Text {
                            id: countText

                            anchors.centerIn: parent
                            text: slot.badge.count ?? ""
                            color: Theme.ground
                            font {
                                family: Theme.font
                                pixelSize: Theme.d(root.badgeTextPx)
                                weight: Font.Bold
                            }
                        }
                    }

                    // A download shows as filled segments in the place of the window marks.
                    Row {
                        id: marks

                        readonly property bool showsProgress: slot.badge.progress !== null
                        readonly property int segments: showsProgress ? 4 : slot.markInfo.count
                        readonly property int filled: showsProgress ? Math.round(slot.badge.progress * segments) : segments

                        anchors.horizontalCenter: parent.horizontalCenter
                        y: root.marksAtTop ? (parent.height - root.tileHeight) / 2 + Theme.d(root.markInsetPx) : (parent.height + root.tileHeight) / 2 - Theme.d(root.markInsetPx + root.markHeightPx)
                        spacing: Theme.d(root.markGapPx)

                        Repeater {
                            model: marks.segments

                            Rectangle {
                                required property int index

                                readonly property bool isFocusedMark: !marks.showsProgress && index === 0 && slot.markInfo.focusedFirst

                                width: Theme.d(isFocusedMark ? root.focusedMarkWidthPx : root.markWidthPx)
                                height: Theme.d(root.markHeightPx)
                                color: index >= marks.filled ? Theme.hair : (isFocusedMark ? slot.focusedMarkColor : slot.markColor)
                            }
                        }
                    }

                    MouseArea {
                        id: area

                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                        cursorShape: Qt.PointingHandCursor
                        onClicked: mouse => slot.press(mouse.button)
                    }

                    BarPopup {
                        anchorItem: cell
                        gap: Theme.d(root.inDock ? root.dockPadPx + root.tipGapPx : root.tipGapPx)
                        open: slot.listOpen
                        catchOutside: slot.clickOpen
                        onDismissed: slot.closeLists()
                        content: Component {
                            TitleList {
                                appName: root.host.nameOf(slot.task)
                                windows: slot.task.windows
                                onHoveredChanged: slot.listHovered = hovered
                                onPicked: id => {
                                    slot.closeLists();
                                    root.host.activate(id);
                                }
                            }
                        }
                    }

                    BarPopup {
                        anchorItem: cell
                        gap: Theme.d(root.inDock ? root.dockPadPx + root.tipGapPx : root.tipGapPx)
                        open: slot.menuOpen
                        catchOutside: true
                        onDismissed: slot.menuOpen = false
                        content: Component {
                            TaskMenu {
                                items: slot.menuItems()
                                onPicked: item => slot.choose(item)
                            }
                        }
                    }
                }
            }
        }
    }
}
