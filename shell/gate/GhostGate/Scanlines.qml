import QtQuick

ShaderEffect {
    // Design pixels: a dark line this thick, once per period.
    readonly property real periodPx: 3
    readonly property real lineWidthPx: 1
    readonly property real pixelRatio: Screen.devicePixelRatio

    property real strength: Theme.scanlineOpacity / Theme.percent
    // Whole physical pixels, so every line is equally thick.
    property real period: Math.max(1, Math.round(Theme.d(periodPx) * pixelRatio))
    property real lineWidth: Math.max(1, Math.round(Theme.d(lineWidthPx) * pixelRatio))
    property size resolution: Qt.size(width * pixelRatio, height * pixelRatio)

    fragmentShader: Qt.resolvedUrl("shaders/scanlines.frag.qsb")
}
