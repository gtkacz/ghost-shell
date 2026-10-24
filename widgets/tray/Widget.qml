import QtQuick
import Quickshell
import Quickshell.Bluetooth
import Quickshell.Services.Pipewire
import Quickshell.Services.SystemTray
import GhostKit

// The tray: the built-in network, Bluetooth, volume, KDE Connect and brightness icons, then the apps' status items, then a chevron.
// A click on a built-in but the sun, or on the chevron, opens KDE's own popup for it through the daemon; the sun opens our own.
Item {
    id: root

    anchors.fill: parent

    // Design pixels, from the reference mockup's .tray.
    readonly property real gapPx: 10
    readonly property string phoneTopic: "batteries"
    readonly property string linksTopic: "net.links"
    readonly property string brightnessTopic: "brightness"
    readonly property real popupGapPx: 6
    readonly property string notificationsTopic: "notifications"

    property var settings

    readonly property var network: TrayRules.network(Data.topic(linksTopic).value)
    readonly property var phone: TrayRules.kdeConnect(Data.topic(phoneTopic).value)
    readonly property var adapter: Bluetooth.defaultAdapter
    readonly property var bluetooth: TrayRules.bluetooth({
        enabled: adapter !== null && adapter.enabled
    })
    readonly property var displaysTopic: Data.topic(brightnessTopic)
    readonly property var displays: displaysTopic.value !== null ? displaysTopic.value.displays : []
    readonly property var brightness: TrayRules.brightness({
        nightLight: TrayRules.nightLightSwitch(displaysTopic.value !== null ? displaysTopic.value.night_light : null).on
    })
    property bool brightnessOpen: false
    readonly property var sink: Pipewire.defaultAudioSink
    readonly property bool sinkReady: sink !== null && sink.ready && sink.audio !== null
    readonly property var volume: TrayRules.volume({
        volume: sinkReady ? sink.audio.volume : 0,
        muted: !sinkReady || sink.audio.muted
    })
    readonly property bool onBottomBar: Data.config !== null && Data.config.bar.position === "bottom"

    implicitWidth: row.implicitWidth

    // The value the wheel sent last, until the topic shows it: notches faster than the round trip add up from it.
    readonly property int wheelHoldMs: 1500
    property int wheelPending: -1
    readonly property int firstPublished: displays.length > 0 ? displays[0].brightness : -1

    onFirstPublishedChanged: wheelPending = TrayRules.pendingAfter(wheelPending, firstPublished)

    function stepFirstDisplay(notches) {
        if (displays.length === 0) {
            return;
        }
        const first = displays[0];
        wheelPending = TrayRules.wheelTarget(wheelPending, first.brightness, first.max, notches);
        wheelHold.restart();
        Data.call("brightness.set", {
            display: first.id,
            value: wheelPending
        });
    }

    Timer {
        id: wheelHold

        interval: root.wheelHoldMs
        onTriggered: root.wheelPending = -1
    }

    // The centre places itself from the bell's rect in screen coordinates; both bars span the screen from its edge, as in BarPopup.
    function placeBell() {
        const win = QsWindow.window;
        const rect = bell.mapToItem(null, 0, 0, bell.width, bell.height);
        const top = onBottomBar ? win.screen.height - win.margins.bottom - win.height + rect.y : win.margins.top + rect.y;
        Notifications.anchorRect = Qt.rect(win.margins.left + rect.x, top, rect.width, rect.height);
    }

    function openPopup(applet) {
        Data.call("plasma.popup", {
            applet: applet
        });
    }

    Component.onCompleted: {
        Data.want(linksTopic);
        Data.want(phoneTopic);
        Data.want(brightnessTopic);
        Data.want(notificationsTopic);
    }
    Component.onDestruction: {
        Data.release(linksTopic);
        Data.release(phoneTopic);
        Data.release(brightnessTopic);
        Data.release(notificationsTopic);
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

        TrayItem {
            id: sun

            anchors.verticalCenter: parent.verticalCenter
            glyph: root.brightness.glyph
            dim: root.brightness.dim
            tip: "Brightness"
            tipSuppressed: root.brightnessOpen
            onClicked: button => {
                if (button === Qt.LeftButton) {
                    root.brightnessOpen = !root.brightnessOpen;
                }
            }
            onScrolled: notches => root.stepFirstDisplay(notches)

            BarPopup {
                anchorItem: sun
                gap: Theme.d(root.popupGapPx)
                open: root.brightnessOpen
                catchOutside: true
                onDismissed: root.brightnessOpen = false
                content: Component {
                    BrightnessPopup {
                        snapshot: root.displaysTopic.value
                        onDisplaySet: (id, value) => Data.call("brightness.set", {
                            display: id,
                            value: value
                        })
                        onNightLightToggled: Data.call("nightlight.toggle", {})
                    }
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
            id: bell

            anchors.verticalCenter: parent.verticalCenter
            glyph: Notifications.bell.glyph
            glyphColour: Theme[Notifications.bell.colour]
            badge: Notifications.bell.badge
            badgeColour: Theme[Notifications.bell.badgeColour]
            tip: Notifications.bell.tip
            onClicked: button => {
                if (button === Qt.LeftButton) {
                    root.placeBell();
                    Notifications.toggle();
                } else if (button === Qt.MiddleButton) {
                    Notifications.toggleDnd();
                }
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
