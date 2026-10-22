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

    // A transient sequence, so it may have a timer of its own while it runs, as PrintSwap's print does.
    Timer {
        interval: root.tickMs
        repeat: true
        running: root.playing
        onTriggered: root.tick()
    }

    LazyLoader {
        active: root.state !== null && root.state.phase !== "idle"

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
