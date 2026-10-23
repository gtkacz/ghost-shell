pragma Singleton

import QtQuick
import "lib/gate.mjs" as GateLib
import "lib/scale.mjs" as ScaleLib
import "lib/theme.mjs" as ThemeLib

// What GhostKit's Theme gives its widgets, without the daemon: the lock and login screens are plain Qt processes that
// read a snapshot file instead, which load() applies.
QtObject {
    id: root

    readonly property int percent: 100
    readonly property var defaults: GateLib.parseSnapshot({})
    property var appearance: defaults.appearance

    // Logical pixels per design pixel; the screen sets it with fit().
    property real u: 1

    property string style: defaults.style
    property string host: defaults.host
    property string displayModel: defaults.displayModel

    readonly property int blinkStepMs: GateLib.BLINK_STEP_MS
    readonly property var blinkSteps: GateLib.BLINK_STEPS

    readonly property color ground: appearance.colors.ground
    readonly property color panel: appearance.colors.panel
    readonly property color hair: appearance.colors.hair
    readonly property color hair2: appearance.colors.hair2
    readonly property color text: appearance.colors.text
    readonly property color hi: appearance.colors.hi
    readonly property color dim: appearance.colors.dim
    readonly property color signal: appearance.colors.signal
    readonly property color warn: appearance.colors.warn
    readonly property color crit: appearance.colors.crit
    readonly property color dot: appearance.colors.dot
    readonly property color burn: appearance.colors.burn

    readonly property real panelFill: appearance.panel_fill
    readonly property real dotOpacity: appearance.dots.opacity
    readonly property real dotSpacing: appearance.dots.spacing
    readonly property real dotSizeRange: appearance.dots.size_range
    readonly property real dotBrightness: appearance.dots.brightness
    readonly property real dotScale: appearance.dots.scale
    readonly property real dotCover: appearance.dots.cover
    readonly property real scanlineOpacity: appearance.scanlines.opacity

    readonly property string font: appearance.font
    readonly property string fontCjk: appearance.font_cjk
    readonly property string japanese: appearance.japanese
    readonly property int alertStripePeriodMs: ThemeLib.secondsToMs(appearance.alert_stripe_period_s)
    readonly property int alertStripeStepMs: ThemeLib.secondsToMs(appearance.alert_stripe_step_s)

    function load(snapshot) {
        appearance = snapshot.appearance;
        style = snapshot.style;
        host = snapshot.host;
        displayModel = snapshot.displayModel;
    }

    function fit(logicalWidth) {
        u = ScaleLib.designUnit(logicalWidth);
    }

    function d(px) {
        return px * u;
    }

    function tag(key) {
        return ThemeLib.tag(key, japanese);
    }

    function chip(key) {
        return ThemeLib.chip(key, japanese);
    }
}
