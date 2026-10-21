import QtQuick
import Quickshell
import "lib/theme.mjs" as ThemeLib

ShaderEffect {
    // Design pixels, from the reference mockup's radial-gradient dots.
    readonly property real dotRadiusPx: 1
    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1

    property color dotColor: ThemeLib.withAlpha(Theme.dot, Theme.dotOpacity)
    // Whole physical pixels, so every dot rasterises the same way across the screen.
    property real spacing: Math.max(1, Math.round(Theme.d(Theme.dotSpacing) * pixelRatio))
    property real radius: Theme.d(dotRadiusPx) * pixelRatio
    property size resolution: Qt.size(width * pixelRatio, height * pixelRatio)

    fragmentShader: Qt.resolvedUrl("shaders/dotgrid.frag.qsb")
}
