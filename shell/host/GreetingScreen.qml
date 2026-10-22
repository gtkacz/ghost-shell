import QtQuick
import Quickshell
import Quickshell.Wayland
import GhostKit
import "../kit/GhostKit/lib/greeting.mjs" as GreetingLib
import "../kit/GhostKit/lib/theme.mjs" as ThemeLib
import "../kit/GhostKit/lib/views.mjs" as ViewsLib

// One screen of the greeting (spec §7), drawn from the frame Greeting computes: every screen gets the ground, the
// dots and the scan pass, and the main screen the tube, the roll band and the typed lines too. Above the scan edge
// nothing at all is drawn, so the windows or the dashboard under the overlay show through.
PanelWindow {
    id: window

    required property ShellScreen modelData
    property bool isMain: false
    // GreetingLib.frameAt()'s frame, and the plan it came from, whose lines hold the text being typed.
    property var frame: null
    property var plan: null
    property bool playing: false
    property string snapshotPath: ""
    property int snapshotDelayMs: 0

    signal skipRequested

    // Design pixels, percent and ems, from the reference mockup's greeting (CSS lines 56–63, 248 and 329–332).
    readonly property real titlePx: 56
    readonly property real titleBoxEm: 1.2
    readonly property real subtitlePx: 13
    readonly property real subtitleBoxEm: 1.4
    readonly property real subtitleGapPx: 14
    readonly property real subtitleTrackingEm: 0.25
    readonly property real cursorWidthEm: 0.5
    readonly property real cursorHeightEm: 0.82
    readonly property real cursorDropEm: 0.04
    readonly property real cursorGapPx: 4
    readonly property real fitMarginPx: 12
    readonly property real tubeLinePx: 2
    readonly property real glowPercent: 10
    readonly property real rollHeightPx: 140
    readonly property real rollFaintAt: 0.6
    readonly property real rollFaintPercent: 4.5
    readonly property real rollBrightAt: 0.85
    readonly property real rollBrightPercent: 7
    // The same line as PrintSwap's, which the dashboard's own view changes print with.
    readonly property real scanlinePx: 2
    readonly property real scanlineOpacity: 55
    readonly property int percent: 100

    // Bound rather than read once: the surface reports a provisional ratio before it is configured.
    readonly property real pixelRatio: QsWindow.window ? QsWindow.window.devicePixelRatio : 1
    readonly property real physicalHeight: Math.round(height * pixelRatio)
    // Logical pixels, on whole physical ones: the scan edge, above which the overlay is transparent, and the
    // bottom of the printed dots.
    readonly property real scanEdge: frame && frame.scanStep !== null ? ViewsLib.revealRows(frame.scanStep, GreetingLib.SCAN_STEPS, physicalHeight) / pixelRatio : 0
    readonly property real dotsBottom: frame ? ViewsLib.revealRows(frame.dotStep, GreetingLib.DOT_STEPS, physicalHeight) / pixelRatio : 0

    readonly property var tube: isMain && frame ? frame.tube : null
    readonly property bool typed: isMain && frame !== null && plan !== null && plan.lines !== null
    // Plan 2c call 11: line 1 keeps its size unless the whole line, cursor included, is wider than the screen less
    // its margins. It is measured from the full line once it is known, so the size never changes while it types.
    readonly property real titleFit: typed ? Math.min(1, (width - 2 * Theme.d(fitMarginPx)) / (titleMetrics.advanceWidth + Theme.d(cursorGapPx + cursorWidthEm * titlePx))) : 1
    readonly property real cursorOpacity: frame ? FrameClock.blinkSteps[Math.floor(frame.cursorPhaseMs / FrameClock.blinkStepMs) % FrameClock.blinkSteps.length] : 0

    screen: modelData
    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    color: "transparent"
    WlrLayershell.layer: WlrLayer.Overlay
    // KWin types a layer surface by its namespace, and its open effects fade some types in. Under the lock the fade
    // waits for the first paint after the unlock and so shows the windows; Utility is the one type no effect here
    // fades (docs/superpowers/notes/2026-10-22-greeting-handoff-spike.md).
    WlrLayershell.namespace: "utility"
    WlrLayershell.keyboardFocus: playing ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

    TextMetrics {
        id: titleMetrics

        text: window.typed ? window.plan.lines[0].text : ""
        font {
            family: Theme.font
            pixelSize: Theme.d(window.titlePx)
            weight: Font.Bold
        }
    }

    // One line of typed text, one Text per character so each burns in on its own time.
    component TypedLine: Item {
        id: typedLine

        // GreetingLib's typed line and the frame's view of it.
        property var line: null
        property var shown: null
        property real pixelSize
        property real boxEm
        property string family
        property int weight
        property real letterSpacing: 0
        property color textColor
        property color nameColor
        // Characters from this index on are in nameColor.
        property int nameStart: -1
        property bool hasCursor: false

        readonly property int shownCount: shown ? shown.shown : 0
        // Every character sits on one baseline, whichever font draws it.
        readonly property real baselineY: (height - metrics.height) / 2 + metrics.ascent

        width: parent.width
        height: pixelSize * boxEm

        FontMetrics {
            id: metrics

            font {
                family: typedLine.family
                pixelSize: typedLine.pixelSize
                weight: typedLine.weight
            }
        }

        // Centred, so it re-centres as it grows.
        Row {
            x: (typedLine.width - width) / 2

            // The whole line is created once, when it is typed; characters not yet typed take no room.
            Repeater {
                model: typedLine.line ? typedLine.line.text.length : 0

                Text {
                    required property int index

                    visible: index < typedLine.shownCount
                    y: typedLine.baselineY - baselineOffset
                    text: typedLine.line.text[index]
                    textFormat: Text.PlainText
                    // Burn-in: from burn to the character's own colour as its burn progress goes from 0 to 1.
                    color: visible ? Qt.tint(index >= typedLine.nameStart && typedLine.nameStart >= 0 ? typedLine.nameColor : typedLine.textColor, ThemeLib.withAlpha(Theme.burn, (1 - typedLine.shown.burn[index]) * window.percent)) : "transparent"
                    font {
                        family: typedLine.family
                        pixelSize: typedLine.pixelSize
                        weight: typedLine.weight
                        letterSpacing: typedLine.letterSpacing
                    }
                }
            }

            Item {
                visible: typedLine.hasCursor
                width: Theme.d(window.cursorGapPx) + cursor.width
                height: typedLine.height

                Rectangle {
                    id: cursor

                    x: Theme.d(window.cursorGapPx)
                    y: typedLine.baselineY + window.cursorDropEm * typedLine.pixelSize - height
                    width: window.cursorWidthEm * typedLine.pixelSize
                    height: window.cursorHeightEm * typedLine.pixelSize
                    color: Theme.signal
                    opacity: window.cursorOpacity
                }
            }
        }
    }

    // The window's own contentItem is created outside the QML engine and refuses grabToImage(), so the snapshot
    // grabs this item instead.
    Item {
        id: content

        anchors.fill: parent
        focus: true
        // Taken whatever the key, so none reaches a window while the greeting has the keyboard.
        Keys.onPressed: event => {
            event.accepted = true;
            if (window.playing) {
                window.skipRequested();
            }
        }

        // Everything is drawn in here, cut at the scan edge, and placed against the window, so it stays where it is
        // on screen while the cut moves down.
        Item {
            id: below

            y: window.scanEdge
            width: parent.width
            height: parent.height - y
            clip: true

            Item {
                y: -below.y
                width: window.width
                height: window.height

                Rectangle {
                    anchors.fill: parent
                    color: Theme.ground
                }

                Item {
                    width: parent.width
                    height: window.dotsBottom
                    clip: true

                    DotGrid {
                        width: window.width
                        height: window.height
                    }
                }

                Rectangle {
                    visible: window.tube !== null
                    y: window.tube ? window.tube.glowInset * parent.height : 0
                    width: parent.width
                    height: window.tube ? (1 - 2 * window.tube.glowInset) * parent.height : 0
                    color: ThemeLib.withAlpha(Theme.signal, window.glowPercent)
                    opacity: window.tube ? window.tube.glowOpacity : 0
                }

                Rectangle {
                    visible: window.tube !== null
                    width: window.tube ? parent.width * window.tube.lineScaleX : 0
                    height: window.tube ? Theme.d(window.tubeLinePx) * window.tube.lineScaleY : 0
                    x: (parent.width - width) / 2
                    y: (parent.height - height) / 2
                    color: Theme.burn
                    opacity: window.tube ? window.tube.lineOpacity : 0
                }

                Rectangle {
                    visible: window.isMain && window.frame !== null && window.frame.rollTop !== null
                    y: visible ? Theme.d(window.frame.rollTop) : 0
                    width: parent.width
                    height: Theme.d(window.rollHeightPx)
                    gradient: Gradient {
                        GradientStop {
                            position: 0
                            color: "transparent"
                        }
                        GradientStop {
                            position: window.rollFaintAt
                            color: ThemeLib.withAlpha(Theme.hi, window.rollFaintPercent)
                        }
                        GradientStop {
                            position: window.rollBrightAt
                            color: ThemeLib.withAlpha(Theme.hi, window.rollBrightPercent)
                        }
                        GradientStop {
                            position: 1
                            color: "transparent"
                        }
                    }
                }

                Column {
                    visible: window.typed
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width
                    spacing: Theme.d(window.subtitleGapPx)

                    TypedLine {
                        line: window.typed ? window.plan.lines[0] : null
                        shown: window.typed ? window.frame.lines[0] : null
                        pixelSize: Theme.d(window.titlePx) * window.titleFit
                        boxEm: window.titleBoxEm
                        family: Theme.font
                        weight: Font.Bold
                        textColor: Theme.hi
                        nameColor: Theme.signal
                        nameStart: window.typed ? window.plan.nameStart : -1
                        hasCursor: window.typed && window.frame.cursorLine === 0
                    }

                    TypedLine {
                        line: window.typed ? window.plan.lines[1] : null
                        shown: window.typed ? window.frame.lines[1] : null
                        pixelSize: Theme.d(window.subtitlePx)
                        boxEm: window.subtitleBoxEm
                        family: Theme.fontCjk
                        weight: Font.Medium
                        letterSpacing: window.subtitleTrackingEm * pixelSize
                        textColor: Theme.signal
                        nameColor: Theme.signal
                        hasCursor: window.typed && window.frame.cursorLine === 1
                    }
                }

                Rectangle {
                    visible: window.frame !== null && window.frame.scanStep !== null
                    y: window.scanEdge
                    width: parent.width
                    height: Math.max(1, Math.round(Theme.d(window.scanlinePx) * window.pixelRatio)) / window.pixelRatio
                    color: ThemeLib.withAlpha(Theme.hi, window.scanlineOpacity)
                }

                Scanlines {
                    anchors.fill: parent
                    visible: window.isMain
                }
            }
        }

        MouseArea {
            anchors.fill: parent
            enabled: window.playing
            onPressed: window.skipRequested()
        }
    }

    Connections {
        id: firstFrame

        target: content.Window.window
        enabled: target !== null

        function onFrameSwapped() {
            firstFrame.enabled = false;
            console.info("greeting first frame at " + Date.now());
        }
    }

    Timer {
        interval: window.snapshotDelayMs
        running: window.isMain && window.snapshotPath !== ""
        onTriggered: content.grabToImage(result => {
            if (result.saveToFile(window.snapshotPath)) {
                console.info("greeting snapshot saved " + window.snapshotPath);
            } else {
                console.warn("greeting snapshot not written to " + window.snapshotPath);
            }
        })
    }
}
