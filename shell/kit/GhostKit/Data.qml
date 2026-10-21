pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import "lib/history.mjs" as HistoryLib
import "lib/subscriptions.mjs" as Subscriptions

Singleton {
    id: root

    readonly property bool connected: link.connected
    readonly property bool stale: link.stale
    readonly property var config: link.config
    readonly property var layout: link.layout
    readonly property var history: link.history

    // While windows cover the dashboard it subscribes only to what the bar shows. Read-only here so
    // that one widget cannot starve the whole desktop of data; the host writes it through hostControl.
    readonly property bool covered: hostControl.covered
    readonly property QtObject hostControl: QtObject {
        property bool covered: false
    }

    signal configError(string message)

    // The one TopicValue for the topic, whose value is null until the topic's first message.
    function topic(name) {
        return link.topicValue(name);
    }

    // Subscribes to a topic that no layout slot requires, until the matching release(). Counted, so
    // widgets that want the same topic each release their own claim.
    function want(name) {
        link.want(name);
    }

    function release(name) {
        link.release(name);
    }

    onCoveredChanged: link.subscribeLater()
    Component.onCompleted: link.start()

    // The protocol lives in here so widgets see only the properties, functions and signal above.
    QtObject {
        id: link

        readonly property int protoVersion: 1
        readonly property string socketFile: "ghostd.sock"
        readonly property int minStaleAfterMs: 3000
        readonly property int staleAfterSteps: 3
        readonly property int firstRetryMs: 1000
        readonly property int maxRetryMs: 10000
        readonly property int defaultStepMs: 1000
        readonly property int defaultHistoryMinutes: 5
        readonly property int msPerMinute: 60000
        // The bar's topics, which stay subscribed while the dashboard is covered. There is no bar yet.
        readonly property var barRequires: []

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
        // The topic list last sent on this connection, as JSON, or null before the first.
        property var subscribed: null
        // Each wanted topic's count of want() calls without a matching release().
        property var wantCounts: new Map()
        // Each topic's seq last received on this connection. Every subscribe makes the daemon replay
        // each topic's last message with its seq unchanged, which is not new data.
        property var receivedSeqs: new Map()

        // Each topic's TopicValue. Filled in place rather than replaced: every binding that calls
        // topic() reads this property, so replacing it would re-run all of them.
        property var values: new Map()
        // What widgets see of the history, replaced as a whole on publish so bindings fire.
        property var history: ({})
        // The latest received state, published on the next frame clock tick while the clock runs:
        // each topic's newest message, and the history with its gap bookkeeping.
        property var staged: new Map()
        property var track: HistoryLib.fromHistory({})
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
            subscribed = null;
            receivedSeqs.clear();
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
                subscribeLater();
                break;
            case "history":
                takeHistory(message);
                break;
            case "data":
                takeData(message);
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

        function topicValue(name) {
            if (!values.has(name)) {
                values.set(name, topicFactory.createObject(root));
            }
            return values.get(name);
        }

        function warnUnlessValid(name) {
            const valid = Subscriptions.isValidTopic(name);
            if (!valid) {
                console.warn("invalid topic name \"" + name + "\"");
            }
            return valid;
        }

        function want(name) {
            if (!warnUnlessValid(name)) {
                return;
            }
            const count = (wantCounts.get(name) || 0) + 1;
            wantCounts.set(name, count);
            if (count === 1) {
                subscribeLater();
            }
        }

        function release(name) {
            if (!warnUnlessValid(name)) {
                return;
            }
            const count = wantCounts.get(name) || 0;
            if (count === 0) {
                console.warn("Data.release(\"" + name + "\") called without a matching want()");
                return;
            }
            if (count > 1) {
                wantCounts.set(name, count - 1);
                return;
            }
            wantCounts.delete(name);
            subscribeLater();
        }

        // Changes in one event-loop turn, such as a release followed by a want, add up to one list.
        function subscribeLater() {
            Qt.callLater(subscribe);
        }

        // Every subscribe makes the daemon replay each subscribed topic's last message, and resend
        // the history when cpu or gpu is subscribed, so an unchanged list is not sent again.
        function subscribe() {
            if (!connected) {
                return;
            }
            const topics = Subscriptions.subscriptionFor({
                requires: layout ? Subscriptions.requiresOf(layout) : [],
                wanted: Array.from(wantCounts.keys()),
                covered: root.covered,
                barRequires: barRequires
            });
            const list = JSON.stringify(topics);
            if (list === subscribed) {
                return;
            }
            subscribed = list;
            send({
                t: "subscribe",
                topics: topics
            });
        }

        function takeHistory(message) {
            stepMs = message.step_ms;
            track = HistoryLib.fromHistory(message.series);
            historyStaged = true;
            publishUnlessAnimating();
        }

        function takeData(message) {
            const recorded = HistoryLib.withData(track, message, historyLength());
            if (recorded.series !== track.series) {
                historyStaged = true;
            }
            track = recorded;
            if (receivedSeqs.get(message.topic) !== message.seq) {
                receivedSeqs.set(message.topic, message.seq);
                staged.set(message.topic, {
                    value: message.data,
                    seq: message.seq,
                    receivedMs: Date.now()
                });
            }
            publishUnlessAnimating();
        }

        function historyLength() {
            const minutes = config ? config.dashboard.history_minutes : defaultHistoryMinutes;
            return Math.max(1, Math.floor(minutes * msPerMinute / stepMs));
        }

        // Data that lands between two clock ticks would render a frame of its own, so while anything
        // animates it waits for the next tick. Otherwise the messages of one daemon tick arrive back to
        // back, and callLater publishes them together instead of once per message.
        function publishUnlessAnimating() {
            if (!FrameClock.running) {
                Qt.callLater(publish);
            }
        }

        // Only the topics that received a message are written, each once with its newest message, so
        // only their bindings re-run.
        function publish() {
            if (staged.size > 0) {
                const pending = staged;
                staged = new Map();
                pending.forEach((message, name) => deliver(topicValue(name).dataControl, message));
            }
            if (historyStaged) {
                historyStaged = false;
                history = track.series;
            }
        }

        // The value goes last, so a handler on it already sees the same message's seq and time.
        function deliver(control, message) {
            control.seq = message.seq;
            control.receivedMs = message.receivedMs;
            control.value = message.value;
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
        id: topicFactory

        TopicValue {}
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
