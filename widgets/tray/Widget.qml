import QtQuick
import Quickshell.Bluetooth
import Quickshell.Services.Pipewire
import Quickshell.Services.SystemTray
import GhostKit

// The tray: the built-in network, Bluetooth, volume and KDE Connect icons, then the apps' status items, then a chevron.
// A click on a built-in or on the chevron opens KDE's own popup for it through the daemon.
Item {
    id: root

    anchors.fill: parent

    // Design pixels, from the reference mockup's .tray.
    readonly property real gapPx: 10
    readonly property string phoneTopic: "batteries"
    readonly property string linksTopic: "net.links"

    property var settings

    readonly property var network: TrayRules.network(Data.topic(linksTopic).value)
    readonly property var phone: TrayRules.kdeConnect(Data.topic(phoneTopic).value)
    readonly property var adapter: Bluetooth.defaultAdapter
    readonly property var bluetooth: TrayRules.bluetooth({
        enabled: adapter !== null && adapter.enabled
    })
    readonly property var sink: Pipewire.defaultAudioSink
    readonly property bool sinkReady: sink !== null && sink.ready && sink.audio !== null
    readonly property var volume: TrayRules.volume({
        volume: sinkReady ? sink.audio.volume : 0,
        muted: !sinkReady || sink.audio.muted
    })
    readonly property bool onBottomBar: Data.config !== null && Data.config.bar.position === "bottom"

    implicitWidth: row.implicitWidth

    function openPopup(applet) {
        Data.call("plasma.popup", {
            applet: applet
        });
    }

    Component.onCompleted: {
        Data.want(linksTopic);
        Data.want(phoneTopic);
    }
    Component.onDestruction: {
        Data.release(linksTopic);
        Data.release(phoneTopic);
    }

    // Quickshell binds a node's audio only while something holds it.
    PwObjectTracker {
        objects: root.sink !== null ? [root.sink] : []
    }

    Row {
        id: row

        height: parent.height
        spacing: Theme.d(root.gapPx)

        TrayItem {
            anchors.verticalCenter: parent.verticalCenter
            glyph: root.network.glyph
            dim: root.network.dim
            tip: "Network"
            onClicked: button => {
                if (button === Qt.LeftButton) {
                    root.openPopup(TrayRules.applets.network);
                }
            }
        }

        TrayItem {
            anchors.verticalCenter: parent.verticalCenter
            glyph: root.bluetooth.glyph
            dim: root.bluetooth.dim
            tip: "Bluetooth"
            onClicked: button => {
                if (button === Qt.LeftButton) {
                    root.openPopup(TrayRules.applets.bluetooth);
                }
            }
        }

        TrayItem {
            anchors.verticalCenter: parent.verticalCenter
            glyph: root.volume.glyph
            dim: root.volume.dim
            tip: root.volume.label
            onClicked: button => {
                if (button === Qt.LeftButton) {
                    root.openPopup(TrayRules.applets.volume);
                } else if (button === Qt.MiddleButton && root.sinkReady) {
                    root.sink.audio.muted = !root.sink.audio.muted;
                }
            }
            onScrolled: notches => {
                if (root.sinkReady) {
                    root.sink.audio.volume = TrayRules.stepVolume(root.sink.audio.volume, notches);
                }
            }
        }

        TrayItem {
            anchors.verticalCenter: parent.verticalCenter
            glyph: root.phone.glyph
            dim: root.phone.dim
            tip: "KDE Connect"
            onClicked: button => {
                if (button === Qt.LeftButton) {
                    root.openPopup(TrayRules.applets.kdeconnect);
                }
            }
        }

        Repeater {
            model: SystemTray.items

            SniItem {
                required property var modelData

                anchors.verticalCenter: parent.verticalCenter
                item: modelData
            }
        }

        TrayItem {
            anchors.verticalCenter: parent.verticalCenter
            glyph: root.onBottomBar ? "chevron-up" : "chevron-down"
            tip: "More"
            onClicked: button => {
                if (button === Qt.LeftButton) {
                    root.openPopup(TrayRules.applets.overflow);
                }
            }
        }
    }
}
