import QtQuick
import "lib/gate.mjs" as GateLib
import "lib/theme.mjs" as ThemeLib

// The boxed crit alert of GhostKit's AlertBox, with its timing taken from gate.mjs and driven by one stepped Timer that
// stops once the blink has ended; the stripes move only with those steps.
Item {
    id: root

    // Design pixels, from the reference mockup's .alert.
    readonly property real edgePx: 3
    readonly property real tilePx: 16
    readonly property real sidePaddingPx: 10
    readonly property real verticalPaddingPx: 5
    readonly property real textPx: 10
    // Letter spacing in em.
    readonly property real textTracking: 0.12
    // The mockup's stripe strength, as a percentage like withAlpha() takes.
    readonly property real stripeAlphaPct: 16
    readonly property real pixelRatio: Screen.devicePixelRatio

    property string text
    property bool shown: false

    property real level: 1
    property real phase: 0
    property real shownAt: 0

    visible: shown
    implicitHeight: label.implicitHeight + Theme.d(2 * verticalPaddingPx)

    function restart() {
        shownAt = Date.now();
        advance();
    }

    function advance() {
        const elapsed = Date.now() - shownAt;
        level = GateLib.alertOpacity(elapsed);
        phase = tile * (elapsed % Theme.alertStripePeriodMs) / Theme.alertStripePeriodMs;
        const delay = GateLib.alertDelay(elapsed);
        if (delay > 0) {
            steps.interval = delay;
            steps.restart();
        } else {
            steps.stop();
        }
    }

    // Whole physical pixels, so every stripe has the same staircase edge.
    readonly property real tile: Math.max(1, Math.round(Theme.d(tilePx) * pixelRatio))

    onShownChanged: {
        if (shown) {
            restart();
        } else {
            steps.stop();
        }
    }

    Timer {
        id: steps

        onTriggered: root.advance()
    }

    ShaderEffect {
        property color stripeColor: ThemeLib.withAlpha(Theme.crit, root.stripeAlphaPct)
        property real tile: root.tile
        property real phase: root.phase
        property size resolution: Qt.size(width * root.pixelRatio, height * root.pixelRatio)

        anchors.fill: parent
        fragmentShader: Qt.resolvedUrl("shaders/stripes.frag.qsb")
    }

    Rectangle {
        id: edge

        width: Theme.d(root.edgePx)
        height: parent.height
        color: Theme.crit
    }

    // A layer with the blink shader rather than a stepped opacity, which would rebuild every batch in the window.
    Item {
        anchors {
            fill: parent
            leftMargin: edge.width + Theme.d(root.sidePaddingPx)
            rightMargin: Theme.d(root.sidePaddingPx)
        }
        layer.enabled: steps.running
        layer.effect: ShaderEffect {
            property real level: root.level

            fragmentShader: Qt.resolvedUrl("shaders/blink.frag.qsb")
        }

        Text {
            id: label

            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            text: root.text
            textFormat: Text.PlainText
            color: Theme.crit
            elide: Text.ElideRight
            font {
                family: Theme.font
                pixelSize: Theme.d(root.textPx)
                weight: Font.DemiBold
                letterSpacing: Theme.d(root.textPx) * root.textTracking
            }
        }
    }
}
