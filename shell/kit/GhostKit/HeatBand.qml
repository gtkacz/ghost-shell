import QtQuick
import Quickshell
import "lib/heatgraph.mjs" as Packing
import "lib/segments.mjs" as Segments

// HEAT_BAND_ROWS rows of a HeatGraph, which lays the bands out and hands each the columns' geometry. The cells sit in
// a ring (lib/heatgraph.mjs), so a new sample rewrites the one word of its column and moves the ring's head on, rather
// than repacking and re-sending every word.
ShaderEffect {
    id: root

    // One series per row of the band.
    property var series: []
    // HeatGraph's lo, hi, warnAt, critAt and live.
    property var graph
    // Whole numbers, but real: ShaderEffect copies an int property's bits into the shader's float as they are.
    property real columnCount
    property real columnWidth
    property real gapSize
    property real rowHeight
    property real firstColumnX

    readonly property real rowCount: series.length
    readonly property real drawnColumns: Packing.heatDrawnColumns(columnCount)
    readonly property real skippedColumns: columnCount - drawnColumns
    readonly property size itemSize: Qt.size(width, height)
    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1

    // A missing cell, so segments.mjs stays the one place that defines how unlit cells look.
    readonly property color unlitColor: Segments.heatCellColor(NaN, "ok", Theme)
    // The ramp's alpha travels in the cell bytes, so the shader restates no colour or alpha either.
    readonly property color rampColor: Qt.rgba(Theme.signal.r, Theme.signal.g, Theme.signal.b, 1)
    readonly property color warnColor: Theme.warn
    readonly property color critColor: Theme.crit

    // Written by draw() rather than bound, so that a new sample sends only the word that changed.
    property var ring: null
    property real head: 0
    property bool completed: false

    function send(index) {
        const components = ring.words[index];
        root["cellWord" + index] = Qt.vector4d(components[0], components[1], components[2], components[3]);
    }

    function draw() {
        if (!completed) {
            return;
        }
        const changed = ring === null ? null : Packing.heatRingAdvance(ring, series, columnCount, graph);
        if (changed === null) {
            ring = Packing.heatRing(series, columnCount, graph);
            for (let i = 0; i < Packing.HEAT_WORDS; i += 1) {
                send(i);
            }
        } else {
            for (const index of changed) {
                send(index);
            }
        }
        head = ring.head;
    }

    onSeriesChanged: draw()
    onGraphChanged: draw()
    onColumnCountChanged: draw()
    Component.onCompleted: {
        completed = true;
        draw();
    }

    // ShaderEffect maps properties to uniforms by name and ignores arrays, hence one property per word.
    property vector4d cellWord0
    property vector4d cellWord1
    property vector4d cellWord2
    property vector4d cellWord3
    property vector4d cellWord4
    property vector4d cellWord5
    property vector4d cellWord6
    property vector4d cellWord7
    property vector4d cellWord8
    property vector4d cellWord9
    property vector4d cellWord10
    property vector4d cellWord11
    property vector4d cellWord12
    property vector4d cellWord13
    property vector4d cellWord14
    property vector4d cellWord15
    property vector4d cellWord16
    property vector4d cellWord17
    property vector4d cellWord18
    property vector4d cellWord19
    property vector4d cellWord20
    property vector4d cellWord21
    property vector4d cellWord22
    property vector4d cellWord23
    property vector4d cellWord24
    property vector4d cellWord25
    property vector4d cellWord26
    property vector4d cellWord27
    property vector4d cellWord28
    property vector4d cellWord29
    property vector4d cellWord30
    property vector4d cellWord31
    property vector4d cellWord32
    property vector4d cellWord33
    property vector4d cellWord34
    property vector4d cellWord35
    property vector4d cellWord36
    property vector4d cellWord37
    property vector4d cellWord38
    property vector4d cellWord39
    property vector4d cellWord40
    property vector4d cellWord41
    property vector4d cellWord42
    property vector4d cellWord43
    property vector4d cellWord44
    property vector4d cellWord45
    property vector4d cellWord46
    property vector4d cellWord47
    property vector4d cellWord48
    property vector4d cellWord49
    property vector4d cellWord50
    property vector4d cellWord51
    property vector4d cellWord52
    property vector4d cellWord53
    property vector4d cellWord54
    property vector4d cellWord55
    property vector4d cellWord56
    property vector4d cellWord57
    property vector4d cellWord58
    property vector4d cellWord59
    property vector4d cellWord60
    property vector4d cellWord61
    property vector4d cellWord62
    property vector4d cellWord63
    property vector4d cellWord64
    property vector4d cellWord65
    property vector4d cellWord66
    property vector4d cellWord67
    property vector4d cellWord68
    property vector4d cellWord69
    property vector4d cellWord70
    property vector4d cellWord71
    property vector4d cellWord72
    property vector4d cellWord73
    property vector4d cellWord74
    property vector4d cellWord75
    property vector4d cellWord76
    property vector4d cellWord77
    property vector4d cellWord78
    property vector4d cellWord79
    property vector4d cellWord80
    property vector4d cellWord81
    property vector4d cellWord82
    property vector4d cellWord83
    property vector4d cellWord84
    property vector4d cellWord85
    property vector4d cellWord86
    property vector4d cellWord87
    property vector4d cellWord88
    property vector4d cellWord89
    property vector4d cellWord90
    property vector4d cellWord91
    property vector4d cellWord92
    property vector4d cellWord93
    property vector4d cellWord94
    property vector4d cellWord95
    property vector4d cellWord96
    property vector4d cellWord97
    property vector4d cellWord98
    property vector4d cellWord99
    property vector4d cellWord100
    property vector4d cellWord101
    property vector4d cellWord102
    property vector4d cellWord103
    property vector4d cellWord104
    property vector4d cellWord105
    property vector4d cellWord106
    property vector4d cellWord107
    property vector4d cellWord108
    property vector4d cellWord109
    property vector4d cellWord110
    property vector4d cellWord111
    property vector4d cellWord112
    property vector4d cellWord113
    property vector4d cellWord114
    property vector4d cellWord115
    property vector4d cellWord116
    property vector4d cellWord117
    property vector4d cellWord118
    property vector4d cellWord119
    property vector4d cellWord120
    property vector4d cellWord121
    property vector4d cellWord122
    property vector4d cellWord123
    property vector4d cellWord124
    property vector4d cellWord125
    property vector4d cellWord126
    property vector4d cellWord127

    fragmentShader: Qt.resolvedUrl("shaders/heatband.frag.qsb")
}
