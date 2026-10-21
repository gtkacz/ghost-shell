pragma Singleton

import QtQuick
import Quickshell
import "lib/theme.mjs" as ThemeLib

Singleton {
    id: root

    readonly property int percent: 100

    // Mirrors the daemon's ResolvedConfig defaults, which apply until the first config arrives.
    readonly property var defaultAppearance: ({
            font: "Kode Mono",
            font_cjk: "Noto Sans CJK JP",
            japanese: "mixed",
            alert_chips: true,
            panel_fill: 55,
            colors: {
                ground: "#0a0f11",
                panel: "#0a0f11",
                hair: "#2a3a3d",
                hair2: "#1a2527",
                text: "#b7d1cd",
                hi: "#e4f1ef",
                dim: "#587170",
                signal: "#a3d4cc",
                warn: "#e9a84c",
                crit: "#ff3b45",
                dot: "#a3d4cc"
            },
            dots: {
                spacing: 24,
                opacity: 16
            },
            scanlines: {
                opacity: 15
            }
        })
    readonly property var defaultThresholds: ({
            load_warn: 75,
            load_crit: 90,
            temp_warn: 80,
            temp_crit: 90,
            battery_warn: 20,
            battery_crit: 10,
            aqi_warn: 100,
            aqi_crit: 150
        })

    readonly property var appearance: Data.config ? Data.config.appearance : defaultAppearance
    readonly property var thresholds: Data.config ? Data.config.thresholds : defaultThresholds

    // Logical pixels per design pixel, set by the desktop from its screen width. Read-only here so that
    // one widget cannot rescale the whole desktop; the host writes it through hostControl.
    readonly property real u: hostControl.u
    readonly property QtObject hostControl: QtObject {
        property real u: 1
    }

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

    // Percentages, as in the config and as withAlpha() takes them.
    readonly property real panelFill: appearance.panel_fill
    readonly property real dotOpacity: appearance.dots.opacity
    readonly property real scanlineOpacity: appearance.scanlines.opacity

    // Design pixels.
    readonly property real dotSpacing: appearance.dots.spacing

    readonly property string font: appearance.font
    readonly property string fontCjk: appearance.font_cjk
    readonly property string japanese: appearance.japanese
    // Absent from the config of a daemon older than this key, which must not read as "off".
    readonly property bool alertChips: appearance.alert_chips !== false

    readonly property real loadWarn: thresholds.load_warn / percent
    readonly property real loadCrit: thresholds.load_crit / percent
    readonly property real tempWarn: thresholds.temp_warn
    readonly property real tempCrit: thresholds.temp_crit
    readonly property real batteryWarn: thresholds.battery_warn
    readonly property real batteryCrit: thresholds.battery_crit
    readonly property real aqiWarn: thresholds.aqi_warn
    readonly property real aqiCrit: thresholds.aqi_crit

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
