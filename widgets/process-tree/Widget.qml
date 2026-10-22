import QtQuick
import QtQuick.Shapes
import GhostKit
import "tree.mjs" as Tree

// Every process as a radial tree round pid 1, after the reference mockup's p-tree: a dot per process sized by its
// memory, coloured only when it is busy, and named when it is busy or heads a large branch. A click on a node folds its
// branch into it and a second click opens it again, for the rest of the session.
Panel {
    id: root

    // Design pixels, from the reference mockup's drawTree.
    readonly property real edgeWidthPx: 0.6
    readonly property real labelPx: 8
    // The gap between a dot and its label, and the label's baseline below the dot's centre line.
    readonly property real labelGapPx: 4
    readonly property real labelBaselinePx: 3
    readonly property real rootSquarePx: 6
    readonly property real rootLabelGapPx: 8
    readonly property real rootLabelBaselinePx: 6
    // How far from a dot a click still picks it.
    readonly property real hitSlopPx: 6
    // Opacities, from the mockup's rgba('sig', …).
    readonly property real edgeOpacity: 0.2
    readonly property real quietDotOpacity: 0.55
    readonly property real degreesPerRadian: 180 / Math.PI

    property var settings

    readonly property string topicName: "procs.tree"
    readonly property var tree: Data.topic(topicName).value
    readonly property bool live: Data.connected && !Data.stale
    readonly property bool wantsTree: visible && !Data.covered

    property bool holdsTree: false
    // Node keys, for this session only; every node starts open.
    property var collapsed: new Set()
    property var built: null
    property var dots: []
    property var labels: []

    // Once per widget rather than per frame: every change of the subscription makes the daemon replay each topic.
    function holdTree() {
        if (wantsTree === holdsTree) {
            return;
        }
        if (wantsTree) {
            Data.want(topicName);
        } else {
            Data.release(topicName);
        }
        holdsTree = wantsTree;
    }

    function quiet(color) {
        return Qt.rgba(color.r, color.g, color.b, quietDotOpacity);
    }

    // Colour only for attention, and none for a reading that may be stale.
    function dotColor(level) {
        if (!live || level === "ok") {
            return quiet(Theme.signal);
        }
        return level === "crit" ? Theme.crit : level === "warn" ? Theme.warn : Theme.hi;
    }

    function labelColor(level) {
        if (!live) {
            return Theme.dim;
        }
        return level === "crit" ? Theme.crit : level === "warn" ? Theme.warn : Theme.text;
    }

    function rebuild() {
        const w = area.width / Theme.u;
        const h = area.height / Theme.u;
        if (!tree || !(w > 0) || !(h > 0)) {
            built = null;
            dots = [];
            labels = [];
            drawEdges([]);
            return;
        }
        const next = Tree.layoutRadial(Tree.buildTree(tree, collapsed), w, h);
        collapsed = Tree.pruneCollapsed(collapsed, next);
        const nextDots = [];
        const nextLabels = [];
        const edges = [];
        next.nodes.forEach(node => {
            const size = Tree.dotSize(node.rss_kib);
            const level = Tree.dotLevel(node.cpu);
            nextDots.push({
                x: Math.round(Theme.d(node.x - size / 2)),
                y: Math.round(Theme.d(node.y - size / 2)),
                size: Math.max(1, Math.round(Theme.d(size))),
                level: level
            });
            if (node.parent) {
                edges.push(node);
            }
            if (node.parent && Tree.showsLabel(node, next.R)) {
                const flip = Math.cos(node.angle) < 0;
                nextLabels.push({
                    x: Theme.d(node.x),
                    y: Theme.d(node.y),
                    rotation: (flip ? node.angle + Math.PI : node.angle) * degreesPerRadian,
                    flip: flip,
                    text: Tree.labelText(node),
                    level: level
                });
            }
        });
        built = next;
        dots = nextDots;
        labels = nextLabels;
        drawEdges(edges);
    }

    // A curve from each parent to its child, bent through the point at the parent's radius on the child's angle. One
    // SVG path rather than an element per curve: a new tree then re-parses one string instead of touching hundreds of
    // path elements, which measured about 0.1-0.3 % of a core less.
    function drawEdges(edges) {
        edgeSvg.path = edges.map(node => {
            const parent = node.parent;
            const controlX = built.cx + Math.cos(node.angle) * parent.radius;
            const controlY = built.cy + Math.sin(node.angle) * parent.radius;
            return "M " + Theme.d(parent.x) + " " + Theme.d(parent.y) + " Q " + Theme.d(controlX) + " " + Theme.d(controlY) + " " + Theme.d(node.x) + " " + Theme.d(node.y);
        }).join(" ");
    }

    function toggleAt(x, y) {
        if (!built) {
            return;
        }
        const key = Tree.hitTest(built.nodes, x / Theme.u, y / Theme.u, hitSlopPx);
        if (key === null) {
            return;
        }
        collapsed = Tree.toggled(collapsed, built.nodes.get(key));
        rebuild();
    }

    anchors.fill: parent
    label: "Process tree"
    tagKey: "tree"
    meta: tree ? tree.procs.length + " PROCS" : "--"

    onTreeChanged: Qt.callLater(rebuild)
    // A hold changes a turn later, never inside the update of the binding that asked for it.
    onWantsTreeChanged: Qt.callLater(holdTree)
    Component.onCompleted: holdTree()
    // Directly: a call left for later is dropped once the widget is gone, and its claim would stay.
    Component.onDestruction: {
        if (holdsTree) {
            Data.release(topicName);
            holdsTree = false;
        }
    }

    Item {
        id: area

        anchors.fill: parent
        clip: true

        onWidthChanged: Qt.callLater(root.rebuild)
        onHeightChanged: Qt.callLater(root.rebuild)

        Shape {
            anchors.fill: parent
            // The default renderer leaves thin curves without antialiasing.
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: "transparent"
                strokeColor: Qt.rgba(Theme.signal.r, Theme.signal.g, Theme.signal.b, root.edgeOpacity)
                strokeWidth: Theme.d(root.edgeWidthPx)

                PathSvg {
                    id: edgeSvg
                }
            }
        }

        Repeater {
            model: root.dots

            Rectangle {
                required property var modelData

                x: modelData.x
                y: modelData.y
                width: modelData.size
                height: modelData.size
                color: root.dotColor(modelData.level)
            }
        }

        Repeater {
            model: root.labels

            // Turned about the dot, and turned over on the left half so that no label reads upside down.
            Item {
                required property var modelData

                x: modelData.x
                y: modelData.y
                rotation: modelData.rotation
                transformOrigin: Item.TopLeft

                Text {
                    x: parent.modelData.flip ? -Theme.d(root.labelGapPx) - implicitWidth : Theme.d(root.labelGapPx)
                    y: Theme.d(root.labelBaselinePx) - baselineOffset
                    text: parent.modelData.text
                    textFormat: Text.PlainText
                    color: root.labelColor(parent.modelData.level)
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.labelPx)
                        weight: Font.DemiBold
                    }
                }
            }
        }

        Rectangle {
            visible: root.built !== null
            x: Math.round(Theme.d((root.built ? root.built.cx : 0) - root.rootSquarePx / 2))
            y: Math.round(Theme.d((root.built ? root.built.cy : 0) - root.rootSquarePx / 2))
            width: Math.round(Theme.d(root.rootSquarePx))
            height: width
            color: Theme.signal
        }

        Text {
            visible: root.built !== null
            x: Theme.d((root.built ? root.built.cx : 0) + root.rootLabelGapPx)
            y: Theme.d((root.built ? root.built.cy : 0) - root.rootLabelBaselinePx) - baselineOffset
            text: root.built ? Tree.rootLabel(root.built.root) : ""
            textFormat: Text.PlainText
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(root.labelPx)
                weight: Font.DemiBold
            }
        }

        ClickArea {
            anchors.fill: parent
            onClicked: mouse => root.toggleAt(mouse.x, mouse.y)
        }
    }
}
