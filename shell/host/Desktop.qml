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

    readonly property string currentView: Data.config ? Data.config.dashboard.default_view : ""
    readonly property var mainScreen: Scale.pickScreen(Quickshell.screens, Data.config ? Data.config.display.model : "")

    Component.onCompleted: {
        if (forcedUText !== "" && !hasForcedU) {
            console.warn("GHOST_FORCE_U must be a positive number, got '" + forcedUText + "'; using the screen width");
        }
        if (viewOverride !== "" && viewOverride !== galleryView) {
            console.warn("GHOST_VIEW must be '" + galleryView + "', got '" + viewOverride + "'; ignoring it");
        }
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
            // The input region is the view's tab strips and nothing else, so every other click passes
            // through to the desktop. This is the one place it is built.
            mask: Region {
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
                target: Theme
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

                DotGrid {
                    anchors.fill: parent
                }

                Loader {
                    id: viewLoader

                    anchors.fill: parent
                    active: window.isMain
                    sourceComponent: root.viewOverride === root.galleryView ? galleryComponent : layoutComponent
                }

                Component {
                    id: layoutComponent

                    LayoutView {
                        layout: Data.layout
                        viewName: root.currentView
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

            Connections {
                target: Data
                enabled: window.isMain && root.snapshotPath !== "" && !window.snapshotScheduled

                function onTopicsChanged() {
                    window.snapshotScheduled = true;
                    snapshotTimer.start();
                }
            }
        }
    }
}
