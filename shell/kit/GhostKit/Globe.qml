import QtQuick
import Quickshell
import "lib/globe.mjs" as GlobeLib
import "lib/landbits.mjs" as LandBits

// The uplink's dot globe, as the reference mockup's drawGlobe() draws it, less the labels. One shader draws the
// outline, the dots and the arcs with their packets, end squares and home pulse: a Shape overlay cost three times the
// CPU and rendered two frames per tick (docs/superpowers/notes/2026-10-21-globe-shader-spike.md). The dots' land bits
// are set once and the endpoints only when they change, so a tick changes nothing but the time. The labels are the
// owner's Texts, placed at labelAnchors, which lib/globe.mjs projects as the shader does.
Item {
    id: root

    property real elapsedMs: 0
    property real unit: Theme.u
    property bool hasHome: false
    property real homeLat: 0
    property real homeLon: 0
    // [{ lat, lon }]; only the first GlobeLib.MAX_ENDPOINTS are drawn.
    property var endpoints: []
    property color signalColor: Theme.signal
    property color hiColor: Theme.hi

    readonly property var labelAnchors: GlobeLib.labelAnchors({
        width,
        height,
        unit,
        elapsedMs,
        hasHome,
        home: {
            lat: homeLat,
            lon: homeLon
        },
        endpoints
    })

    readonly property var frame: GlobeLib.globeLayout({
        width,
        height,
        unit
    })
    readonly property var endpointVectors: (endpoints || []).slice(0, GlobeLib.MAX_ENDPOINTS).map(place => GlobeLib.unitVector(place))
    readonly property real presentW: 1

    function presentVector(v) {
        return Qt.vector4d(v[0], v[1], v[2], presentW);
    }

    function endpointWord(k) {
        const v = endpointVectors[k];
        return v ? presentVector(v) : Qt.vector4d(0, 0, 0, 0);
    }

    function landWord(index) {
        const components = LandBits.LAND_WORDS[index];
        return Qt.vector4d(components[0], components[1], components[2], components[3]);
    }

    ShaderEffect {
        anchors.fill: parent

        readonly property size itemSize: Qt.size(width, height)
        // Bound rather than read once: the surface reports a provisional ratio before it is configured.
        readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1
        readonly property real unit: root.unit
        readonly property real radius: root.frame.radius
        readonly property real dotSize: root.frame.dot
        readonly property real rot: GlobeLib.rotation(root.elapsedMs)
        readonly property real tilt: GlobeLib.TILT
        // Real, not int: ShaderEffect copies an int's bits into a float uniform, which reads as almost zero.
        readonly property real pointCount: GlobeLib.LATTICE_POINTS
        readonly property real elapsedMs: root.elapsedMs
        readonly property real arcWidth: root.frame.arcWidth
        readonly property color signalColor: root.signalColor
        readonly property color hiColor: root.hiColor
        readonly property real landAlphaFloor: GlobeLib.LAND_ALPHA_FLOOR
        readonly property real landAlphaSpan: GlobeLib.LAND_ALPHA_SPAN
        readonly property real seaAlphaSpan: GlobeLib.SEA_ALPHA_SPAN
        readonly property real ringAlpha: GlobeLib.RING_ALPHA
        readonly property real arcAlpha: GlobeLib.ARC_ALPHA

        // ShaderEffect maps properties to uniforms by name and ignores arrays, hence one property per vector.
        readonly property vector4d home: root.hasHome ? root.presentVector(GlobeLib.unitVector({
            lat: root.homeLat,
            lon: root.homeLon
        })) : Qt.vector4d(0, 0, 0, 0)
        readonly property vector4d endpoint0: root.endpointWord(0)
        readonly property vector4d endpoint1: root.endpointWord(1)
        readonly property vector4d endpoint2: root.endpointWord(2)
        readonly property vector4d endpoint3: root.endpointWord(3)
        readonly property vector4d endpoint4: root.endpointWord(4)
        readonly property vector4d endpoint5: root.endpointWord(5)
        readonly property vector4d endpoint6: root.endpointWord(6)
        readonly property vector4d endpoint7: root.endpointWord(7)
        readonly property vector4d landWord0: root.landWord(0)
        readonly property vector4d landWord1: root.landWord(1)
        readonly property vector4d landWord2: root.landWord(2)
        readonly property vector4d landWord3: root.landWord(3)
        readonly property vector4d landWord4: root.landWord(4)
        readonly property vector4d landWord5: root.landWord(5)
        readonly property vector4d landWord6: root.landWord(6)
        readonly property vector4d landWord7: root.landWord(7)
        readonly property vector4d landWord8: root.landWord(8)
        readonly property vector4d landWord9: root.landWord(9)
        readonly property vector4d landWord10: root.landWord(10)
        readonly property vector4d landWord11: root.landWord(11)
        readonly property vector4d landWord12: root.landWord(12)
        readonly property vector4d landWord13: root.landWord(13)
        readonly property vector4d landWord14: root.landWord(14)
        readonly property vector4d landWord15: root.landWord(15)
        readonly property vector4d landWord16: root.landWord(16)
        readonly property vector4d landWord17: root.landWord(17)
        readonly property vector4d landWord18: root.landWord(18)
        readonly property vector4d landWord19: root.landWord(19)
        readonly property vector4d landWord20: root.landWord(20)
        readonly property vector4d landWord21: root.landWord(21)
        readonly property vector4d landWord22: root.landWord(22)
        readonly property vector4d landWord23: root.landWord(23)
        readonly property vector4d landWord24: root.landWord(24)
        readonly property vector4d landWord25: root.landWord(25)
        readonly property vector4d landWord26: root.landWord(26)
        readonly property vector4d landWord27: root.landWord(27)
        readonly property vector4d landWord28: root.landWord(28)
        readonly property vector4d landWord29: root.landWord(29)
        readonly property vector4d landWord30: root.landWord(30)
        readonly property vector4d landWord31: root.landWord(31)
        readonly property vector4d landWord32: root.landWord(32)
        readonly property vector4d landWord33: root.landWord(33)

        fragmentShader: Qt.resolvedUrl("shaders/globe.frag.qsb")
    }
}
