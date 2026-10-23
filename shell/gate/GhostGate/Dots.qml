import QtQuick
import "lib/theme.mjs" as ThemeLib

// The dot grid at rest, as the greeting leaves it: the shader's still mode, with the strengths GhostKit's defaults give.
ShaderEffect {
    // Design pixels, from the reference mockup's radial-gradient dots.
    readonly property real dotRadiusPx: 1
    readonly property real pixelRatio: Screen.devicePixelRatio
    // Far enough off the item that the halo's tail rounds to nothing.
    readonly property point offItem: Qt.point(-1e6, -1e6)

    property color dotColor: ThemeLib.withAlpha(Theme.dot, Theme.dotOpacity)
    // Whole physical pixels, so every dot rasterises the same way across the screen.
    property real spacing: Math.max(1, Math.round(Theme.d(Theme.dotSpacing) * pixelRatio))
    property real radius: Theme.d(dotRadiusPx) * pixelRatio
    property size resolution: Qt.size(width * pixelRatio, height * pixelRatio)
    property int mode: 0
    property real time: 0
    property real sizeRange: 2.5
    property real brightness: 1.6
    property real featureUnit: Theme.d(1) * pixelRatio
    property real cover: 0.4
    property point pointer: offItem

    fragmentShader: Qt.resolvedUrl("shaders/dotgrid.frag.qsb")
}
