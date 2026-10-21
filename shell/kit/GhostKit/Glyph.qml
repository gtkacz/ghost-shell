import QtQuick
import QtQuick.Shapes

// An icon from the reference mockup, drawn in its 12-unit box and scaled to size. Arrows are chevrons
// and every icon is a path, never a Unicode glyph, so none depends on what the font covers.
Item {
    id: root

    // The box the paths below are drawn in, and the stroke the mockup gives the stroked ones.
    readonly property real boxUnits: 12
    readonly property real strokeUnits: 1.6
    // Filled shapes have no stroke. The search circle is two arcs because a path has no circle command.
    readonly property var shapes: ({
            "chevron-up": {
                path: "M2.6 7.6 6 4.6l3.4 3",
                filled: false
            },
            "chevron-down": {
                path: "M2.6 4.4 6 7.4l3.4-3",
                filled: false
            },
            "chevron-left": {
                path: "M7.6 2.6 4.6 6l3 3.4",
                filled: false
            },
            "chevron-right": {
                path: "M4.4 2.6 7.4 6l-3 3.4",
                filled: false
            },
            "swap": {
                path: "M5.2 2.6 2.2 6l3 3.4M6.8 2.6l3 3.4-3 3.4",
                filled: false
            },
            "prev": {
                path: "M2.6 3v6M9 2.8 5.6 6 9 9.2",
                filled: false
            },
            "next": {
                path: "M9.4 3v6M3 2.8 6.4 6 3 9.2",
                filled: false
            },
            "play": {
                path: "M3 2v8l7-4z",
                filled: true
            },
            "pause": {
                path: "M2.5 2h2.4v8h-2.4zM7.1 2h2.4v8h-2.4z",
                filled: true
            },
            "search": {
                path: "M1.4 5a3.6 3.6 0 1 0 7.2 0a3.6 3.6 0 1 0-7.2 0M7.7 7.7 11 11",
                filled: false
            },
            "scroll": {
                path: "M3 4.6 6 2.2l3 2.4M3 7.4 6 9.8l3-2.4",
                filled: false
            }
        })

    property string name
    property color color: Theme.text
    property real size: Theme.d(boxUnits)

    readonly property bool known: Object.prototype.hasOwnProperty.call(shapes, name)
    readonly property var shape: known ? shapes[name] : null
    readonly property real unit: size / boxUnits

    implicitWidth: size
    implicitHeight: size

    onKnownChanged: warnIfUnknown()
    Component.onCompleted: warnIfUnknown()

    function warnIfUnknown() {
        if (!known) {
            console.warn("Glyph: no glyph named '" + name + "'");
        }
    }

    Shape {
        anchors.fill: parent
        visible: root.known
        // The default renderer leaves diagonal strokes without antialiasing.
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            scale: Qt.size(root.unit, root.unit)
            fillColor: root.shape && root.shape.filled ? root.color : "transparent"
            strokeColor: root.shape && root.shape.filled ? "transparent" : root.color
            strokeWidth: root.strokeUnits * root.unit
            capStyle: ShapePath.SquareCap
            joinStyle: ShapePath.MiterJoin

            PathSvg {
                path: root.shape ? root.shape.path : ""
            }
        }
    }
}
