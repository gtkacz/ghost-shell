import QtQuick
import QtQuick.Shapes
import "lib/theme.mjs" as ThemeLib

Item {
    id: root

    // Design pixels, from the reference mockup's panel frame and header.
    readonly property real chamferPx: 12
    readonly property real edgeWidthPx: 1
    readonly property real bracketWidthPx: 1.5
    readonly property real bracketArmPx: 14
    // The mockup centres the bracket stroke 1 px in, so it covers the edge and shows just inside it.
    readonly property real bracketInsetPx: 1
    readonly property real headerHeightPx: 26
    readonly property real headerTopPx: 9
    readonly property real headerBottomPx: 6
    readonly property real headerSidePx: 12
    readonly property real headerGapPx: 8
    readonly property real squarePx: 5
    readonly property real headerTextPx: 10
    readonly property real bodyTopPx: 4
    readonly property real paddingPx: 12

    // Letter spacing in em.
    readonly property real labelTracking: 0.16
    readonly property real tagTracking: 0.06
    readonly property real metaTracking: 0.1
    readonly property real tagOpacity: 0.85
    // The mockup's writeLabels(): each label scrambles for between 500 and 800 ms.
    readonly property real labelScrambleMinMs: 500
    readonly property real labelScrambleSpanMs: 300

    // The mockup's headerless panels (the time bands) and frameless ones (the time hero).
    property bool header: true
    property bool framed: true
    property string label
    property string tagKey
    property string meta
    property real padding: Theme.d(paddingPx)
    default property alias content: body.data

    readonly property real chamfer: Theme.d(chamferPx)
    readonly property real edgeInset: Theme.d(edgeWidthPx) / 2
    readonly property real bracketInset: Theme.d(bracketInsetPx)
    readonly property real bracketArm: Theme.d(bracketArmPx)
    readonly property real headerTextSize: Theme.d(headerTextPx)
    readonly property real headerGap: Theme.d(headerGapPx)

    Shape {
        anchors.fill: parent
        visible: root.framed
        // The default renderer leaves the 45° cuts and thin strokes without antialiasing.
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: ThemeLib.withAlpha(Theme.panel, Theme.panelFill)
            strokeColor: "transparent"
            startX: 0
            startY: 0

            PathLine {
                x: root.width - root.chamfer
                y: 0
            }
            PathLine {
                x: root.width
                y: root.chamfer
            }
            PathLine {
                x: root.width
                y: root.height
            }
            PathLine {
                x: root.chamfer
                y: root.height
            }
            PathLine {
                x: 0
                y: root.height - root.chamfer
            }
            PathLine {
                x: 0
                y: 0
            }
        }

        ShapePath {
            fillColor: "transparent"
            strokeColor: Theme.hair
            strokeWidth: Theme.d(root.edgeWidthPx)
            joinStyle: ShapePath.MiterJoin
            startX: root.edgeInset
            startY: root.edgeInset

            PathLine {
                x: root.width - root.chamfer
                y: root.edgeInset
            }
            PathLine {
                x: root.width - root.edgeInset
                y: root.chamfer
            }
            PathLine {
                x: root.width - root.edgeInset
                y: root.height - root.edgeInset
            }
            PathLine {
                x: root.chamfer
                y: root.height - root.edgeInset
            }
            PathLine {
                x: root.edgeInset
                y: root.height - root.chamfer
            }
            PathLine {
                x: root.edgeInset
                y: root.edgeInset
            }
        }

        ShapePath {
            fillColor: "transparent"
            strokeColor: Theme.signal
            strokeWidth: Theme.d(root.bracketWidthPx)
            joinStyle: ShapePath.MiterJoin
            capStyle: ShapePath.FlatCap
            startX: root.bracketInset
            startY: root.bracketArm

            PathLine {
                x: root.bracketInset
                y: root.bracketInset
            }
            PathLine {
                x: root.bracketArm
                y: root.bracketInset
            }
            PathMove {
                x: root.width - root.bracketArm
                y: root.height - root.bracketInset
            }
            PathLine {
                x: root.width - root.bracketInset
                y: root.height - root.bracketInset
            }
            PathLine {
                x: root.width - root.bracketInset
                y: root.height - root.bracketArm
            }
        }
    }

    Item {
        id: headerItem

        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
        }
        visible: root.header
        height: root.header ? Theme.d(root.headerHeightPx) : 0

        Item {
            id: headerLine

            anchors {
                fill: parent
                topMargin: Theme.d(root.headerTopPx)
                bottomMargin: Theme.d(root.headerBottomPx)
                leftMargin: Theme.d(root.headerSidePx)
                rightMargin: Theme.d(root.headerSidePx)
            }
        }

        Rectangle {
            id: square

            anchors {
                left: headerLine.left
                verticalCenter: headerLine.verticalCenter
            }
            width: Theme.d(root.squarePx)
            height: width
            color: Theme.signal
        }

        ScrambleText {
            id: labelText

            anchors {
                left: square.right
                leftMargin: root.headerGap
                verticalCenter: headerLine.verticalCenter
            }
            width: Math.max(0, Math.min(implicitWidth, headerLine.x + headerLine.width - x))
            text: root.label
            // Widgets set the label and meta from their own data, which AutoText would render as markup.
            textFormat: Text.PlainText
            scrambleKey: "panel:" + root.label
            durationMs: root.labelScrambleMinMs + Math.random() * root.labelScrambleSpanMs
            color: Theme.hi
            elide: Text.ElideRight
            font {
                family: Theme.font
                pixelSize: root.headerTextSize
                weight: Font.DemiBold
                letterSpacing: root.headerTextSize * root.labelTracking
                capitalization: Font.AllUppercase
            }
        }

        Text {
            id: tagText

            anchors {
                left: labelText.right
                leftMargin: text === "" ? 0 : root.headerGap
                verticalCenter: headerLine.verticalCenter
            }
            width: Math.max(0, Math.min(implicitWidth, headerLine.x + headerLine.width - x))
            text: Theme.tag(root.tagKey)
            color: Theme.signal
            opacity: root.tagOpacity
            elide: Text.ElideRight
            font {
                family: Theme.fontCjk
                pixelSize: root.headerTextSize
                weight: Font.Medium
                letterSpacing: root.headerTextSize * root.tagTracking
            }
        }

        Text {
            anchors {
                left: tagText.right
                leftMargin: root.headerGap
                right: headerLine.right
                verticalCenter: headerLine.verticalCenter
            }
            text: root.meta
            textFormat: Text.PlainText
            color: Theme.dim
            horizontalAlignment: Text.AlignRight
            elide: Text.ElideRight
            font {
                family: Theme.font
                pixelSize: root.headerTextSize
                weight: Font.Medium
                letterSpacing: root.headerTextSize * root.metaTracking
                capitalization: Font.AllUppercase
            }
        }
    }

    Item {
        id: body

        anchors {
            top: headerItem.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            margins: root.padding
            topMargin: root.header ? Theme.d(root.bodyTopPx) : root.padding
        }
    }
}
