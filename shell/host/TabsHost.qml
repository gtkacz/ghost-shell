import QtQuick
import GhostKit

// A tabs node: a strip of tab titles above a body that hosts only the active tab's widget, so the
// other tabs cost nothing until they are chosen.
Item {
    id: root

    // Design pixels, from the reference mockup's .tabs.
    readonly property real tabSidePx: 8
    readonly property real tabVerticalPx: 2
    readonly property real tabGapPx: 2
    readonly property real textPx: 10
    // Letter spacing in em.
    readonly property real tracking: 0.14

    // A tabs node as computeRects places it: `tabs` (each a slot with a `title`), `default` and `path`.
    required property var node
    // The view the node belongs to, under which Views keeps the chosen tab.
    required property string view
    // Logical pixels; the view decides it, because the strip is also where the window takes input.
    required property real stripHeight
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

    Item {
        id: strip

        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
        }
        height: root.stripHeight
        clip: true

        Row {
            anchors.verticalCenter: parent.verticalCenter
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

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.current = tab.index
                    }
                }
            }
        }
    }

    WidgetHost {
        anchors {
            top: strip.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        slot: root.node.tabs[root.current]
    }
}
