import QtQuick
import QtCore
import org.kde.kwin as KWin
import "GhostGate"
import "GhostGate/lib/gate.mjs" as GateLib
import "GhostGate/lib/switcher.mjs" as Switcher
import "GhostGate/lib/theme.mjs" as ThemeLib

// The panel in GhostGate is the switcher; this only connects it to KWin. It runs inside KWin, so it builds nothing until
// the switcher is held open and leaves no window behind when it closes.
KWin.TabBoxSwitcher {
    id: tabBox

    Instantiator {
        active: tabBox.visible

        delegate: Window {
            id: window

            readonly property int backdropPct: 60
            property bool openingSeen: false

            x: tabBox.screenGeometry.x
            y: tabBox.screenGeometry.y
            width: tabBox.screenGeometry.width
            height: tabBox.screenGeometry.height
            flags: Qt.ToolTip | Qt.BypassWindowManagerHint | Qt.FramelessWindowHint
            // How the window finds itself in KWin's stacking order below.
            title: "ghost-shell switcher"
            color: "transparent"

            // Only the first frame's business, so a hold runs no script on its later frames.
            Connections {
                target: window
                enabled: !window.openingSeen

                function onFrameSwapped() {
                    window.openingSeen = true;
                    // The effects would otherwise keep the closed panel on screen, up to 600 ms with KWin's defaults. Only the
                    // closing can be stopped: they start the opening before a layout has drawn anything.
                    const own = KWin.Workspace.stackingOrder.find(candidate => candidate.caption === window.title);
                    if (own) {
                        own.skipsCloseAnimation = true;
                    }
                    // Debug, which qtlogging.ini turns off, so a hold writes nothing to the user's journal.
                    console.debug("switcher: open, signal " + Theme.signal);
                }
            }

            // Declares no properties: a declared one would be written back to ghostd's file.
            Settings {
                id: snapshot

                location: StandardPaths.writableLocation(StandardPaths.GenericStateLocation) + "/ghost-shell/gate.ini"
            }

            // In this order because KWin draws a window's first frame as soon as it is shown: shown earlier, that frame has the
            // last opening's theme, and the first opening's has the wrong size too.
            Component.onCompleted: {
                Theme.load(GateLib.readSnapshot(key => snapshot.value(key)));
                Theme.fit(tabBox.screenGeometry.width);
                content.active = true;
                window.visibility = Window.FullScreen;
            }

            Loader {
                id: content

                anchors.fill: parent
                active: false

                sourceComponent: Item {
                    Rectangle {
                        anchors.fill: parent
                        color: ThemeLib.withAlpha(Theme.ground, window.backdropPct)
                    }

                    SwitcherPanel {
                        id: panel

                        anchors.centerIn: parent
                        model: tabBox.model
                        currentIndex: tabBox.currentIndex
                        nameOf: (windowId, caption) => {
                            const wanted = String(windowId);
                            const found = KWin.Workspace.stackingOrder.find(candidate => String(candidate.internalId) === wanted);
                            return Switcher.appName(found ? found.desktopFileName : "", found ? found.resourceClass : "", caption);
                        }
                        onActivated: index => tabBox.model.activate(index)

                        KWin.WindowThumbnail {
                            anchors.fill: parent
                            wId: panel.currentWindowId ?? ""
                        }
                    }
                }
            }
        }
    }
}
