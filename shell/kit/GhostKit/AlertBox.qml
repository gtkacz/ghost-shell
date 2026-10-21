import QtQuick
import Quickshell
import "lib/attention.mjs" as Attention
import "lib/theme.mjs" as ThemeLib

// A boxed alert: a coloured left edge, scrolling stripes, and text that flickers in on arrival and then
// blinks like the boot cursor. That lasts ATTENTION_MS from the first time the alert is shown, and then the text
// holds still at full colour while the stripes step once a second. Every step lands on a FrameClock tick or
// second, so it adds no frames of its own.
Item {
    id: root

    // Design pixels, from the reference mockup's .alert.
    readonly property real edgePx: 3
    readonly property real tilePx: 16
    readonly property real sidePaddingPx: 8
    readonly property real verticalPaddingPx: 3
    readonly property real gapPx: 8
    readonly property real textPx: 9.5
    readonly property real chipLinePx: 14
    readonly property real chipBorderPx: 1
    // Letter spacing in em.
    readonly property real textTracking: 0.08
    // The mockup's stripe strength, as a percentage like withAlpha() takes.
    readonly property real stripeAlphaPct: 13

    readonly property int stripePeriodMs: 1600
    readonly property int arrivalMs: 1600
    // Held steps: each opacity starts at its percentage of arrivalMs and holds until the next one.
    readonly property var arrivalStartPercents: [0, 6, 10, 14, 40, 70]
    readonly property var arrivalOpacities: [0.3, 1, 0.46, 1, 0.82, 0.64]

    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1

    property string text
    property string trailing
    // "warn" or "crit".
    property string level: "warn"
    // A Theme.chip() key; the chip is hidden when the key has no chip in the current Japanese mode, or when the
    // user turned alert chips off.
    property string chipKey
    // Whether the alert's own source is live, such as Data.topic(name).live.
    property bool sourceLive: true
    // Names the alert, so a new one blinks and the same one shown again does not. A new level or message counts as
    // a new alert; a caller whose message changes under one alert passes that alert's id instead.
    property string alertKey: level + ":" + text

    readonly property bool live: Data.connected && !Data.stale && sourceLive
    readonly property bool shown: visible && live
    // True from the first time the alert is shown until its attention window ends. A stale alert stands still:
    // the daemon is gone, so nothing it reported is current.
    property bool attracting: false
    readonly property bool animating: attracting
    readonly property bool truncated: messageText.truncated
    readonly property bool hasChip: chip.hasText
    readonly property bool hasTrailing: trailing !== ""
    readonly property color levelColor: !live ? Theme.dim : level === "crit" ? Theme.crit : Theme.warn
    readonly property real gap: Theme.d(gapPx)
    readonly property real textSize: Theme.d(textPx)

    property real appearedAt: 0
    property bool holdsClock: false
    property bool holdsSeconds: false

    readonly property real contentOpacity: {
        if (!holdsClock) {
            return 1;
        }
        const sinceArrival = FrameClock.elapsedMs - appearedAt;
        if (sinceArrival >= arrivalMs) {
            return FrameClock.blinkOpacity;
        }
        const reachedPercent = Theme.percent * sinceArrival / arrivalMs;
        let opacity = arrivalOpacities[0];
        for (let i = 0; i < arrivalStartPercents.length; i += 1) {
            if (reachedPercent >= arrivalStartPercents[i]) {
                opacity = arrivalOpacities[i];
            }
        }
        return opacity;
    }

    // The clock runs only while something holds it, so an alert holds it exactly while it can be seen
    // with live data and restarts its arrival whenever it reappears.
    function holdClock(wanted) {
        if (wanted === holdsClock) {
            return;
        }
        if (wanted) {
            appearedAt = FrameClock.elapsedMs;
            FrameClock.acquire();
        } else {
            FrameClock.release();
        }
        holdsClock = wanted;
    }

    // Past its window a shown alert needs the clock only at second boundaries, for its stripes' steps.
    function holdSeconds(wanted) {
        if (wanted === holdsSeconds) {
            return;
        }
        if (wanted) {
            FrameClock.acquireSeconds();
        } else {
            FrameClock.releaseSeconds();
        }
        holdsSeconds = wanted;
    }

    // Rounded up: a box at exactly its implicit width gives the message its measured width less a floating-point
    // error, which is enough for the text to elide its last characters.
    implicitWidth: Math.ceil(Theme.d(edgePx + 2 * sidePaddingPx) + (hasChip ? chip.width + gap : 0) + messageText.implicitWidth
        + (hasTrailing ? gap + trailingText.implicitWidth : 0))
    implicitHeight: Theme.d(chipLinePx + 2 * chipBorderPx + 2 * verticalPaddingPx)

    // Shown with live data, an alert blinks for what is left of its window, which another box showing the same
    // alert, or an earlier showing of this one, may already have used.
    function claimAttention() {
        const leftMs = shown ? AlertAttention.attentionLeftMs(alertKey) : 0;
        attracting = leftMs > 0;
        if (attracting) {
            attentionTimer.interval = Math.ceil(leftMs);
            attentionTimer.restart();
        } else {
            attentionTimer.stop();
        }
        holdSeconds(shown && !attracting);
    }

    onShownChanged: claimAttention()
    onAlertKeyChanged: {
        if (holdsClock) {
            appearedAt = FrameClock.elapsedMs;
        }
        claimAttention();
    }
    onAnimatingChanged: holdClock(animating)
    // The attention timer ends the window without a change of `shown`, so claimAttention() does not run.
    onAttractingChanged: holdSeconds(shown && !attracting)
    Component.onCompleted: claimAttention()
    Component.onDestruction: {
        holdClock(false);
        holdSeconds(false);
    }

    Timer {
        id: attentionTimer

        onTriggered: root.attracting = false
    }

    ShaderEffect {
        property color stripeColor: ThemeLib.withAlpha(root.levelColor, root.stripeAlphaPct)
        // Whole physical pixels, so every stripe has the same staircase edge.
        property real tile: Math.max(1, Math.round(Theme.d(root.tilePx) * root.pixelRatio))
        property real phase: root.holdsClock ? tile * (FrameClock.elapsedMs % root.stripePeriodMs) / root.stripePeriodMs
            : root.holdsSeconds ? Attention.heldStripePhase(FrameClock.epochSecond, tile, root.stripePeriodMs) : 0
        property size resolution: Qt.size(width * root.pixelRatio, height * root.pixelRatio)

        anchors.fill: parent
        fragmentShader: Qt.resolvedUrl("shaders/stripes.frag.qsb")
    }

    Rectangle {
        id: edge

        width: Theme.d(root.edgePx)
        height: parent.height
        color: root.levelColor
    }

    // The blink is a uniform of this alert's own layer. Stepping an item's opacity instead makes Qt
    // rebuild every batch in the window (measured: about 2 600 elements uploaded four times a second),
    // and a layer whose content blinks renders each step one frame late, adding frames.
    Item {
        anchors {
            fill: parent
            leftMargin: edge.width + Theme.d(root.sidePaddingPx)
            rightMargin: Theme.d(root.sidePaddingPx)
        }
        layer.enabled: true
        layer.effect: ShaderEffect {
            property real level: root.contentOpacity

            fragmentShader: Qt.resolvedUrl("shaders/blink.frag.qsb")
        }

        Chip {
            id: chip

            anchors.verticalCenter: parent.verticalCenter
            // An empty key is how a Chip reads as absent, so hasChip, the text's anchor and implicitWidth follow.
            key: Theme.alertChips ? root.chipKey : ""
            color: root.levelColor
            borderColor: root.levelColor
        }

        Text {
            id: messageText

            anchors {
                left: root.hasChip ? chip.right : parent.left
                leftMargin: root.hasChip ? root.gap : 0
                right: root.hasTrailing ? trailingText.left : parent.right
                rightMargin: root.hasTrailing ? root.gap : 0
                verticalCenter: parent.verticalCenter
            }
            text: root.text
            // Callers pass provider and device strings, which AutoText would render as markup.
            textFormat: Text.PlainText
            color: root.levelColor
            elide: Text.ElideRight
            font {
                family: Theme.font
                pixelSize: root.textSize
                weight: Font.DemiBold
                letterSpacing: root.textSize * root.textTracking
            }
        }

        Digits {
            id: trailingText

            anchors {
                right: parent.right
                verticalCenter: parent.verticalCenter
            }
            visible: root.hasTrailing
            text: root.trailing
            pixelSize: root.textSize
            weight: Font.DemiBold
            color: root.levelColor
        }
    }
}
