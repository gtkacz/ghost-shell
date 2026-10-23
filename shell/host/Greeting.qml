import QtQuick
import Quickshell
import Quickshell.Io
import GhostKit
import "../kit/GhostKit/lib/greeting.mjs" as GreetingLib
import "../kit/GhostKit/lib/scale.mjs" as Scale

// Plays the greeting (spec §7) and hides the dashboard until its scan pass reveals it. The overlay exists only while
// the greeting is not idle, so an idle shell carries no greeting objects and nothing ticks.
Scope {
    id: root

    readonly property string titlePrefix: "Welcome back, "
    // About 62.5 frames a second, finer than the scan pass's 26 ms steps (Plan 2c call 8).
    readonly property int tickMs: 16
    readonly property int snapshotDelayMs: 1500
    readonly property real frozenJitter: 0.5
    readonly property string markerName: "ghost-shell-greeted"
    readonly property var modes: ["auto", "off", "login"]
    readonly property string defaultMode: "auto"

    readonly property string modeText: Quickshell.env("GHOST_GREETING") || defaultMode
    readonly property string mode: modes.includes(modeText) ? modeText : defaultMode
    readonly property string frozenText: Quickshell.env("GHOST_GREETING_AT") || ""
    // Number() rather than parseFloat() so a value with trailing junk is rejected, not truncated.
    readonly property real frozenMs: Number(frozenText)
    readonly property bool frozen: frozenText !== "" && Number.isFinite(frozenMs) && frozenMs >= 0
    readonly property string snapshotPath: Quickshell.env("GHOST_GREETING_SNAPSHOT") || ""
    readonly property string runtimeDir: Quickshell.env("XDG_RUNTIME_DIR") || ""
    // Login mode is for development: it neither reads nor writes the marker, so a dev run never keeps the real
    // login from greeting.
    readonly property string markerPath: mode === "auto" && runtimeDir !== "" ? runtimeDir + "/" + markerName : ""

    readonly property var mainScreen: Scale.pickScreen(Quickshell.screens, Data.config ? Data.config.display.model : "")

    // GreetingLib's phase state, null until the start has read the marker. The dashboard waits for it, so that its
    // widgets are not created only to be hidden at once.
    property var state: null
    property var plan: null
    property var frame: null
    property double startedMs: 0
    property bool revealed: false
    // Counts the connections to ghostd, whose first session value each is only a baseline for greeting_seq.
    property int connection: 0
    // Up for one moment only, while the overlay is replaced by a new one.
    property bool remapping: false

    readonly property bool playing: state !== null && state.phase === "playing"
    readonly property bool hidesDashboard: state === null || state.phase === "armed" || (playing && !revealed)

    function skip() {
        if (!playing) {
            return;
        }
        if (!revealed) {
            reveal();
        }
        console.info("greeting skipped");
        state = GreetingLib.skipped(state);
    }

    // The frame is computed before the phase is set, so the overlay's first frame already draws it.
    function play(next) {
        revealed = false;
        const designHeight = mainScreen ? Scale.designHeight(mainScreen.width, mainScreen.height) : undefined;
        plan = GreetingLib.sequence({
            fromDots: next.fromDots,
            designHeight: designHeight
        });
        startedMs = Date.now();
        frame = GreetingLib.frameAt(plan, elapsedMs());
        console.info("greeting playing " + next.kind);
        state = next;
    }

    // KWin draws one frame of an overlay mapped under the lock and no more until the unlock, so the resting frame is
    // set before the phase, which is what creates the overlay.
    function follow(next) {
        const was = state.phase;
        if (next.phase === "playing" && was !== "playing") {
            play(next);
            return;
        }
        if (next.phase === "armed" && was !== "armed") {
            plan = null;
            frame = GreetingLib.restingFrame();
            console.info("greeting armed");
        } else if (next.phase === "idle" && was === "armed") {
            console.info("greeting disarmed");
        }
        state = next;
    }

    // KWin hides the overlay as soon as the lock starts, before ghostd says so, and keeps the buffer it drew last: a
    // frame of the cut greeting, which the unlock would show. A sleep locks first too (kscreenlocker's LockOnResume
    // locks at once on PrepareForSleep), so it cuts the same way. New surfaces mapped under the lock draw the resting
    // frame as their one frame.
    function takeSession(session) {
        const cut = playing && (session.locked || session.sleeping);
        follow(GreetingLib.withSession(state, session, connection));
        if (cut) {
            remapping = true;
            remapping = false;
        }
    }

    function elapsedMs() {
        return frozen ? frozenMs : Date.now() - startedMs;
    }

    function tick() {
        const atMs = elapsedMs();
        // A frozen greeting reaches its typing at once, so it waits for the config whose name and language it shows.
        if (plan.lines === null && atMs >= plan.typingStartMs && (!frozen || Data.config !== null)) {
            plan = GreetingLib.typeText(plan, {
                title: titlePrefix + (Data.config ? Data.config.greeting.name : Quickshell.env("USER")),
                nameStart: titlePrefix.length,
                subtitle: GreetingLib.subtitleText({
                    japanese: Theme.japanese,
                    sessionId: Data.topic("session").value?.id ?? ""
                })
            }, frozen ? () => frozenJitter : Math.random);
        }
        frame = GreetingLib.frameAt(plan, atMs);
        if (frame.scanStep !== null && !revealed) {
            reveal();
        }
        if (frame.done) {
            console.info("greeting done");
            state = GreetingLib.finished(state);
        }
    }

    // Both resets come before the dashboard is shown again, so its widgets, created then, scramble and blink anew.
    function reveal() {
        FirstWrite.reset();
        AlertAttention.rearm();
        console.info("greeting revealed");
        revealed = true;
    }

    Component.onCompleted: {
        if (modeText !== mode) {
            console.warn("GHOST_GREETING must be one of " + modes.join(", ") + ", got '" + modeText + "'; using " + defaultMode);
        }
        if (frozenText !== "" && !frozen) {
            console.warn("GHOST_GREETING_AT must be a number of milliseconds, got '" + frozenText + "'; not freezing");
        }
        let greeted = false;
        if (markerPath !== "") {
            // text() makes the blocking read happen now; loaded then says whether the marker exists.
            marker.text();
            greeted = marker.loaded;
        }
        const initial = GreetingLib.initialGreeting({
            mode: mode,
            greeted: greeted
        });
        if (initial.phase !== "playing") {
            state = initial;
            return;
        }
        // Any content does; an empty text would equal the absent file's and not be written.
        if (markerPath !== "") {
            marker.setText(String(Date.now()));
        }
        play(initial);
    }

    // The runtime directory is emptied at logout, so the marker says this login has been greeted (Plan 2c call 6).
    FileView {
        id: marker

        path: root.markerPath
        blockLoading: true
        blockWrites: true
        printErrors: false
    }

    Connections {
        target: Data

        function onConnectedChanged() {
            if (Data.connected) {
                root.connection += 1;
            } else {
                root.follow(GreetingLib.disconnected(root.state));
            }
        }
    }

    Connections {
        target: Data.topic("session")

        function onValueChanged() {
            root.takeSession(Data.topic("session").value);
        }
    }

    // Lets scripts/smoke-greeting.sh lock, unlock and skip without a screen locker or a keyboard. Served only under
    // GHOST_DEBUG=1. It lives in this scope, which is never destroyed, because Quickshell 0.2.1 keeps a destroyed
    // handler registered, and calling it then crashes qs.
    IpcHandler {
        enabled: Quickshell.env("GHOST_DEBUG") === "1"
        target: "greeting"

        // A session snapshot as ghostd publishes it, taken as if it came from the current connection.
        function session(json: string): void {
            let value = null;
            try {
                value = JSON.parse(json);
            } catch (parseError) {
                value = null;
            }
            if (value === null || typeof value !== "object") {
                console.warn("greeting session wants a JSON object, got '" + json + "'");
                return;
            }
            root.takeSession(value);
        }

        function skip(): void {
            root.skip();
        }
    }

    // A transient sequence, so it may have a timer of its own while it runs, as PrintSwap's print does.
    Timer {
        interval: root.tickMs
        repeat: true
        running: root.playing
        onTriggered: root.tick()
    }

    LazyLoader {
        active: root.state !== null && root.state.phase !== "idle" && !root.remapping

        Variants {
            model: Quickshell.screens

            GreetingScreen {
                isMain: modelData === root.mainScreen
                frame: root.frame
                plan: root.plan
                playing: root.playing
                snapshotPath: root.snapshotPath
                snapshotDelayMs: root.snapshotDelayMs
                // A skip unloads this window, so it waits until the key or click handler that asked has returned;
                // callLater also folds several requests in one turn into one skip.
                onSkipRequested: Qt.callLater(root.skip)
            }
        }
    }
}
