import QtQuick
import QtCore
import org.kde.kirigami as Kirigami
import org.kde.kwin as KWin
import "GhostGate"
import "GhostGate/lib/gate.mjs" as GateLib
import "GhostGate/lib/theme.mjs" as ThemeLib

// Plan 3c Task 1's throwaway layout: only what the proofs need to see, in the shape spec §5.1 names.
KWin.TabBoxSwitcher {
    id: tabBox

    readonly property string marker: "SPIKE_MARKER"

    Component.onCompleted: console.info("spike-layout: root " + marker + " at " + Date.now())

    Instantiator {
        active: tabBox.visible

        delegate: Window {
            id: window

            property bool reportedFrame: false
            property int rowsReady: 0

            x: tabBox.screenGeometry.x
            y: tabBox.screenGeometry.y
            width: tabBox.screenGeometry.width
            height: tabBox.screenGeometry.height
            flags: Qt.ToolTip | Qt.BypassWindowManagerHint | Qt.FramelessWindowHint
            visibility: Window.FullScreen
            visible: true
            title: "ghost-shell switcher"
            color: "transparent"

            onFrameSwapped: {
                if (!reportedFrame) {
                    reportedFrame = true;
                    const own = KWin.Workspace.stackingOrder.find(candidate => candidate.caption === window.title);
                    if (own) {
                        own.skipsCloseAnimation = true;
                    }
                    console.info("spike-layout: open " + Date.now() + " marker " + tabBox.marker + " own-window " + (own ? "found" : "NOT FOUND"));
                }
            }

            // Declares no properties: a declared one would be written back to ghostd's file.
            Settings {
                id: snapshot

                location: StandardPaths.writableLocation(StandardPaths.GenericStateLocation) + "/ghost-shell/gate.ini"
            }

            Component.onCompleted: {
                Theme.load(GateLib.readSnapshot(key => snapshot.value(key)));
                Theme.fit(tabBox.screenGeometry.width);
                console.info("spike-layout: signal " + snapshot.value("colors/signal") + " themed " + Theme.signal + " icons-unit " + Theme.u);
            }

            Rectangle {
                anchors.fill: parent
                color: ThemeLib.withAlpha(Theme.ground, 60)
            }

            // Lists nothing on screen: each row reports what KWin's model gives and what Workspace can add to it.
            Item {
                visible: false

                Repeater {
                    id: rows

                    model: tabBox.model
                    onItemAdded: window.rowsReady++

                    delegate: Item {
                        required property string caption
                        required property var windowId
                        required property var icon
                        required property int index

                        Component.onCompleted: {
                            const wanted = String(windowId);
                            const found = KWin.Workspace.stackingOrder.find(candidate => String(candidate.internalId) === wanted);
                            console.info("spike-layout: row " + index + " id " + wanted + " caption [" + caption + "] desktop [" + (found ? found.desktopFileName : "NOT FOUND") + "] class [" + (found ? found.resourceClass : "NOT FOUND") + "]");
                        }
                    }
                }
            }

            Panel {
                id: panel

                anchors.centerIn: parent
                width: Theme.d(950)
                height: Theme.d(390)
                label: "SWITCH WINDOW"
                tagKey: "switch"
                meta: (tabBox.currentIndex + 1) + " / " + rows.count
                scrambles: false
                fill: ThemeLib.withAlpha(Theme.panel, 94)

                Kirigami.Icon {
                    id: firstIcon

                    width: Theme.d(32)
                    height: Theme.d(32)
                    source: window.rowsReady > 0 && rows.itemAt(0) ? rows.itemAt(0).icon : ""
                    layer.enabled: true
                    visible: false
                }

                Duotone {
                    width: firstIcon.width
                    height: firstIcon.height
                    source: firstIcon
                    shadowColor: "transparent"
                    lightColor: Theme.signal
                }

                Item {
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    width: Theme.d(528)
                    height: Theme.d(286)

                    KWin.WindowThumbnail {
                        anchors.fill: parent
                        wId: window.rowsReady > 0 && rows.itemAt(tabBox.currentIndex) ? rows.itemAt(tabBox.currentIndex).windowId : ""
                    }
                }
            }

            Scanlines {
                anchors.fill: panel
            }
        }
    }
}
