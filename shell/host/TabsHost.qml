import QtQuick
import GhostKit

// A tabs node: a body that hosts only the active tab's widget, so the other tabs cost nothing until
// they are chosen, and a strip of tab titles drawn at the right of that widget's panel header.
Item {
    id: root

    // Design pixels, from the reference mockup's .tabs.
    readonly property real tabSidePx: 8
    readonly property real tabVerticalPx: 2
    readonly property real tabGapPx: 2
    readonly property real textPx: 10
    // Letter spacing in em.
    readonly property real tracking: 0.14
    // Design pixels, from the mockup's .ph: Panel's header row, whose line the strip sits on.
    readonly property real headerHeightPx: 26
    readonly property real headerTopPx: 9
    readonly property real headerBottomPx: 6
    readonly property real headerSidePx: 12

    // A tabs node as computeRects places it: `tabs` (each a slot with a `title`), `default` and `path`.
    required property var node
    // The view the node belongs to, under which Views keeps the chosen tab.
    required property string view
    // A layout change may have removed the tab chosen before.
    property int current: {
        const chosen = Views.tabIndex(view, node.path, node.default);
        return chosen < node.tabs.length ? chosen : node.default;
    }

    readonly property real textSize: Theme.d(textPx)

    onCurrentChanged: Views.setTabIndex(view, node.path, current)

    Connections {
        target: Views

        function onTabRequested(view, path, index) {
            if (view === root.view && path === root.node.path) {
                root.current = index;
            }
        }
    }

    WidgetHost {
        anchors.fill: parent
        slot: root.node.tabs[root.current]
        headerReserve: strip.width
    }

    Item {
        id: headerLine

        anchors {
            top: parent.top
            topMargin: Theme.d(root.headerTopPx)
            right: parent.right
            rightMargin: Theme.d(root.headerSidePx)
        }
        height: Theme.d(root.headerHeightPx - root.headerTopPx - root.headerBottomPx)
    }

    Row {
        id: strip

        anchors {
            right: headerLine.right
            verticalCenter: headerLine.verticalCenter
        }
        spacing: Theme.d(root.tabGapPx)

        Repeater {
            model: root.node.tabs

            Rectangle {
                id: tab

                required property var modelData
                required property int index
                readonly property bool selected: index === root.current

                width: title.implicitWidth + Theme.d(2 * root.tabSidePx)
                height: title.implicitHeight + Theme.d(2 * root.tabVerticalPx)
                color: selected ? Theme.signal : "transparent"

                Text {
                    id: title

                    anchors.centerIn: parent
                    text: tab.modelData.title
                    // A third-party manifest supplies the title, which AutoText would render as markup.
                    textFormat: Text.PlainText
                    // The mockup's dark text on the signal fill is the ground colour.
                    color: tab.selected ? Theme.ground : Theme.dim
                    font {
                        family: Theme.font
                        pixelSize: root.textSize
                        weight: Font.DemiBold
                        letterSpacing: root.textSize * root.tracking
                        capitalization: Font.AllUppercase
                    }
                }

                // A ClickArea rather than a MouseArea, so the view adds the tab to the window's input region.
                ClickArea {
                    anchors.fill: parent
                    onClicked: root.current = tab.index
                }
            }
        }
    }
}
