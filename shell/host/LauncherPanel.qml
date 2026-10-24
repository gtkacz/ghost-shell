import QtQuick
import Quickshell
import Quickshell.Wayland
import GhostKit
import "../kit/GhostKit/lib/layout.mjs" as Layout
import "../kit/GhostKit/lib/theme.mjs" as ThemeLib
import "../kit/GhostKit/lib/views.mjs" as ViewsLib

// The launcher's drop panel under (or over) the bar, at the screen's left edge, on a surface the size of the panel. It
// is on the Overlay layer so it opens over a fullscreen window, and closes when it loses keyboard focus, which the
// outside catcher on the Top layer could not do over a fullscreen window.
Scope {
    id: root

    // Design pixels, from the launcher mockup's .panel, .dA, .search and .power.
    readonly property real panelWidthPx: 392
    readonly property real panelHeightPx: 560
    readonly property real fillPct: 94
    readonly property real searchMarginTopPx: 2
    readonly property real searchMarginSidePx: 14
    readonly property real searchMarginBottomPx: 4
    readonly property real searchHeightPx: 30
    readonly property real searchPadPx: 8
    readonly property real promptPx: 10
    readonly property real queryPx: 13
    readonly property real placeholderPx: 11
    readonly property real cursorWidthPx: 7
    readonly property real cursorHeightPx: 14
    readonly property real powerPadTopPx: 8
    readonly property real powerPadSidePx: 10
    readonly property real powerPadBottomPx: 12
    readonly property real powerGapPx: 2
    readonly property real powerButtonPadTopPx: 6
    readonly property real powerButtonPadBottomPx: 5
    readonly property real powerGlyphPx: 14
    readonly property real powerLabelPx: 8.5
    readonly property real powerLabelGapPx: 4
    // The reveal: the same line the dashboard's print draws at its moving edge, in ten held steps.
    readonly property int revealSteps: 10
    readonly property int revealStepMs: 18
    readonly property real scanlinePx: 2
    readonly property real scanlineOpacity: 55
    // Letter spacing in em.
    readonly property real queryTracking: 0.04
    readonly property real placeholderTracking: 0.08
    readonly property real powerTracking: 0.1
    readonly property string placeholderText: "SEARCH APPS"
    readonly property string armedLabel: "AGAIN"

    required property var screen

    LazyLoader {
        active: Launcher.open && root.screen !== null

        PanelWindow {
            id: window

            readonly property bool atBottom: Data.config !== null && Data.config.bar.position === "bottom"
            // Whole device pixels, so the panel's edge meets the bar's.
            readonly property real bandPx: Math.round(Theme.d(Layout.BAR_BAND_PX) * devicePixelRatio) / devicePixelRatio
            property int step: 1

            screen: root.screen
            anchors {
                top: !window.atBottom
                bottom: window.atBottom
                left: true
            }
            margins {
                top: window.atBottom ? 0 : window.bandPx
                bottom: window.atBottom ? window.bandPx : 0
            }
            implicitWidth: Theme.d(root.panelWidthPx)
            implicitHeight: Theme.d(root.panelHeightPx)
            exclusionMode: ExclusionMode.Ignore
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "utility"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

            Item {
                id: stage

                readonly property real fullHeight: Theme.d(root.panelHeightPx)
                readonly property real revealed: ViewsLib.revealRows(window.step, root.revealSteps, Math.round(fullHeight * window.devicePixelRatio)) / window.devicePixelRatio

                width: Theme.d(root.panelWidthPx)
                height: revealed
                y: window.atBottom ? parent.height - revealed : 0
                clip: true

                Item {
                    id: panelBox

                    // Read by every ScrambleText inside, so the header scrambles on each opening.
                    readonly property string firstWriteScope: FirstWrite.hostScope("launcher", "panel")

                    width: stage.width
                    height: stage.fullHeight
                    anchors {
                        top: window.atBottom ? undefined : parent.top
                        bottom: window.atBottom ? parent.bottom : undefined
                    }

                    Panel {
                        anchors.fill: parent
                        fill: ThemeLib.withAlpha(Theme.ground, root.fillPct)
                        label: "LAUNCH"
                        tagKey: "launch"
                        meta: Launcher.appCount + " APPS"
                        padding: 0

                        Column {
                            anchors.fill: parent

                            Item {
                                id: search

                                x: Theme.d(root.searchMarginSidePx)
                                y: Theme.d(root.searchMarginTopPx)
                                width: parent.width - Theme.d(2 * root.searchMarginSidePx)
                                height: Theme.d(root.searchHeightPx + root.searchMarginTopPx + root.searchMarginBottomPx)

                                Rectangle {
                                    y: Theme.d(root.searchMarginTopPx)
                                    width: parent.width
                                    height: Theme.d(root.searchHeightPx)
                                    color: Qt.alpha(Theme.signal, 0.03)
                                    border {
                                        width: Theme.d(1)
                                        color: Theme.hair
                                    }

                                    Glyph {
                                        id: prompt

                                        x: Theme.d(root.searchPadPx)
                                        anchors.verticalCenter: parent.verticalCenter
                                        name: "chevron-right"
                                        size: Theme.d(root.promptPx)
                                        color: Theme.signal
                                    }

                                    Text {
                                        id: queryText

                                        anchors {
                                            left: prompt.right
                                            leftMargin: Theme.d(root.searchPadPx)
                                            verticalCenter: parent.verticalCenter
                                        }
                                        text: Launcher.query
                                        textFormat: Text.PlainText
                                        color: Theme.hi
                                        font {
                                            family: Theme.font
                                            pixelSize: Theme.d(root.queryPx)
                                            letterSpacing: Theme.d(root.queryPx) * root.queryTracking
                                        }
                                    }

                                    Rectangle {
                                        x: queryText.x + queryText.width
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: Theme.d(root.cursorWidthPx)
                                        height: Theme.d(root.cursorHeightPx)
                                        color: Theme.signal
                                        opacity: FrameClock.blinkOpacity
                                    }

                                    Text {
                                        anchors {
                                            left: queryText.right
                                            leftMargin: Theme.d(root.cursorWidthPx + root.searchPadPx)
                                            verticalCenter: parent.verticalCenter
                                        }
                                        visible: Launcher.query === ""
                                        text: root.placeholderText
                                        color: Theme.dim
                                        font {
                                            family: Theme.font
                                            pixelSize: Theme.d(root.placeholderPx)
                                            letterSpacing: Theme.d(root.placeholderPx) * root.placeholderTracking
                                        }
                                    }

                                    Rectangle {
                                        anchors {
                                            right: parent.right
                                            rightMargin: Theme.d(root.searchPadPx)
                                            verticalCenter: parent.verticalCenter
                                        }
                                        width: escText.implicitWidth + Theme.d(12)
                                        height: Theme.d(16)
                                        color: "transparent"
                                        border {
                                            width: Theme.d(1)
                                            color: Theme.hair
                                        }

                                        Text {
                                            id: escText

                                            anchors.centerIn: parent
                                            text: "ESC"
                                            color: Theme.dim
                                            font {
                                                family: Theme.font
                                                pixelSize: Theme.d(9)
                                                weight: Font.Medium
                                                letterSpacing: Theme.d(9) * 0.08
                                            }
                                        }
                                    }
                                }
                            }

                            LauncherList {
                                width: parent.width
                                height: parent.height - search.height - powerRow.height
                            }

                            Item {
                                id: powerRow

                                width: parent.width
                                height: Theme.d(root.powerPadTopPx + root.powerPadBottomPx + root.powerButtonPadTopPx + root.powerButtonPadBottomPx + root.powerGlyphPx + root.powerLabelGapPx + 2 + root.powerLabelPx * 1.3)

                                Rectangle {
                                    width: parent.width
                                    height: Math.max(1, Math.round(Theme.d(1)))
                                    color: Theme.hair2
                                }

                                Row {
                                    x: Theme.d(root.powerPadSidePx)
                                    y: Theme.d(root.powerPadTopPx)
                                    spacing: Theme.d(root.powerGapPx)

                                    Repeater {
                                        model: Launcher.powerActions

                                        Item {
                                            id: button

                                            required property var modelData
                                            required property int index
                                            readonly property bool armed: Launcher.armed === modelData.id
                                            readonly property color tone: armed ? Theme.warn : Theme.signal

                                            width: (powerRow.width - Theme.d(2 * root.powerPadSidePx + (Launcher.powerActions.length - 1) * root.powerGapPx)) / Launcher.powerActions.length
                                            height: powerRow.height - Theme.d(root.powerPadTopPx + root.powerPadBottomPx)

                                            Rectangle {
                                                anchors.fill: parent
                                                color: "transparent"
                                                border {
                                                    width: Theme.d(1)
                                                    color: button.armed ? Theme.warn : (hover.hovered || Launcher.powerFocus === button.index ? Theme.hair : "transparent")
                                                }
                                            }

                                            Glyph {
                                                x: (parent.width - width) / 2
                                                y: Theme.d(root.powerButtonPadTopPx + 1)
                                                name: button.modelData.glyph
                                                size: Theme.d(root.powerGlyphPx)
                                                color: button.tone
                                            }

                                            Text {
                                                anchors {
                                                    horizontalCenter: parent.horizontalCenter
                                                    bottom: parent.bottom
                                                    bottomMargin: Theme.d(root.powerButtonPadBottomPx + 1)
                                                }
                                                text: button.armed ? root.armedLabel : button.modelData.label
                                                color: button.armed ? Theme.warn : Theme.dim
                                                font {
                                                    family: Theme.font
                                                    pixelSize: Theme.d(root.powerLabelPx)
                                                    letterSpacing: Theme.d(root.powerLabelPx) * root.powerTracking
                                                }
                                            }

                                            HoverHandler {
                                                id: hover

                                                cursorShape: Qt.PointingHandCursor
                                            }

                                            MouseArea {
                                                anchors.fill: parent
                                                onClicked: Launcher.press(button.modelData.id)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Above the panel, on the edge the reveal is moving.
                Rectangle {
                    y: window.atBottom ? 0 : parent.height - height
                    width: parent.width
                    height: Math.max(1, Math.round(Theme.d(root.scanlinePx) * window.devicePixelRatio)) / window.devicePixelRatio
                    color: ThemeLib.withAlpha(Theme.hi, root.scanlineOpacity)
                    visible: window.step < root.revealSteps
                }
            }

            Timer {
                interval: root.revealStepMs
                repeat: true
                running: window.step < root.revealSteps
                onTriggered: window.step += 1
            }

            Item {
                property bool wasActive: false

                focus: true
                // The surface takes no exclusive focus, so activating anything else, by a click or a new window, closes the launcher.
                Window.onActiveChanged: {
                    if (Window.active) {
                        wasActive = true;
                    } else if (wasActive) {
                        Launcher.close();
                    }
                }
                Keys.onPressed: event => {
                    const name = root.keyName(event.key);
                    if (name !== "") {
                        Launcher.key(name);
                    } else if (event.text.length === 1 && event.text >= " " && !(event.modifiers & (Qt.ControlModifier | Qt.MetaModifier | Qt.AltModifier))) {
                        Launcher.type(event.text);
                    } else {
                        return;
                    }
                    event.accepted = true;
                }
            }
        }
    }

    function keyName(key) {
        switch (key) {
        case Qt.Key_Escape:
            return "Escape";
        case Qt.Key_Return:
        case Qt.Key_Enter:
            return "Return";
        case Qt.Key_Backspace:
            return "Backspace";
        case Qt.Key_Tab:
            return "Tab";
        case Qt.Key_Backtab:
            return "Backtab";
        case Qt.Key_Up:
            return "Up";
        case Qt.Key_Down:
            return "Down";
        case Qt.Key_Left:
            return "Left";
        case Qt.Key_Right:
            return "Right";
        default:
            return "";
        }
    }
}
