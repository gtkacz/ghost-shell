import QtQuick
import Quickshell
import Quickshell.Services.Pipewire
import GhostKit
import "audio.mjs" as Audio

// The default output and input as rows: OUT with its device, volume and mute, and IN with its device and mute, which
// shows the input's level and LIVE while another app records. The level is read only then, and only while the rows can
// be seen, so the shell opens no capture stream on the microphone at any other time (decision 2). Hidden, the rows
// still tell the MIC LIVE alert which apps record, which needs no capture stream at all.
Column {
    id: root

    // Design pixels, from the reference mockup's .rows and .row. The mockup has no audio rows, so the meter's length is
    // this widget's own: twelve cells.
    readonly property real rowGapPx: 5
    readonly property real labelWidthPx: 38
    readonly property real gapPx: 8
    readonly property real textPx: 10
    readonly property real meterPx: 70
    // The widest value either row shows, so both meters keep one place while the values change.
    readonly property string widestValue: "MUTED"

    readonly property PwNode sink: Pipewire.defaultAudioSink
    readonly property PwNode source: Pipewire.defaultAudioSource
    // Quickshell reads a node's properties only while it is bound, and the rows bind only other apps' capture streams.
    readonly property var captureStreams: Pipewire.nodes.values.filter(node => Audio.needsBinding(plainNode(node)))
    readonly property var apps: Audio.recordingApps(captureStreams.map(node => plainNode(node)), [Quickshell.processId])
    readonly property bool recording: apps.length > 0
    readonly property string alertText: Audio.micAlertText(apps)

    readonly property real outVolume: hasAudio(sink) ? sink.audio.volume : NaN
    readonly property bool outMuted: hasAudio(sink) && sink.audio.muted
    readonly property bool inMuted: hasAudio(source) && source.audio.muted
    readonly property string controlVolume: Audio.controlVolume(outVolume, outMuted)
    readonly property bool levelEnabled: Audio.micLevelEnabled(apps, visible, inMuted)
    // The input's peak, 0..1, copied on frame ticks: the monitor reports every PipeWire period, far more often than the
    // rows redraw.
    property real level: 0
    property bool holdsClock: false

    function plainNode(node) {
        return {
            isStream: node.isStream,
            isSink: node.isSink,
            isAudio: node.audio !== null,
            name: node.name,
            props: node.properties
        };
    }

    function hasAudio(node) {
        return node !== null && node.audio !== null;
    }

    // A device's mute is written only once it is bound, when Quickshell knows its state.
    function canToggle(node) {
        return hasAudio(node) && node.ready;
    }

    function toggleMute(node) {
        node.audio.muted = !node.audio.muted;
    }

    function deviceName(node) {
        return node !== null ? node.description || node.nickname || node.name : "--";
    }

    // A hold of its own rather than a share of the spectrum's: FrameClock counts its holders and runs one timer, so a
    // second hold adds no frames, and the level never has to take over a hold the spectrum is about to drop.
    function holdClock(wanted) {
        if (wanted === holdsClock) {
            return;
        }
        if (wanted) {
            FrameClock.acquire();
        } else {
            FrameClock.release();
        }
        holdsClock = wanted;
    }

    component DeviceRow: Item {
        id: row

        property string label
        property var node: null
        property bool metered: true
        property var fraction
        property string value
        property color valueColor

        signal toggled

        implicitHeight: labelText.implicitHeight

        Text {
            id: labelText

            width: Theme.d(root.labelWidthPx)
            anchors.verticalCenter: parent.verticalCenter
            text: row.label
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(root.textPx)
                weight: Font.Medium
            }
        }

        Text {
            anchors {
                left: labelText.right
                leftMargin: Theme.d(root.gapPx)
                right: meter.left
                rightMargin: Theme.d(root.gapPx)
                verticalCenter: parent.verticalCenter
            }
            text: root.deviceName(row.node)
            // PipeWire's device names come from the hardware and any app, which AutoText would render as markup.
            textFormat: Text.PlainText
            color: row.node !== null ? Theme.text : Theme.dim
            elide: Text.ElideRight
            font {
                family: Theme.font
                pixelSize: Theme.d(root.textPx)
                weight: Font.Medium
            }
        }

        // A volume or a level is not an alarm, so the meter stays signal all the way.
        SegMeter {
            id: meter

            anchors {
                right: valueCell.left
                rightMargin: Theme.d(root.gapPx)
                verticalCenter: parent.verticalCenter
            }
            width: Theme.d(root.meterPx)
            visible: row.metered
            value: row.fraction
            warnAt: Infinity
            critAt: Infinity
        }

        Item {
            id: valueCell

            anchors {
                right: parent.right
                top: parent.top
                bottom: parent.bottom
            }
            width: valueSpan.advanceWidth

            Digits {
                anchors {
                    right: parent.right
                    verticalCenter: parent.verticalCenter
                }
                text: row.value
                pixelSize: Theme.d(root.textPx)
                weight: Font.Medium
                color: row.valueColor
            }
        }

        // The whole row toggles the mute: the value alone is a few characters wide, and a click beside it would pass
        // through to the desktop.
        ClickArea {
            anchors.fill: parent
            enabled: root.canToggle(row.node)
            onClicked: row.toggled()
        }
    }

    spacing: Theme.d(rowGapPx)

    onLevelEnabledChanged: {
        holdClock(levelEnabled);
        if (!levelEnabled) {
            level = 0;
        }
    }
    Component.onCompleted: holdClock(levelEnabled)
    Component.onDestruction: holdClock(false)

    TextMetrics {
        id: valueSpan

        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
            weight: Font.Medium
        }
        text: root.widestValue
    }

    PwObjectTracker {
        objects: [root.sink, root.source].filter(node => node !== null).concat(root.captureStreams)
    }

    // Quickshell's monitor starts out enabled and opens its stream the moment it has a node, before an `enabled`
    // binding lands, so it is given the node only while the level is read; otherwise every new view would open the
    // microphone for an instant.
    PwNodePeakMonitor {
        id: peaks

        node: root.levelEnabled ? root.source : null
        enabled: root.levelEnabled
    }

    Connections {
        target: FrameClock
        enabled: root.levelEnabled

        function onFrameChanged() {
            root.level = peaks.peak;
        }
    }

    DeviceRow {
        width: root.width
        label: "OUT"
        node: root.sink
        fraction: root.outVolume
        value: Audio.volumeText(root.outVolume, root.outMuted)
        valueColor: Number.isFinite(root.outVolume) && !root.outMuted ? Theme.hi : Theme.dim
        onToggled: root.toggleMute(root.sink)
    }

    DeviceRow {
        width: root.width
        label: "IN"
        node: root.source
        // No level while nothing records: reading one would open a capture stream on the microphone.
        metered: root.recording
        fraction: root.level
        value: Audio.inputText(root.apps, root.inMuted)
        valueColor: root.recording ? Theme.warn : Theme.dim
        onToggled: root.toggleMute(root.source)
    }
}
