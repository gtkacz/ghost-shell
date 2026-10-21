import QtQuick
import Quickshell

ShaderEffect {
    // Design pixels: a dark line this thick, once per period.
    readonly property real periodPx: 3
    readonly property real lineWidthPx: 1
    readonly property int percent: 100
    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1

    property real strength: Theme.scanlineOpacity / percent
    // Whole physical pixels, so every line is equally thick.
    property real period: Math.max(1, Math.round(Theme.d(periodPx) * pixelRatio))
    property real lineWidth: Math.max(1, Math.round(Theme.d(lineWidthPx) * pixelRatio))
    property size resolution: Qt.size(width * pixelRatio, height * pixelRatio)

    fragmentShader: Qt.resolvedUrl("shaders/scanlines.frag.qsb")
}
