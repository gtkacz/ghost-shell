import QtQuick
import GhostKit
import "media.mjs" as Media

// The spectrum's bands and the holds behind them: the audio.spectrum subscription and the frame clock while the bars
// are shown and the player plays, and the clock alone while they fall to dark after it stops.
Item {
    id: root

    readonly property int spectrumBands: 32
    // appearance.spectrum_fps at which the spectrum holds FrameClock's fast interval; any other rate holds the normal one.
    readonly property int fastSpectrumFps: 30
    readonly property real bandMax: 255
    readonly property var darkLevels: new Array(spectrumBands).fill(0)

    // Whether the bars are on screen, and whether the player plays.
    property bool spectrumShown: false
    property bool playing: false

    readonly property int spectrumFps: Theme.spectrumFps
    // The spec's "only while playing and visible"; the rate is appearance.spectrum_fps.
    readonly property bool spectrumLive: spectrumShown && playing
    readonly property var spectrumTopic: Data.topic("audio.spectrum")
    // The bands the bars fall through after the music stops, 0..255, or none when they are not falling. The topic is
    // released at once, and the daemon stops capturing, so the fall runs on the shell's copy of the last bands.
    property var fallBands: []
    readonly property bool falling: fallBands.length > 0
    readonly property var levels: spectrumLive && spectrumTopic.live && spectrumTopic.value ? spectrumTopic.value.bands.map(band => band / bandMax) : falling ? fallBands.map(band => band / bandMax) : darkLevels

    property bool holdsSpectrum: false
    // The rate the spectrum holds the frame clock at, 0 while it holds none.
    property int heldFps: 0

    // Only when playing-and-visible changes, never per frame: every change of the subscription makes the daemon
    // resend history and replay each value.
    function holdSpectrum(wanted) {
        if (wanted === holdsSpectrum) {
            return;
        }
        if (wanted) {
            Data.want("audio.spectrum");
        } else {
            Data.release("audio.spectrum");
        }
        holdsSpectrum = wanted;
        holdSpectrumClock();
    }

    // Called a turn after spectrumLive changes, never from its handler: that handler runs while `view` is still being
    // worked out (spectrumLive follows playing, which follows view), and dropping the last clock hold there lets
    // FrameClock re-read the second, which nowMs and so `view` depend on. Qt then reports a binding loop for `view`.
    function holdLiveSpectrum() {
        holdSpectrum(spectrumLive);
        // holdSpectrum returns early when the data hold already matches, but a resume and a pause inside one turn can
        // start or clear a fall without changing it, and the clock hold has to follow the fall too.
        holdSpectrumClock();
    }

    // The clock runs for the live spectrum and for its fall. A fall that starts or ends beside a live spectrum asks
    // for the rate already held, so the one hold carries over and the clock never stops between the two.
    function holdSpectrumClock() {
        holdClockAt((holdsSpectrum || falling) ? spectrumFps : 0);
    }

    // The released topic keeps its last value, which is what the bars showed while the topic was live.
    function startFall() {
        const shown = spectrumTopic.live && spectrumTopic.value ? spectrumTopic.value.bands : [];
        if (!Media.isDark(shown)) {
            fallBands = Array.from(shown);
        }
    }

    function stepFall() {
        const next = Media.fallStep(fallBands);
        if (Media.isDark(next)) {
            endFall();
        } else {
            fallBands = next;
        }
    }

    function endFall() {
        fallBands = [];
        holdSpectrumClock();
    }

    // A changed appearance.spectrum_fps moves the hold to the other interval and leaves the subscription alone.
    function holdClockAt(fps) {
        if (fps === heldFps) {
            return;
        }
        const previous = heldFps;
        // The new hold is taken before the old one goes, so a widget that is the clock's only holder never lets it
        // stop for an instant between the two.
        if (fps === fastSpectrumFps) {
            FrameClock.acquireFast();
        } else if (fps > 0) {
            FrameClock.acquire();
        }
        if (previous === fastSpectrumFps) {
            FrameClock.releaseFast();
        } else if (previous > 0) {
            FrameClock.release();
        }
        heldFps = fps;
    }

    onSpectrumLiveChanged: {
        if (spectrumLive) {
            fallBands = [];
        } else if (spectrumShown) {
            startFall();
        }
        Qt.callLater(holdLiveSpectrum);
    }
    // A fall ends when the bars are hidden. A live spectrum that is hidden stops through its own spectrumLive change, but a
    // paused one has no change of its own to end the fall on.
    onSpectrumShownChanged: {
        if (!spectrumShown) {
            endFall();
        }
    }
    onSpectrumFpsChanged: holdSpectrumClock()
    Component.onCompleted: holdSpectrum(spectrumLive)
    Component.onDestruction: {
        endFall();
        holdSpectrum(false);
    }

    Connections {
        target: FrameClock
        enabled: root.falling

        function onFrameChanged() {
            root.stepFall();
        }
    }
}
