import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Mpris
import GhostKit
import "media.mjs" as Media

// The player: its art, track, a spectrum of what the default output plays, the progress and the controls, in three
// styles: a stack with the art on top (V1), a headerless strip (V2) and a compact row for the MONITOR band. Paused,
// the spectrum falls to dark; stopped or gone, the last track played is shown dimmed. That track is remembered across
// restarts in $XDG_STATE_HOME/ghost-shell/media.json. Below the player, rows show the default output and input, and
// a MIC LIVE alert says which apps record.
Panel {
    id: root

    readonly property string stackStyle: "stack"
    readonly property string stripStyle: "strip"
    readonly property string compactStyle: "compact"

    // Design pixels, from the reference mockup's .media, .art, .minfo, .prog and .ctrl, and its mediaStrip and
    // bandMedia.
    readonly property real stackArtPx: 124
    readonly property real stripArtPx: 84
    readonly property real compactArtPx: 44
    readonly property real artGapPx: 12
    readonly property real bodyGapPx: 8
    readonly property real infoGapPx: 2
    readonly property real stripInfoGapPx: 4
    readonly property real stripInfoWidthPx: 220
    readonly property real stripVerticalPx: 12
    readonly property real stripSidePx: 14
    readonly property real stripColumnGapPx: 14
    readonly property real stripSpectrumPx: 52
    readonly property real compactVerticalPx: 10
    readonly property real compactSidePx: 14
    readonly property real compactGapPx: 6
    readonly property real statusPx: 8.5
    readonly property real statusGapPx: 6
    readonly property real titlePx: 18
    readonly property real compactTitlePx: 15
    readonly property real artistPx: 10
    readonly property real progressPx: 9.5
    readonly property real progressGapPx: 8
    readonly property real meterPx: 4
    readonly property real controlPx: 13
    readonly property real controlGapPx: 20
    readonly property real spectrumCellPx: 3
    readonly property real spectrumGapPx: 2
    readonly property real controlVolumePx: 9.5
    // Letter spacing in em.
    readonly property real statusTracking: 0.14
    readonly property real titleTracking: 0.02
    readonly property real controlVolumeTracking: 0.1
    // The mockup's scramble(el, to, dur) for a new track.
    readonly property real titleScrambleMs: 600
    readonly property real artistScrambleMs: 700

    readonly property real millisPerSecond: 1000
    readonly property string stateFile: "ghost-shell/media.json"
    readonly property string defaultStateDir: ".local/state"

    property var settings

    readonly property string style: settings.style
    // Whether the OUT and IN rows show. MIC LIVE shows either way: it is a privacy cue, and it opens no capture stream.
    readonly property bool audio: settings.audio
    // The daemon's default style when no config has arrived.
    readonly property bool mono: (Data.config?.bar?.icons ?? "mono") === "mono"

    // Every MPRIS player on the session bus, in no useful order.
    readonly property var players: Array.from(Mpris.players.values)
    // The dbus name of the last player seen playing, which pickPlayer prefers.
    property string lastActiveId: ""
    readonly property int playerIndex: Media.pickPlayer(players.map(candidate => ({
                    id: candidate.dbusName,
                    identity: candidate.identity,
                    playing: candidate.playbackState === MprisPlaybackState.Playing
                })), lastActiveId)
    readonly property MprisPlayer player: playerIndex >= 0 ? players[playerIndex] : null
    // Only the properties media.mjs reads, so the view follows them and nothing else on the player. A position or
    // length the player cannot report reads as unknown, which hides the progress.
    readonly property var playerState: player ? {
        playbackState: stateWord(player.playbackState),
        trackTitle: player.trackTitle,
        trackArtist: player.trackArtist,
        trackArtUrl: player.trackArtUrl,
        identity: player.identity,
        desktopEntry: player.desktopEntry,
        position: player.positionSupported ? player.position : NaN,
        length: player.lengthSupported ? player.length : 0
    } : null

    // Read from media.json once the widget is complete, then written with every new track and when the player goes.
    property var remembered: null
    property bool hadPlayer: false
    // The player's state can change while the widget is still being created, before the file has a path or was read.
    property bool loaded: false

    readonly property real nowMs: FrameClock.epochSecond * millisPerSecond
    readonly property var view: Media.mediaView(playerState, remembered, nowMs)
    readonly property bool playing: view.mode === "playing"
    readonly property bool stopped: view.mode === "stopped"
    readonly property string chipKey: playing ? "play" : stopped ? "stop" : "pause"
    // A headerless line is narrow, so when its chip already gives the state the words leave it out.
    readonly property string lineStatus: Theme.chip(chipKey) !== "" ? Media.shortStatus(view) : view.status
    readonly property var fraction: Media.progress(view.positionS, view.lengthS)
    readonly property bool hasProgress: fraction !== null
    // The title's key is the track's, so a new track scrambles once however many views show it; the artist line needs
    // a claim of its own.
    readonly property string titleKey: "media:" + view.title + "|" + view.artist
    readonly property string artistKey: titleKey + "|artist"

    readonly property bool canPrevious: player !== null && player.canGoPrevious
    readonly property bool canNext: player !== null && player.canGoNext
    // With no player, play opens the one that played the remembered track, if this machine has it.
    readonly property var launcher: player === null && remembered !== null && remembered.desktopEntry !== "" ? desktopEntry(remembered.desktopEntry) : null
    readonly property bool canPlayPause: player !== null ? (playing ? player.canPause : player.canPlay) : launcher !== null

    readonly property bool hasSpectrum: style !== compactStyle

    readonly property string statePath: {
        const stateHome = Quickshell.env("XDG_STATE_HOME") || "";
        // The XDG spec says to ignore a relative XDG_STATE_HOME.
        const dir = stateHome.startsWith("/") ? stateHome : Quickshell.env("HOME") + "/" + defaultStateDir;
        return dir + "/" + stateFile;
    }

    property bool holdsSeconds: false

    function stateWord(state) {
        if (state === MprisPlaybackState.Playing) {
            return "playing";
        }
        if (state === MprisPlaybackState.Paused) {
            return "paused";
        }
        return "stopped";
    }

    // DesktopEntries scans the applications on its first use, so a lookup before the scan ends finds nothing; reading
    // the list makes the caller's binding look again once it has.
    function desktopEntry(id) {
        DesktopEntries.applications.values;
        return DesktopEntries.byId(id);
    }

    // A turn later, since the player was picked using lastActiveId: writing it while that pick is still being
    // worked out would be a binding loop.
    function noteActive() {
        if (playing) {
            lastActiveId = player.dbusName;
        }
    }

    function trackIdentity(track) {
        return track ? [track.title, track.artist, track.artUrl, track.identity, track.desktopEntry].join("\n") : "";
    }

    function remember(track) {
        remembered = track;
        store.setText(JSON.stringify(track));
    }

    // A new track, or the same one whose art or player details arrived later.
    function rememberIfNew() {
        if (!loaded) {
            return;
        }
        const track = Media.rememberTrack(playerState, Date.now());
        if (track !== null && trackIdentity(track) !== trackIdentity(remembered)) {
            remember(track);
        }
    }

    function playOrPause() {
        if (player === null) {
            if (launcher !== null) {
                Launch.app(launcher);
            }
            return;
        }
        if (playing) {
            player.pause();
        } else {
            player.play();
        }
    }

    // The age of the remembered track reads the wall clock, and the position is asked for again every second; the
    // frame clock keeps the second current only for a holder.
    function holdSeconds(wanted) {
        if (wanted === holdsSeconds) {
            return;
        }
        if (wanted) {
            FrameClock.acquireSeconds();
        } else {
            FrameClock.releaseSeconds();
        }
        holdsSeconds = wanted;
    }

    // The art of the track shown, dimmed with it when it is the remembered one.
    component MediaArt: Art {
        source: root.view.artUrl
        mono: root.mono
        dimmed: root.stopped
    }

    // The play state's chip and the status words.
    component StatusLine: Item {
        id: line

        property string text
        property bool centred: false

        implicitHeight: Math.max(chip.implicitHeight, label.implicitHeight)

        Row {
            x: line.centred ? Math.max(0, (line.width - width) / 2) : 0
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.d(root.statusGapPx)

            Chip {
                id: chip

                anchors.verticalCenter: parent.verticalCenter
                key: root.chipKey
            }

            Text {
                id: label

                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, Math.max(0, line.width - (chip.visible ? chip.width + parent.spacing : 0)))
                text: line.text
                // The strip adds the player's own name, which AutoText would render as markup.
                textFormat: Text.PlainText
                color: Theme.dim
                elide: Text.ElideRight
                font {
                    family: Theme.font
                    pixelSize: Theme.d(root.statusPx)
                    weight: Font.Medium
                    letterSpacing: Theme.d(root.statusPx) * root.statusTracking
                }
            }
        }
    }

    component Title: ScrambleText {
        property real textPx: root.titlePx

        text: root.view.title
        scrambleKey: root.titleKey
        durationMs: root.titleScrambleMs
        color: root.stopped ? Theme.dim : Theme.hi
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: Theme.d(textPx)
            weight: Font.Bold
            letterSpacing: Theme.d(textPx) * root.titleTracking
        }
    }

    component Artist: ScrambleText {
        text: root.view.artist
        scrambleKey: root.artistKey
        durationMs: root.artistScrambleMs
        color: root.stopped ? Theme.dim : Theme.text
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: Theme.d(root.artistPx)
            weight: Font.Medium
        }
    }

    // The position, a thin meter and the length; absent for a stream without a length.
    component Progress: Item {
        id: progressRow

        readonly property real gap: Theme.d(root.progressGapPx)

        visible: root.hasProgress
        implicitHeight: positionText.implicitHeight

        Digits {
            id: positionText

            anchors.verticalCenter: parent.verticalCenter
            text: Media.mmss(root.view.positionS)
            pixelSize: Theme.d(root.progressPx)
            weight: Font.Medium
            color: Theme.dim
        }

        // The share of the track played is not an alarm, so the meter stays signal all the way.
        SegMeter {
            anchors {
                left: positionText.right
                leftMargin: progressRow.gap
                right: lengthText.left
                rightMargin: progressRow.gap
                verticalCenter: parent.verticalCenter
            }
            height: Theme.d(root.meterPx)
            value: root.fraction
            warnAt: Infinity
            critAt: Infinity
        }

        Digits {
            id: lengthText

            anchors {
                right: parent.right
                verticalCenter: parent.verticalCenter
            }
            text: Media.mmss(root.view.lengthS)
            pixelSize: Theme.d(root.progressPx)
            weight: Font.Medium
            color: Theme.dim
        }
    }

    // A control that is dim and lets clicks through to the desktop when the player cannot take it.
    component Control: Glyph {
        id: control

        property bool available: true

        signal activated

        size: Theme.d(root.controlPx)
        color: available ? Theme.hi : Theme.dim

        ClickArea {
            anchors.fill: parent
            enabled: control.available
            onClicked: control.activated()
        }
    }

    component Controls: Row {
        id: controls

        // The output's volume, as the mockup's `VOL 64`, or nothing without the audio rows.
        property string volume

        spacing: Theme.d(root.controlGapPx)

        Control {
            name: "prev"
            available: root.canPrevious
            onActivated: root.player.previous()
        }

        Control {
            name: root.playing ? "pause" : "play"
            available: root.canPlayPause
            onActivated: root.playOrPause()
        }

        Control {
            name: "next"
            available: root.canNext
            onActivated: root.player.next()
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: controls.volume !== ""
            text: controls.volume
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(root.controlVolumePx)
                weight: Font.Medium
                letterSpacing: Theme.d(root.controlVolumePx) * root.controlVolumeTracking
            }
        }
    }

    // MIC LIVE, naming the apps a style's audio rows see recording.
    component MicAlert: AlertBox {
        property AudioRows rows

        visible: rows.recording
        text: rows.alertText
        level: "warn"
        chipKey: "live"
    }

    anchors.fill: parent
    header: style === stackStyle
    label: "Media"
    tagKey: "media"
    meta: view.identity
    padding: header ? Theme.d(paddingPx) : 0

    onVisibleChanged: holdSeconds(visible)
    onPlayingChanged: Qt.callLater(noteActive)
    onPlayerStateChanged: rememberIfNew()
    // The pick can move to another playing player without playing changing, so every new pick is noted as well. A
    // player that quits is the moment its track was last played.
    onPlayerChanged: {
        Qt.callLater(noteActive);
        if (player === null && hadPlayer && remembered !== null) {
            remember(Object.assign({}, remembered, {
                atMs: Date.now()
            }));
        }
        hadPlayer = player !== null;
    }
    Component.onCompleted: {
        remembered = Media.parseRemembered(store.text());
        loaded = true;
        hadPlayer = player !== null;
        noteActive();
        rememberIfNew();
        holdSeconds(visible);
    }
    Component.onDestruction: holdSeconds(false)

    Spectrum {
        id: spectrum

        spectrumShown: root.hasSpectrum && root.visible
        playing: root.playing
    }

    FileView {
        id: store

        path: root.statePath
        // The file is a few hundred bytes, and the widget needs it before it decides what to show.
        blockLoading: true
        atomicWrites: true
        // A missing file is the first run, not a problem.
        printErrors: false
        onLoadFailed: error => {
            if (error !== FileViewError.FileNotFound) {
                console.warn("media: cannot read " + path + ": " + FileViewError.toString(error));
            }
        }
        onSaveFailed: error => console.warn("media: cannot write " + path + ": " + FileViewError.toString(error))
    }

    // MprisPlayer works the position out only when asked, by this signal.
    Connections {
        target: FrameClock
        enabled: root.playing

        function onEpochSecondChanged() {
            root.player.positionChanged();
        }
    }

    Component {
        id: stack

        Item {
            Column {
                id: stackTop

                anchors {
                    top: parent.top
                    left: parent.left
                    right: parent.right
                }
                spacing: Theme.d(root.artGapPx)

                MediaArt {
                    anchors.horizontalCenter: parent.horizontalCenter
                    sidePx: root.stackArtPx
                }

                Column {
                    width: parent.width
                    spacing: Theme.d(root.infoGapPx)

                    StatusLine {
                        width: parent.width
                        centred: true
                        text: root.view.status
                    }

                    Title {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                    }

                    Artist {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                    }
                }
            }

            // The one element that stretches.
            SegBars {
                anchors {
                    top: stackTop.bottom
                    topMargin: Theme.d(root.bodyGapPx)
                    left: parent.left
                    right: parent.right
                    bottom: stackBottom.top
                    bottomMargin: Theme.d(root.bodyGapPx)
                }
                levels: spectrum.levels
                cellH: root.spectrumCellPx
                gap: root.spectrumGapPx
            }

            Column {
                id: stackBottom

                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                }
                spacing: Theme.d(root.bodyGapPx)

                Progress {
                    width: parent.width
                }

                Controls {
                    anchors.horizontalCenter: parent.horizontalCenter
                    volume: root.audio ? stackAudio.controlVolume : ""
                }

                AudioRows {
                    id: stackAudio

                    width: parent.width
                    visible: root.audio
                }

                MicAlert {
                    width: parent.width
                    rows: stackAudio
                }
            }
        }
    }

    Component {
        id: strip

        Item {
            Item {
                anchors {
                    fill: parent
                    topMargin: Theme.d(root.stripVerticalPx)
                    bottomMargin: Theme.d(root.stripVerticalPx)
                    leftMargin: Theme.d(root.stripSidePx)
                    rightMargin: Theme.d(root.stripSidePx)
                }

                MediaArt {
                    id: stripArt

                    anchors.verticalCenter: parent.verticalCenter
                    sidePx: root.stripArtPx
                }

                Column {
                    id: stripInfo

                    anchors {
                        left: stripArt.right
                        leftMargin: Theme.d(root.stripColumnGapPx)
                        verticalCenter: parent.verticalCenter
                    }
                    width: Theme.d(root.stripInfoWidthPx)
                    spacing: Theme.d(root.stripInfoGapPx)

                    // Headerless, so the player's name joins the status, as in the mockup. The strip has no other room
                    // for MIC LIVE, so the alert takes the status's place while an app records.
                    StatusLine {
                        width: parent.width
                        visible: !stripAlert.visible
                        text: root.view.identity !== "" ? root.lineStatus + " · " + root.view.identity.toUpperCase() : root.lineStatus
                    }

                    MicAlert {
                        id: stripAlert

                        width: parent.width
                        rows: stripAudio
                    }

                    Title {
                        width: parent.width
                    }

                    Artist {
                        width: parent.width
                    }

                    Progress {
                        width: parent.width
                    }
                }

                Item {
                    id: stripSide

                    anchors {
                        left: stripInfo.right
                        leftMargin: Theme.d(root.stripColumnGapPx)
                        right: parent.right
                        top: parent.top
                        bottom: parent.bottom
                    }

                    Column {
                        anchors {
                            left: parent.left
                            right: parent.right
                            verticalCenter: parent.verticalCenter
                        }
                        spacing: Theme.d(root.bodyGapPx)

                        // The rows take their room from the spectrum, which keeps the mockup's height without them.
                        SegBars {
                            width: parent.width
                            height: Math.min(Theme.d(root.stripSpectrumPx), stripSide.height - parent.spacing
                                - stripControls.height - (stripAudio.visible ? parent.spacing + stripAudio.height : 0))
                            levels: spectrum.levels
                            cellH: root.spectrumCellPx
                            gap: root.spectrumGapPx
                        }

                        Controls {
                            id: stripControls

                            anchors.horizontalCenter: parent.horizontalCenter
                            volume: root.audio ? stripAudio.controlVolume : ""
                        }

                        AudioRows {
                            id: stripAudio

                            width: parent.width
                            visible: root.audio
                        }
                    }
                }
            }
        }
    }

    Component {
        id: compact

        Item {
            Column {
                anchors {
                    left: parent.left
                    leftMargin: Theme.d(root.compactSidePx)
                    right: parent.right
                    rightMargin: Theme.d(root.compactSidePx)
                    verticalCenter: parent.verticalCenter
                }
                spacing: Theme.d(root.compactGapPx)

                Item {
                    width: parent.width
                    height: Math.max(compactArt.height, compactInfo.height)

                    MediaArt {
                        id: compactArt

                        anchors.verticalCenter: parent.verticalCenter
                        sidePx: root.compactArtPx
                    }

                    Column {
                        id: compactInfo

                        anchors {
                            left: compactArt.right
                            leftMargin: Theme.d(root.artGapPx)
                            right: compactControls.left
                            rightMargin: Theme.d(root.artGapPx)
                            verticalCenter: parent.verticalCenter
                        }
                        spacing: Theme.d(root.infoGapPx)

                        StatusLine {
                            width: parent.width
                            text: root.lineStatus
                        }

                        Title {
                            width: parent.width
                            textPx: root.compactTitlePx
                        }

                        Artist {
                            width: parent.width
                        }
                    }

                    Controls {
                        id: compactControls

                        anchors {
                            right: parent.right
                            verticalCenter: parent.verticalCenter
                        }
                        volume: root.audio ? compactAudio.controlVolume : ""
                    }
                }

                Progress {
                    width: parent.width
                }

                MicAlert {
                    width: parent.width
                    rows: compactAudio
                }
            }

            // The band has no room for two rows, so they stay hidden and only tell the alert which apps record. Hidden,
            // they never read the input's level.
            AudioRows {
                id: compactAudio

                visible: false
            }
        }
    }

    Loader {
        anchors.fill: parent
        sourceComponent: root.style === root.stackStyle ? stack : root.style === root.stripStyle ? strip : compact
    }
}
