import QtQuick
import Quickshell
import Quickshell.Wayland
import GhostKit
import "../kit/GhostKit/lib/scale.mjs" as Scale

Scope {
    id: root

    // KWin derives the window type from the namespace: "desktop" keeps the surface in its desktop
    // layer and visible under Show Desktop. Kept here so a move to another layer is a one-line edit.
    readonly property int surfaceLayer: WlrLayer.Bottom
    readonly property string surfaceNamespace: "desktop"

    readonly property real marginPx: 12
    readonly property int snapshotDelayMs: 3000

    readonly property string galleryView: "gallery"

    readonly property bool debug: Quickshell.env("GHOST_DEBUG") === "1"
    readonly property string viewOverride: Quickshell.env("GHOST_VIEW") || ""
    readonly property string snapshotPath: Quickshell.env("GHOST_SNAPSHOT") || ""
    readonly property string forcedUText: Quickshell.env("GHOST_FORCE_U") || ""
    // Number() rather than parseFloat() so a value with trailing junk is rejected, not truncated.
    readonly property real forcedU: Number(forcedUText)
    readonly property bool hasForcedU: Number.isFinite(forcedU) && forcedU > 0

    readonly property var mainScreen: Scale.pickScreen(Quickshell.screens, Data.config ? Data.config.display.model : "")
    readonly property string mainScreenName: mainScreen ? mainScreen.name : ""
    // Only the main screen's window shows the dashboard; the others show the dot grid alone, which
    // draws nothing after its first frame.
    readonly property bool covered: (Data.topic("desktop").value?.covered_outputs ?? []).includes(mainScreenName)
    // True while the greeting hides the dashboard, which is then hidden exactly as if windows covered it.
    property bool greetingHides: false
    readonly property bool hidden: covered || greetingHides

    // The first evaluation of the name's binding counts as a change, so this also logs at start.
    onMainScreenNameChanged: console.info("desktop on " + mainScreenName)
    onCoveredChanged: console.info(covered ? "dashboard covered" : "dashboard visible")
    Component.onCompleted: {
        if (forcedUText !== "" && !hasForcedU) {
            console.warn("GHOST_FORCE_U must be a positive number, got '" + forcedUText + "'; using the screen width");
        }
        if (viewOverride !== "" && viewOverride !== galleryView) {
            console.warn("GHOST_VIEW must be '" + galleryView + "', got '" + viewOverride + "'; ignoring it");
        }
    }

    // While covered the daemon collects only what the bar needs. The frame clock keeps running for the
    // bar's clock: the dashboard's widgets are destroyed, so they hold it no longer.
    Binding {
        target: Data.hostControl
        property: "covered"
        value: root.hidden
    }

    Component {
        id: galleryComponent

        Gallery {}
    }

    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: window

            required property ShellScreen modelData
            readonly property bool isMain: modelData === root.mainScreen
            property bool snapshotScheduled: false

            screen: modelData
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }
            exclusionMode: ExclusionMode.Ignore
            color: Theme.ground
            WlrLayershell.layer: root.surfaceLayer
            WlrLayershell.namespace: root.surfaceNamespace
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            // The input region is the view's claiming ClickAreas, its tab strips among them, and nothing
            // else, so every other click passes through to the desktop. When the cursor motion is
            // enabled, the window leaves its input region unmasked so it can follow the pointer.
            mask: Theme.dotMotion.pointer ? null : interactiveMask

            Region {
                id: interactiveMask

                regions: inputRegions.instances
            }

            Variants {
                id: inputRegions

                model: window.isMain && viewLoader.item && viewLoader.item.inputRects ? viewLoader.item.inputRects : []

                Region {
                    required property var modelData

                    x: modelData.x
                    y: modelData.y
                    width: modelData.width
                    height: modelData.height
                }
            }

            // RestoreNone: when the main screen moves, the old main window's binding must not put u
            // back to its default after the new one has set it.
            Binding {
                when: window.isMain
                restoreMode: Binding.RestoreNone
                target: Theme.hostControl
                property: "u"
                // The screen's width is the width this full-screen surface is configured to. The window's
                // own width passes through 100, 0 and 500 first, and every cell grid built at those units
                // is thrown away (measured: about 26 000 transient rectangles and 500 MB more RSS).
                value: root.hasForcedU ? root.forcedU : Scale.designUnit(window.modelData.width)
            }

            // The window's own contentItem is created outside the QML engine and refuses grabToImage(),
            // so the snapshot grabs this item instead.
            Item {
                id: content

                anchors.fill: parent

                // The window colour is not part of the grabbed item, so a snapshot would otherwise
                // have a transparent ground.
                Rectangle {
                    anchors.fill: parent
                    visible: root.snapshotPath !== ""
                    color: Theme.ground
                }

                // Passive, so it follows the pointer over widgets without taking their hover or clicks; only
                // the cursor motion of the dot grid reads it.
                HoverHandler {
                    id: pointerHover

                    enabled: Theme.dotMotion.pointer
                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                }

                DotGrid {
                    anchors.fill: parent
                    pointerInside: pointerHover.hovered
                    pointerAt: pointerHover.point.position
                }

                // Inactive while covered, so every widget is destroyed and gives back its wants and clock
                // holds; a print under way ends with its PrintSwap. Uncovering creates a PrintSwap, which
                // shows the current view without a print: windows moving away is not a view change.
                Loader {
                    id: viewLoader

                    anchors.fill: parent
                    active: window.isMain && !root.hidden
                    sourceComponent: root.viewOverride === root.galleryView ? galleryComponent : layoutComponent
                }

                Component {
                    id: layoutComponent

                    PrintSwap {
                        layout: Data.layout
                        designWidth: Scale.DESIGN_WIDTH
                        // From the screen rather than the window, whose size passes through provisional
                        // values first and would rebuild every widget at each of them.
                        designHeight: Scale.designHeight(window.modelData.width, window.modelData.height)
                    }
                }

                Scanlines {
                    anchors.fill: parent
                    visible: window.isMain
                }

                Loader {
                    anchors {
                        top: parent.top
                        right: parent.right
                        margins: Theme.d(root.marginPx)
                    }
                    active: root.debug && window.isMain
                    sourceComponent: PerfOverlay {}
                }
            }

            Timer {
                id: snapshotTimer

                interval: root.snapshotDelayMs
                onTriggered: content.grabToImage(result => {
                    if (result.saveToFile(root.snapshotPath)) {
                        console.info("snapshot saved " + root.snapshotPath);
                    } else {
                        console.warn("snapshot not written to " + root.snapshotPath);
                    }
                })
            }

            // perf is in every subscription and arrives on every daemon tick, so its first value marks
            // the first round of data.
            Connections {
                target: Data.topic("perf")
                enabled: window.isMain && root.snapshotPath !== "" && !window.snapshotScheduled

                function onValueChanged() {
                    window.snapshotScheduled = true;
                    snapshotTimer.start();
                }
            }
        }
    }
}
