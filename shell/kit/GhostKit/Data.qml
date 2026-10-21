pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root

    readonly property int protoVersion: 1
    readonly property string socketFile: "ghostd.sock"
    readonly property string perfTopic: "perf"
    readonly property int staleAfterMs: 3000
    readonly property int firstRetryMs: 1000
    readonly property int maxRetryMs: 10000
    readonly property int defaultStepMs: 1000
    readonly property int defaultHistoryMinutes: 5
    readonly property int msPerMinute: 60000
    // History series, named as the daemon records them.
    readonly property string loadKey: "load"
    readonly property var ccdKeys: ["ccd0", "ccd1"]
    readonly property string gpuLoadKey: "gpu_load"
    readonly property var historyKeys: ({
            cpu: [loadKey].concat(ccdKeys),
            gpu: [gpuLoadKey]
        })

    readonly property string socketPath: Quickshell.env("GHOST_SOCKET") || runtimeSocket()

    readonly property bool connected: link.connected
    readonly property bool stale: link.stale
    readonly property var config: link.config
    readonly property var layout: link.layout
    readonly property var topics: link.topics
    readonly property var history: link.history

    signal configError(string message)

    function runtimeSocket() {
        const runtimeDir = Quickshell.env("XDG_RUNTIME_DIR");
        return runtimeDir ? runtimeDir + "/" + socketFile : "";
    }

    // A Quickshell Socket whose connect attempt failed keeps its dead QLocalSocket and ignores later
    // requests to connect, so every attempt gets a fresh Socket.
    function open() {
        if (link.socket) {
            link.socket.destroy();
        }
        link.socket = socketFactory.createObject(root, {
            path: socketPath
        });
        link.socket.connected = true;
    }

    function send(message) {
        link.socket.write(JSON.stringify(message) + "\n");
        link.socket.flush();
    }

    function linkChanged(socket) {
        if (socket !== link.socket) {
            return;
        }
        if (!socket.connected) {
            linkLost(socket);
            return;
        }
        link.connected = true;
        console.info("ghostd connected");
        send({
            t: "hello",
            proto: protoVersion,
            topics: []
        });
    }

    // A lost connection reports both an error and a state change, so only the first one schedules.
    function linkLost(socket) {
        if (socket !== link.socket || retryTimer.running) {
            return;
        }
        if (link.connected) {
            link.connected = false;
            console.info("ghostd disconnected");
        }
        retryTimer.interval = link.retryMs;
        retryTimer.start();
        link.retryMs = Math.min(link.retryMs * 2, maxRetryMs);
    }

    function receive(line) {
        let message;
        try {
            message = JSON.parse(line);
        } catch (parseError) {
            console.warn("ghostd sent a line that is not JSON");
            return;
        }
        link.stale = false;
        staleTimer.restart();
        switch (message.t) {
        case "hello":
            link.retryMs = firstRetryMs;
            break;
        case "config":
            link.config = message.data;
            break;
        case "layout":
            link.layout = message.data;
            send({
                t: "subscribe",
                topics: requiredTopics(message.data)
            });
            break;
        case "history":
            takeHistory(message);
            break;
        case "data":
            takeData(message.topic, message.data);
            break;
        case "config_error":
            console.warn("ghostd config problem: " + message.message);
            configError(message.message);
            break;
        case "error":
            console.error("ghostd refused the shell: " + message.message);
            break;
        }
    }

    function requiredTopics(layout) {
        const wanted = new Set([perfTopic]);
        const visit = node => {
            if (Array.isArray(node.children)) {
                node.children.forEach(visit);
                return;
            }
            const slots = Array.isArray(node.tabs) ? node.tabs : [node];
            slots.forEach(slot => slot.requires.forEach(topic => wanted.add(topic)));
        };
        Object.keys(layout.views).forEach(name => visit(layout.views[name]));
        return Array.from(wanted);
    }

    function takeHistory(message) {
        link.stepMs = message.step_ms;
        link.history = message.series;
        // After a history the daemon replays each topic's last value, which the history already ends
        // with; appending it again would shift every graph by one sample.
        const pending = {};
        for (const topic of Object.keys(historyKeys)) {
            pending[topic] = historyKeys[topic].some(key => (message.series[key] || []).length > 0);
        }
        link.replayPending = pending;
    }

    function takeData(topic, data) {
        const isReplay = link.replayPending[topic] === true;
        link.replayPending[topic] = false;
        const samples = isReplay ? null : historySamples(topic, data);
        if (samples) {
            appendHistory(samples);
        }
        const next = Object.assign({}, link.topics);
        next[topic] = data;
        link.topics = next;
    }

    // A series only grows while its source exists, matching the daemon, which records gpu_load only
    // while a GPU is present.
    function historySamples(topic, data) {
        if (topic === "cpu") {
            const samples = {
                [loadKey]: data.load
            };
            const ccdCount = Math.min(data.ccds.length, ccdKeys.length);
            for (let i = 0; i < ccdCount; i += 1) {
                samples[ccdKeys[i]] = data.ccds[i].load;
            }
            return samples;
        }
        if (topic === "gpu" && data.present) {
            return {
                [gpuLoadKey]: data.load
            };
        }
        return null;
    }

    function appendHistory(samples) {
        const keep = historyLength();
        const next = Object.assign({}, link.history);
        for (const key of Object.keys(samples)) {
            next[key] = (next[key] || []).concat([samples[key]]).slice(-keep);
        }
        link.history = next;
    }

    function historyLength() {
        const minutes = link.config ? link.config.dashboard.history_minutes : defaultHistoryMinutes;
        return Math.max(1, Math.floor(minutes * msPerMinute / link.stepMs));
    }

    Component.onCompleted: {
        if (socketPath === "") {
            console.warn("cannot find ghostd: set GHOST_SOCKET or XDG_RUNTIME_DIR");
            return;
        }
        open();
    }

    QtObject {
        id: link

        property Socket socket: null
        property bool connected: false
        property bool stale: false
        property var config: null
        property var layout: null
        property var topics: ({})
        property var history: ({})
        property int stepMs: root.defaultStepMs
        property int retryMs: root.firstRetryMs
        property var replayPending: ({})
    }

    Timer {
        id: staleTimer

        interval: root.staleAfterMs
        running: true
        onTriggered: link.stale = true
    }

    Timer {
        id: retryTimer

        onTriggered: root.open()
    }

    Component {
        id: socketFactory

        Socket {
            id: socket

            parser: SplitParser {
                onRead: line => root.receive(line)
            }
            onConnectionStateChanged: root.linkChanged(socket)
            onError: root.linkLost(socket)
        }
    }
}
