import QtQuick
import GhostKit

// The perf topic reports CPU in whole-percent steps, so a single sample flickers between 0 and 1 %;
// a moving average shows the actual cost.
Text {
    id: root

    readonly property int windowSamples: 10
    readonly property int kibPerMib: 1024
    readonly property int cpuDecimals: 1
    readonly property real fontPx: 10
    readonly property string missing: "--"

    readonly property QtObject perf: Data.topic("perf")
    property var samples: []
    property int lastSeq: 0

    function mean(key) {
        const values = samples.map(sample => sample[key]).filter(Number.isFinite);
        if (values.length === 0) {
            return NaN;
        }
        return values.reduce((sum, value) => sum + value, 0) / values.length;
    }

    function cpuText(key) {
        const value = mean(key);
        return Number.isFinite(value) ? value.toFixed(cpuDecimals) + "%" : missing;
    }

    function rssText(key) {
        const value = mean(key);
        return Number.isFinite(value) ? Math.round(value / kibPerMib) + " MiB" : missing;
    }

    text: "GHOSTD " + cpuText("daemon_cpu") + " " + rssText("daemon_rss_kib") + "   SHELL " + cpuText("shell_cpu") + " "
        + rssText("shell_rss_kib")
    color: Data.connected && !Data.stale ? Theme.text : Theme.dim
    font.family: Theme.font
    font.pixelSize: Theme.d(fontPx)

    Connections {
        target: root.perf

        // Every new subscription replays the last sample, which the window already holds.
        function onValueChanged() {
            if (root.perf.seq === root.lastSeq) {
                return;
            }
            root.lastSeq = root.perf.seq;
            root.samples = root.samples.concat([root.perf.value]).slice(-root.windowSamples);
        }
    }
}
