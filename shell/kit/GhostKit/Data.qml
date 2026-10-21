pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root

    readonly property bool connected: link.connected
    readonly property bool stale: link.stale
    readonly property var config: link.config
    readonly property var layout: link.layout
    readonly property var topics: link.topics
    readonly property var history: link.history

    signal configError(string message)

    Component.onCompleted: link.start()

    // The protocol lives in here so widgets see only the properties and the signal above.
    QtObject {
        id: link

        readonly property int protoVersion: 1
        readonly property string socketFile: "ghostd.sock"
        readonly property string cpuTopic: "cpu"
        readonly property string gpuTopic: "gpu"
        readonly property string perfTopic: "perf"
        readonly property int minStaleAfterMs: 3000
        readonly property int staleAfterSteps: 3
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
                [cpuTopic]: [loadKey].concat(ccdKeys),
                [gpuTopic]: [gpuLoadKey]
            })

        readonly property string socketPath: Quickshell.env("GHOST_SOCKET") || runtimeSocket()

        property Socket socket: null
        property bool connected: false
        property bool stale: false
        property var config: null
        property var layout: null
        property int stepMs: defaultStepMs
        // Data arrives once per step, so a fixed limit would flag every gap between ticks of a slower daemon.
        readonly property int staleAfterMs: Math.max(minStaleAfterMs, staleAfterSteps * stepMs)
        property int retryMs: firstRetryMs
        property var replayPending: ({})

        // What widgets see. Each is replaced as a whole on publish so bindings fire.
        property var topics: ({})
        property var history: ({})
        // The latest received state, published on the next frame clock tick while the clock runs.
        property var stagedTopics: ({})
        property var stagedHistory: ({})
        property bool topicsStaged: false
        property bool historyStaged: false

        function start() {
            if (socketPath === "") {
                console.warn("cannot find ghostd: set GHOST_SOCKET or XDG_RUNTIME_DIR");
                return;
            }
            open();
        }

        function runtimeSocket() {
            const runtimeDir = Quickshell.env("XDG_RUNTIME_DIR");
            return runtimeDir ? runtimeDir + "/" + socketFile : "";
        }

        // A Quickshell Socket whose connect attempt failed keeps its dead QLocalSocket and ignores
        // later requests to connect, so every attempt gets a fresh Socket.
        function open() {
            if (socket) {
                socket.destroy();
            }
            socket = socketFactory.createObject(root, {
                path: socketPath
            });
            socket.connected = true;
        }

        function send(message) {
            socket.write(JSON.stringify(message) + "\n");
            socket.flush();
        }

        function handleConnection(changed) {
            if (changed !== socket) {
                return;
            }
            if (!changed.connected) {
                handleLoss(changed);
                return;
            }
            connected = true;
            console.info("ghostd connected");
            send({
                t: "hello",
                proto: protoVersion,
                topics: []
            });
        }

        // A lost connection reports both an error and a state change, so only the first one schedules.
        function handleLoss(failed) {
            if (failed !== socket || retryTimer.running) {
                return;
            }
            if (connected) {
                connected = false;
                console.info("ghostd disconnected");
            }
            retryTimer.interval = retryMs;
            retryTimer.start();
            retryMs = Math.min(retryMs * 2, maxRetryMs);
        }

        function parseObject(line) {
            try {
                const value = JSON.parse(line);
                return value !== null && typeof value === "object" ? value : null;
            } catch (parseError) {
                return null;
            }
        }

        function receive(line) {
            const message = parseObject(line);
            if (message === null) {
                console.warn("ghostd sent a line that is not a JSON object");
                return;
            }
            stale = false;
            staleTimer.restart();
            switch (message.t) {
            case "hello":
                retryMs = firstRetryMs;
                break;
            case "config":
                config = message.data;
                break;
            case "layout":
                layout = message.data;
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
                root.configError(message.message);
                break;
            case "error":
                console.error("ghostd refused the shell: " + message.message);
                break;
            }
        }

        function requiredTopics(resolvedLayout) {
            const wanted = new Set([perfTopic]);
            const visit = node => {
                if (Array.isArray(node.children)) {
                    node.children.forEach(visit);
                    return;
                }
                const slots = Array.isArray(node.tabs) ? node.tabs : [node];
                slots.forEach(slot => slot.requires.forEach(topic => wanted.add(topic)));
            };
            Object.keys(resolvedLayout.views).forEach(name => visit(resolvedLayout.views[name]));
            return Array.from(wanted);
        }

        function takeHistory(message) {
            stepMs = message.step_ms;
            stagedHistory = message.series;
            historyStaged = true;
            // After a history the daemon replays each topic's last value, which the history already
            // ends with; appending it again would shift every graph by one sample.
            const pending = {};
            for (const topic of Object.keys(historyKeys)) {
                pending[topic] = historyKeys[topic].some(key => (message.series[key] || []).length > 0);
            }
            replayPending = pending;
            publishUnlessAnimating();
        }

        function takeData(topic, data) {
            const isReplay = replayPending[topic] === true;
            replayPending[topic] = false;
            const samples = isReplay ? null : historySamples(topic, data);
            if (samples) {
                appendHistory(samples);
            }
            stagedTopics[topic] = data;
            topicsStaged = true;
            publishUnlessAnimating();
        }

        // A series only grows while its source exists, matching the daemon, which records gpu_load
        // only while a GPU is present.
        function historySamples(topic, data) {
            if (topic === cpuTopic) {
                const samples = {
                    [loadKey]: data.load
                };
                const ccdCount = Math.min(data.ccds.length, ccdKeys.length);
                for (let i = 0; i < ccdCount; i += 1) {
                    samples[ccdKeys[i]] = data.ccds[i].load;
                }
                return samples;
            }
            if (topic === gpuTopic && data.present) {
                return {
                    [gpuLoadKey]: data.load
                };
            }
            return null;
        }

        // Series are replaced, never changed in place, so published snapshots can share them.
        function appendHistory(samples) {
            const keep = historyLength();
            for (const key of Object.keys(samples)) {
                stagedHistory[key] = (stagedHistory[key] || []).concat([samples[key]]).slice(-keep);
            }
            historyStaged = true;
        }

        function historyLength() {
            const minutes = config ? config.dashboard.history_minutes : defaultHistoryMinutes;
            return Math.max(1, Math.floor(minutes * msPerMinute / stepMs));
        }

        // Data that lands between two clock ticks would render a frame of its own, so while anything
        // animates it waits for the next tick.
        function publishUnlessAnimating() {
            if (!FrameClock.running) {
                publish();
            }
        }

        function publish() {
            if (topicsStaged) {
                topicsStaged = false;
                topics = Object.assign({}, stagedTopics);
            }
            if (historyStaged) {
                historyStaged = false;
                history = Object.assign({}, stagedHistory);
            }
        }
    }

    Connections {
        target: FrameClock

        function onFrameChanged() {
            link.publish();
        }

        // Updates staged since the last tick must not wait for the next message once the clock stops.
        function onRunningChanged() {
            link.publish();
        }
    }

    Timer {
        id: staleTimer

        interval: link.staleAfterMs
        running: true
        onTriggered: link.stale = true
    }

    Timer {
        id: retryTimer

        onTriggered: link.open()
    }

    Component {
        id: socketFactory

        Socket {
            id: socket

            parser: SplitParser {
                onRead: line => link.receive(line)
            }
            onConnectionStateChanged: link.handleConnection(socket)
            onError: link.handleLoss(socket)
        }
    }
}
